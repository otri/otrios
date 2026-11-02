// SPDX-License-Identifier: LGPL-3.0-only
//! Private self-profile metadata; display names never rename authenticated actors.
use super::*;
use otrios_interfaces::spaces::UserProfile;
const MAX_BYTES: usize = 1024;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRecord {
    version: u32,
    root: String,
    actor: String,
    profile: UserProfile,
}
fn key(actor: &str) -> String {
    let mut bytes = b"OTriOS\0self-profile\0v1\0".to_vec();
    bytes.extend((actor.len() as u32).to_le_bytes());
    bytes.extend(actor.as_bytes());
    crate::canvas::hex(&crate::canvas::hash(&bytes))
}
impl Store {
    pub fn profile(&self, actor: &str) -> Result<Option<UserProfile>> {
        self.registry()?;
        if !valid_identity(actor) {
            return Err(Error::Invalid);
        }
        let root = suffix(self.path(), ".profiles");
        match fs::symlink_metadata(&root) {
            Ok(_) => directory(&root)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error(e)),
        }
        let Some(bytes) = read(
            &root.join(format!("{}.flex", key(actor))),
            MAX_BYTES + 37,
            true,
        )?
        else {
            return Ok(None);
        };
        if bytes.len() < 40
            || !bytes.starts_with(b"OTSU1")
            || crate::canvas::hash(&bytes[37..]) != bytes[5..37]
        {
            return Err(Error::Invalid);
        }
        crate::flex_guard::validate(&bytes[37..], 8, MAX_BYTES, 32).map_err(|_| Error::Invalid)?;
        let record: ProfileRecord =
            flexbuffers::from_slice(&bytes[37..]).map_err(|_| Error::Invalid)?;
        if record.version != 1
            || record.root != self.current.root
            || record.actor != actor
            || record.profile.revision == 0
            || !valid_name(&record.profile.name)
        {
            return Err(Error::Invalid);
        }
        Ok(Some(record.profile))
    }
    pub(super) fn commit_profile(&mut self, actor: &str, request: &Request) -> Result<()> {
        let Change::Profile {
            name,
            expected_revision,
        } = &request.change
        else {
            return Err(Error::Invalid);
        };
        if request.version != VERSION || !valid_identity(actor) || !valid_name(name) {
            return Err(Error::Invalid);
        }
        if request.expected_generation != self.registry()?.generation {
            return Err(Error::Conflict);
        }
        if self.profile(actor)?.map_or(0, |p| p.revision) != *expected_revision {
            return Err(Error::Conflict);
        }
        let root = suffix(self.path(), ".profiles");
        if !root.exists() {
            backups::new_directory(&root)?;
            backups::sync_directory(self.path.parent().ok_or(Error::Invalid)?)?;
        }
        directory(&root)?;
        let entries = fs::read_dir(&root).map_err(io_error)?;
        let mut count = 0;
        for (visited, entry) in entries.enumerate() {
            if visited >= 384 {
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
                .and_then(|s| s.strip_suffix(".tmp"))
            {
                if !valid_id(nonce) {
                    return Err(Error::Invalid);
                }
                fs::remove_file(entry.path()).map_err(io_error)?;
                continue;
            }
            if !name.strip_suffix(".flex").is_some_and(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            }) {
                return Err(Error::Invalid);
            }
            count += 1;
        }
        if *expected_revision == 0 && count >= 256 {
            return Err(Error::Capacity);
        }
        let record = ProfileRecord {
            version: 1,
            root: self.current.root.clone(),
            actor: actor.into(),
            profile: UserProfile {
                name: name.trim().into(),
                revision: expected_revision.checked_add(1).ok_or(Error::Capacity)?,
            },
        };
        let payload = flexbuffers::to_vec(&record).map_err(io_error)?;
        if payload.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        let mut bytes = b"OTSU1".to_vec();
        bytes.extend(crate::canvas::hash(&payload));
        bytes.extend(payload);
        if let Err(error) = write(&root.join(format!("{}.flex", key(actor))), &bytes) {
            self.failed = true;
            return Err(error);
        }
        Ok(())
    }
}
