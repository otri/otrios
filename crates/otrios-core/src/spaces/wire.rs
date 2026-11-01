// SPDX-License-Identifier: LGPL-3.0-only
//! Management v1 uses a separate bounded channel on the negotiated shared session.
use super::*;
const COMMAND: &[u8] = b"OTriOS\0spaces-command\0v3\0";
const EVENT: &[u8] = b"OTriOS\0spaces-event\0v3\0";

fn command_shape(command: &Command) -> Result<()> {
    match command {
        Command::Watch {
            version,
            observation,
            selected,
            cursor,
        } => {
            if *version != VERSION
                || *observation == 0
                || selected.as_ref().is_some_and(|id| !valid_id(id))
                || cursor.as_ref().is_some_and(|c| !valid_id(&c.authority))
            {
                return Err(Error::Invalid);
            }
        }
        Command::Manage {
            version,
            id,
            request,
        } => {
            if *version != VERSION || *id == 0 || request.version != VERSION {
                return Err(Error::Invalid);
            }
            let valid = match &request.change {
                Change::Create { name } => !name.trim().is_empty() && name.len() <= 128,
                Change::ConfigureSpace {
                    space,
                    name,
                    layout,
                } => valid_id(space) && valid_name(name) && valid_layout(*layout),
                Change::PlaceSpace { space, before } => {
                    valid_id(space) && before.as_ref().is_none_or(|id| valid_id(id) && id != space)
                }
                Change::DeleteSpace { space, .. } | Change::RestoreSpace { space } => {
                    valid_id(space)
                }
                Change::RestoreDataset {
                    space,
                    app,
                    collection,
                }
                | Change::DeleteDataset {
                    space,
                    app,
                    collection,
                } => {
                    valid_id(space)
                        && valid_id(collection)
                        && otrios_interfaces::canvas::valid_destination(app)
                }
                Change::LabelApp { space, app, name } => {
                    valid_id(space)
                        && otrios_interfaces::canvas::valid_destination(app)
                        && valid_name(name)
                }
                Change::PlaceApp { space, app, before } => {
                    valid_id(space)
                        && otrios_interfaces::canvas::valid_destination(app)
                        && before.as_ref().is_none_or(|id| {
                            id != app && otrios_interfaces::canvas::valid_destination(id)
                        })
                }
                Change::Membership {
                    space, identity, ..
                } => valid_id(space) && valid_identity(identity),
                Change::Audience { space, .. } => valid_id(space),
                Change::Attach { space, app } | Change::Remove { space, app } => {
                    valid_id(space) && otrios_interfaces::canvas::valid_destination(app)
                }
                Change::DeleteData {
                    space,
                    app,
                    collection,
                    ..
                } => {
                    valid_id(space)
                        && valid_id(collection)
                        && otrios_interfaces::canvas::valid_destination(app)
                }
            };
            if !valid {
                return Err(Error::Invalid);
            }
        }
    }
    Ok(())
}
fn event_shape(event: &Event) -> Result<()> {
    match event {
        Event::Snapshot {
            observation,
            snapshot,
            ..
        } => {
            if *observation == 0 {
                return Err(Error::Invalid);
            }
            discovery::validate_discovery(snapshot)
        }
        Event::Result {
            version,
            id,
            created,
            ..
        } => {
            if *version != VERSION || *id == 0 || created.as_ref().is_some_and(|id| !valid_id(id)) {
                Err(Error::Invalid)
            } else {
                Ok(())
            }
        }
        Event::Unavailable { version } | Event::ResyncRequired { version } => {
            if *version == VERSION {
                Ok(())
            } else {
                Err(Error::Invalid)
            }
        }
    }
}
fn encode<T: serde::Serialize>(prefix: &[u8], value: &T, limit: usize) -> Result<Vec<u8>> {
    let payload = flexbuffers::to_vec(value).map_err(|_| Error::Invalid)?;
    if prefix.len().saturating_add(payload.len()) > limit {
        return Err(Error::Capacity);
    }
    let mut out = prefix.to_vec();
    out.extend(payload);
    Ok(out)
}
fn decode<T: serde::de::DeserializeOwned>(prefix: &[u8], bytes: &[u8], limit: usize) -> Result<T> {
    if bytes.len() > limit {
        return Err(Error::Capacity);
    }
    let payload = bytes.strip_prefix(prefix).ok_or(Error::Invalid)?;
    crate::flex_guard::validate(payload, 16, MAX_EVENT_BYTES, MAX_APPS)
        .map_err(|_| Error::Invalid)?;
    flexbuffers::from_slice(payload).map_err(|_| Error::Invalid)
}
pub fn encode_command(command: &Command) -> Result<Vec<u8>> {
    command_shape(command)?;
    encode(COMMAND, command, MAX_COMMAND_BYTES)
}
pub fn decode_command(bytes: &[u8]) -> Result<Command> {
    let command = decode(COMMAND, bytes, MAX_COMMAND_BYTES)?;
    command_shape(&command)?;
    Ok(command)
}
pub fn encode_event(event: &Event) -> Result<Vec<u8>> {
    event_shape(event)?;
    encode(EVENT, event, MAX_EVENT_BYTES)
}
pub fn decode_event(bytes: &[u8]) -> Result<Event> {
    let event = decode(EVENT, bytes, MAX_EVENT_BYTES)?;
    event_shape(&event)?;
    Ok(event)
}
pub fn management_status(error: &Error) -> ManagementStatus {
    match error {
        Error::Invalid => ManagementStatus::Invalid,
        Error::Denied => ManagementStatus::Denied,
        Error::Conflict => ManagementStatus::Conflict,
        Error::LastAdmin => ManagementStatus::LastAdmin,
        Error::Incompatible => ManagementStatus::Incompatible,
        Error::Capacity => ManagementStatus::Capacity,
        _ => ManagementStatus::Unavailable,
    }
}
