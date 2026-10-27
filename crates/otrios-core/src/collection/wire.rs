// SPDX-License-Identifier: LGPL-3.0-only
//! Explicit bounded binary window control. This is independent of release hello;
//! the enclosing host must negotiate its state protocol before decoding bytes.
use super::*;
use crate::merkle;

const COMMAND: &[u8] = b"OTriOS\0collection-command\0v1\0";
const OFFER: &[u8] = b"OTriOS\0collection-offer\0v1\0";
const EVENT: &[u8] = b"OTriOS\0collection-event\0v1\0";
const MAX_COMMAND: usize = 1024;
const MAX_OFFER: usize = window::MAX_VIEW_BYTES + 256;

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if count > self.0.len() {
            return Err(Error::Invalid);
        }
        let (bytes, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().map_err(|_| Error::Invalid)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn flag(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Invalid),
        }
    }
    fn end(self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
}
fn shape(plan: &Selection) -> Result<()> {
    if plan.fields.is_empty()
        || plan.fields.len() > MAX_FIELDS
        || plan.fields.windows(2).any(|pair| pair[0] >= pair[1])
        || plan.record_limit == 0
        || usize::from(plan.record_limit) > MAX_WINDOW_RECORDS
        || plan.byte_limit == 0
        || plan.byte_limit as usize > MAX_WINDOW_BYTES
    {
        return Err(Error::Limit);
    }
    Ok(())
}
pub fn encode_command(command: &Command) -> Result<Vec<u8>> {
    let mut out = COMMAND.to_vec();
    match command {
        Command::Select { generation, plan } => {
            shape(plan)?;
            out.push(0);
            out.extend(generation.to_le_bytes());
            out.extend(plan.collection.to_le_bytes());
            out.extend(plan.schema);
            match plan.anchor {
                Anchor::Tail => out.push(0),
                Anchor::Before(cursor) => {
                    out.push(1);
                    out.extend(cursor.epoch.to_le_bytes());
                    out.extend(cursor.collection.to_le_bytes());
                    out.extend(cursor.key.to_le_bytes());
                }
                Anchor::Around(key) => {
                    out.push(2);
                    out.extend(key.to_le_bytes());
                }
            }
            out.extend(plan.record_limit.to_le_bytes());
            out.extend(plan.byte_limit.to_le_bytes());
            out.push(plan.fields.len() as u8);
            for field in &plan.fields {
                out.extend(field.to_le_bytes());
            }
        }
        Command::Acknowledge {
            generation,
            cursor,
            root,
        } => {
            out.push(1);
            out.extend(generation.to_le_bytes());
            out.extend(cursor.epoch.to_le_bytes());
            out.extend(cursor.lease.to_le_bytes());
            out.extend(cursor.sequence.to_le_bytes());
            out.push(u8::from(root.is_some()));
            if let Some(root) = root {
                out.extend(root);
            }
        }
        Command::Resync { generation } => {
            out.push(2);
            out.extend(generation.to_le_bytes());
        }
        Command::Cancel { generation } => {
            out.push(3);
            out.extend(generation.to_le_bytes());
        }
    }
    Ok(out)
}
pub fn decode_command(bytes: &[u8]) -> Result<Command> {
    if bytes.len() > MAX_COMMAND {
        return Err(Error::Limit);
    }
    let mut r = Reader(bytes);
    if r.take(COMMAND.len())? != COMMAND {
        return Err(Error::Invalid);
    }
    let tag = r.u8()?;
    let generation = r.u64()?;
    let result = match tag {
        0 => {
            let collection = r.u64()?;
            let schema = r.array()?;
            let anchor = match r.u8()? {
                0 => Anchor::Tail,
                1 => Anchor::Before(Cursor {
                    epoch: r.u64()?,
                    collection: r.u64()?,
                    key: r.u64()?,
                }),
                2 => Anchor::Around(r.u64()?),
                _ => return Err(Error::Invalid),
            };
            let record_limit = r.u16()?;
            let byte_limit = r.u32()?;
            let count = usize::from(r.u8()?);
            if count == 0 || count > MAX_FIELDS {
                return Err(Error::Limit);
            }
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                fields.push(r.u64()?);
            }
            let plan = Selection {
                collection,
                schema,
                anchor,
                record_limit,
                byte_limit,
                fields,
            };
            shape(&plan)?;
            Command::Select { generation, plan }
        }
        1 => {
            let cursor = merkle::Cursor {
                epoch: r.u64()?,
                lease: r.u64()?,
                sequence: r.u64()?,
            };
            let root = if r.flag()? { Some(r.array()?) } else { None };
            Command::Acknowledge {
                generation,
                cursor,
                root,
            }
        }
        2 => Command::Resync { generation },
        3 => Command::Cancel { generation },
        _ => return Err(Error::Invalid),
    };
    r.end()?;
    Ok(result)
}
impl Offer {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.transfer.bytes().len() > window::MAX_VIEW_BYTES {
            return Err(Error::Limit);
        }
        let mut out = OFFER.to_vec();
        out.extend(self.epoch.to_le_bytes());
        out.extend(self.lease.to_le_bytes());
        out.extend(self.generation.to_le_bytes());
        out.extend(self.revision);
        out.push(u8::from(self.more_before));
        out.push(u8::from(self.more_after));
        out.extend((self.transfer.bytes().len() as u32).to_le_bytes());
        out.extend(self.transfer.bytes());
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_OFFER {
            return Err(Error::Limit);
        }
        let mut r = Reader(bytes);
        if r.take(OFFER.len())? != OFFER {
            return Err(Error::Invalid);
        }
        let epoch = r.u64()?;
        let lease = r.u64()?;
        let generation = r.u64()?;
        let revision = r.array()?;
        let more_before = r.flag()?;
        let more_after = r.flag()?;
        let size = r.u32()? as usize;
        if size > window::MAX_VIEW_BYTES {
            return Err(Error::Limit);
        }
        let transfer = merkle::Offer::from_bytes(r.take(size)?)?;
        r.end()?;
        if transfer.cursor.epoch != epoch
            || transfer.cursor.lease != lease
            || generation == 0
            || lease == 0
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            epoch,
            lease,
            generation,
            revision,
            more_before,
            more_after,
            transfer,
        })
    }
}

