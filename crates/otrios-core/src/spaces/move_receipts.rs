// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded durable move outcomes. An operation ID is never an admission grant.
use super::*;
const MAX_RECEIPTS: usize = 1024;
const MAX_BYTES: usize = 2048;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MoveReceipt {
    pub version: u32,
    pub root: String,
    pub operation: String,
    pub actor: String,
    pub source: String,
    pub destination: String,
    pub request_sha256: Vec<u8>,
    pub generation: u64,
}
pub(super) fn digest(actor: &str, request: &Request) -> Result<Vec<u8>> {
    let Change::Move {
        operation,
        source,
        destination,
        app,
        collection,
        destination_collection,
        mode,
    } = &request.change
    else {
        return Err(Error::Invalid);
    };
    let mut bytes = b"OTriOS\0space-move-request\0v1\0".to_vec();
    for text in [actor, operation, source, destination, app, collection] {
        bytes.extend(
            u32::try_from(text.len())
                .map_err(|_| Error::Capacity)?
                .to_le_bytes(),
        );
        bytes.extend(text.as_bytes());
    }
    bytes.extend(request.version.to_le_bytes());
    bytes.extend(request.expected_generation.to_le_bytes());
    bytes.push(u8::from(destination_collection.is_some()));
    if let Some(cid) = destination_collection {
        bytes.extend(cid.as_bytes());
    }
    bytes.push(match mode {
        MoveMode::AppAndData => 0,
        MoveMode::AppOnly => 1,
        MoveMode::DataOnly => 2,
    });
    Ok(crate::canvas::hash(&bytes).to_vec())
}
impl Store {
    pub fn move_committed(
        &self,
        actor: &str,
        source: &str,
        destination: &str,
        operation: &str,
    ) -> Result<Option<u64>> {
        self.move_admin(actor, source, destination)?;
        if !valid_id(operation) {
            return Err(Error::Invalid);
        }
        let Some(receipt) = self.read_move_receipt(operation)? else {
            return Ok(None);
        };
        if receipt.source != source || receipt.destination != destination {
            return Ok(None);
        }
        Ok(Some(receipt.generation))
    }
    pub(super) fn move_admin(&self, actor: &str, source: &str, destination: &str) -> Result<()> {
        if !valid_identity(actor)
            || !valid_id(source)
            || !valid_id(destination)
            || source == destination
        {
            return Err(Error::Invalid);
        }
        let registry = self.registry()?;
        for id in [source, destination] {
            admin(
                registry
                    .spaces
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or(Error::Denied)?,
                actor,
            )?;
        }
        Ok(())
    }
    pub(super) fn replayed_move(&self, actor: &str, request: &Request) -> Result<bool> {
        let Change::Move {
            operation,
            source,
            destination,
            ..
        } = &request.change
        else {
            return Ok(false);
        };
        self.move_admin(actor, source, destination)?;
        let Some(receipt) = self.read_move_receipt(operation)? else {
            return Ok(false);
        };
        if receipt.actor != actor || receipt.request_sha256 != digest(actor, request)? {
            return Err(Error::Conflict);
        }
        Ok(true)
    }
    fn read_move_receipt(&self, operation: &str) -> Result<Option<MoveReceipt>> {
        if !valid_id(operation) {
            return Err(Error::Invalid);
        }
        let root = suffix(self.path(), ".moves");
        match fs::symlink_metadata(&root) {
            Ok(_) => directory(&root)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error(e)),
        }
        let Some(bytes) = read(
            &root.join(format!("{operation}.flex")),
            MAX_BYTES + 37,
            true,
        )?
        else {
            return Ok(None);
        };
        if bytes.len() < 40
            || !bytes.starts_with(b"OTSM1")
            || crate::canvas::hash(&bytes[37..]) != bytes[5..37]
        {
            return Err(Error::Invalid);
        }
        crate::flex_guard::validate(&bytes[37..], 4, MAX_BYTES, 32).map_err(|_| Error::Invalid)?;
        let receipt: MoveReceipt =
            flexbuffers::from_slice(&bytes[37..]).map_err(|_| Error::Invalid)?;
        self.validate_move_receipt(&receipt, self.current.registry.generation)?;
        if receipt.operation != operation {
            return Err(Error::Invalid);
        }
        Ok(Some(receipt))
    }
    pub(super) fn validate_move_receipt(&self, r: &MoveReceipt, generation: u64) -> Result<()> {
        if r.version != 1
            || r.root != self.current.root
            || !valid_id(&r.operation)
            || !valid_identity(&r.actor)
            || !valid_id(&r.source)
            || !valid_id(&r.destination)
            || r.source == r.destination
            || r.request_sha256.len() != 32
            || r.generation == 0
            || r.generation > generation
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub(super) fn move_receipt_capacity(&self) -> Result<PathBuf> {
        let root = suffix(self.path(), ".moves");
        if !root.exists() {
            backups::new_directory(&root)?;
            backups::sync_directory(self.path.parent().ok_or(Error::Invalid)?)?;
        }
        directory(&root)?;
        let mut count = 0;
        for (visited, entry) in fs::read_dir(&root).map_err(io_error)?.enumerate() {
            if visited >= MAX_RECEIPTS + 128 {
                return Err(Error::Capacity);
            }
            let entry = entry.map_err(io_error)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            private(&fs::symlink_metadata(entry.path()).map_err(io_error)?)?;
            if let Some(nonce) = name
                .strip_prefix(".spaces-")
                .and_then(|n| n.strip_suffix(".tmp"))
            {
                if !valid_id(nonce) {
                    return Err(Error::Invalid);
                }
                fs::remove_file(entry.path()).map_err(io_error)?;
                continue;
            }
            if !name.strip_suffix(".flex").is_some_and(valid_id) {
                return Err(Error::Invalid);
            }
            count += 1;
        }
        if count >= MAX_RECEIPTS {
            return Err(Error::Capacity);
        }
        Ok(root)
    }
    pub(super) fn publish_move_receipt(&self, receipt: &MoveReceipt) -> Result<()> {
        self.validate_move_receipt(receipt, self.current.registry.generation)?;
        if let Some(existing) = self.read_move_receipt(&receipt.operation)? {
            return if existing == *receipt {
                Ok(())
            } else {
                Err(Error::Conflict)
            };
        }
        let root = self.move_receipt_capacity()?;
        let payload = flexbuffers::to_vec(receipt).map_err(io_error)?;
        if payload.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        let mut bytes = b"OTSM1".to_vec();
        bytes.extend(crate::canvas::hash(&payload));
        bytes.extend(payload);
        write(&root.join(format!("{}.flex", receipt.operation)), &bytes)
    }
}
