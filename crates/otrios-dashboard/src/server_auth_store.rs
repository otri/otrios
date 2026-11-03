// SPDX-License-Identifier: Apache-2.0
//! Private host authentication state, independent of signed releases/app data.
use super::*;
use std::{collections::BTreeMap, io::Read};

#[derive(Clone)]
pub(super) struct Session {
    pub username: String,
    pub identity: String,
    pub credential: String,
}
pub(super) struct Store {
    path: PathBuf,
    _owner: fs::File,
    pub rows: BTreeMap<String, Session>,
    failed: bool,
}
fn private_file(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(std::io::Error::other(
            "Session state must be a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::other(
                "Session state must be private (mode 600)",
            ));
        }
    }
    Ok(())
}
fn create(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
impl Store {
    pub fn open(
        path: &Path,
        users: &HashMap<String, Login>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let absolute = std::path::absolute(path)?;
        let path = absolute.as_path();
        let lock = path.with_extension("sessions.lock");
        if lock == path {
            return Err("Session file conflicts with its owner lock".into());
        }
        let owner = match create(&lock) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                private_file(&lock)?;
                fs::OpenOptions::new().read(true).write(true).open(&lock)?
            }
            Err(error) => return Err(error.into()),
        };
        owner.try_lock()?;
        let mut rows = BTreeMap::new();
        let mut pruned = false;
        match fs::symlink_metadata(path) {
            Ok(_) => {
                private_file(path)?;
                let mut bytes = Vec::new();
                fs::File::open(path)?.take(262145).read_to_end(&mut bytes)?;
                if bytes.len() > 262144 {
                    return Err("Session state exceeds its byte limit".into());
                }
                let snapshot: Value = serde_json::from_slice(&bytes)?;
                let entries = snapshot["sessions"].as_object().ok_or("Missing sessions")?;
                if snapshot.as_object().is_none_or(|v| v.len() != 2)
                    || snapshot["version"].as_u64() != Some(1)
                    || entries.len() > MAX_SESSIONS
                {
                    return Err("Unsupported session state".into());
                }
                for (key, entry) in entries {
                    if entry.as_object().is_none_or(|v| v.len() != 3) {
                        return Err("Invalid session record".into());
                    }
                    let session = Session {
                        username: entry["username"]
                            .as_str()
                            .ok_or("Missing session username")?
                            .into(),
                        identity: entry["identity"]
                            .as_str()
                            .ok_or("Missing session identity")?
                            .into(),
                        credential: entry["credential"]
                            .as_str()
                            .ok_or("Missing session verifier")?
                            .into(),
                    };
                    if !valid_id(key)
                        || !valid_id(&session.credential)
                        || session.username.len() > 320
                        || !otrios_core::spaces::valid_identity(&session.identity)
                    {
                        return Err("Invalid session state".into());
                    }
                    // Explicit credential/account changes revoke old grants too.
                    if users.get(&session.username).is_some_and(|login| {
                        login.identity == session.identity
                            && digest(&login.hash) == session.credential
                    }) {
                        rows.insert(key.clone(), session);
                    } else {
                        pruned = true;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut store = Self {
            path: path.to_owned(),
            _owner: owner,
            rows,
            failed: false,
        };
        if pruned {
            store
                .commit(store.rows.clone())
                .map_err(|_| "Could not persist credential revocation")?;
        }
        Ok(store)
    }
    pub fn commit(&mut self, rows: BTreeMap<String, Session>) -> Result<(), StatusCode> {
        if self.failed {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        let save = || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let entries: BTreeMap<_, _> = rows.iter().map(|(key, session)| (key, json!({"username":session.username,"identity":session.identity,"credential":session.credential}))).collect();
            let bytes = serde_json::to_vec(&json!({"version":1,"sessions":entries}))?;
            if rows.len() > MAX_SESSIONS || bytes.len() > 262144 {
                return Err("Session state exceeds limits".into());
            }
            let mut random = [0; 16];
            getrandom::fill(&mut random)?;
            let temporary = self
                .path
                .with_extension(format!("sessions-{}.tmp", digest(&format!("{random:?}"))));
            let result = (|| -> std::io::Result<()> {
                let mut file = create(&temporary)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                fs::rename(&temporary, &self.path)?;
                fs::File::open(self.path.parent().unwrap_or(Path::new(".")))?.sync_all()
            })();
            if result.is_err() {
                let _ = fs::remove_file(temporary);
            }
            result?;
            Ok(())
        };
        if save().is_err() {
            // A failed directory sync has an uncertain durable outcome. Never
            // keep granting RAM-only access after a failed durable mutation.
            self.failed = true;
            self.rows.clear();
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        self.rows = rows;
        Ok(())
    }
}