impl Event {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = EVENT.to_vec();
        match self {
            Self::View(offer) => {
                out.push(0);
                let bytes = offer.to_bytes()?;
                out.extend((bytes.len() as u32).to_le_bytes());
                out.extend(bytes);
            }
            Self::Rejected { generation, status } => {
                out.push(1);
                out.extend(generation.to_le_bytes());
                out.push(*status as u8);
            }
            Self::Revoked {
                epoch,
                generation,
                collection,
            } => {
                out.push(2);
                out.extend(epoch.to_le_bytes());
                out.extend(generation.to_le_bytes());
                out.extend(collection.to_le_bytes());
            }
        }
        if out.len() > MAX_EVENT_BYTES {
            return Err(Error::Limit);
        }
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_EVENT_BYTES {
            return Err(Error::Limit);
        }
        let mut r = Reader(bytes);
        if r.take(EVENT.len())? != EVENT {
            return Err(Error::Invalid);
        }
        let result = match r.u8()? {
            0 => {
                let size = r.u32()? as usize;
                Self::View(Offer::from_bytes(r.take(size)?)?)
            }
            1 => {
                let generation = r.u64()?;
                let status = match r.u8()? {
                    0 => Status::Invalid,
                    1 => Status::Limit,
                    2 => Status::Schema,
                    3 => Status::Forbidden,
                    4 => Status::Stale,
                    5 => Status::AnchorExpired,
                    6 => Status::Backpressure,
                    7 => Status::ResyncRequired,
                    8 => Status::Unavailable,
                    _ => return Err(Error::Invalid),
                };
                Self::Rejected { generation, status }
            }
            2 => Self::Revoked {
                epoch: r.u64()?,
                generation: r.u64()?,
                collection: r.u64()?,
            },
            _ => return Err(Error::Invalid),
        };
        r.end()?;
        Ok(result)
    }
}
