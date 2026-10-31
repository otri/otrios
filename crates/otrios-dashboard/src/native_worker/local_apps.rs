// SPDX-License-Identifier: Apache-2.0
//! Bounded filesystem/Unix notification adapter. Core owns namespace semantics.
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
        net::UnixDatagram,
    },
    path::{Path, PathBuf},
};

pub(super) struct Observer {
    path: PathBuf,
    socket_path: PathBuf,
    socket: UnixDatagram,
    _owner: File,
}
fn inspect(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Local apps metadata must be a private regular file".into());
    }
    Ok(metadata)
}
impl Observer {
    pub(super) fn bind(path: &Path) -> Result<Self, String> {
        let parent = path.parent().ok_or("Local apps path needs a parent")?;
        let metadata = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o022 != 0 {
            return Err("Local apps directory must not be writable by other users".into());
        }
        let lock_path = PathBuf::from(format!("{}.observer.lock", path.display()));
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&lock_path)
            .map_err(|e| e.to_string())?;
        inspect(&lock_path)?;
        owner
            .try_lock()
            .map_err(|_| "Local apps already has an observer")?;
        let socket_path = PathBuf::from(format!("{}.notify", path.display()));
        match fs::symlink_metadata(&socket_path) {
            Ok(metadata) if metadata.file_type().is_socket() => {
                fs::remove_file(&socket_path).map_err(|e| e.to_string())?
            }
            Ok(_) => return Err("Local apps notification path is not a socket".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
        let socket = UnixDatagram::bind(&socket_path).map_err(|e| e.to_string())?;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self {
            path: path.into(),
            socket_path,
            socket,
            _owner: owner,
        })
    }
    pub(super) fn capture(&self) -> Result<otrios_interfaces::device_apps::Namespace, String> {
        inspect(&self.path)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let limit = otrios_interfaces::device_apps::MAX_BYTES;
        if !metadata.is_file()
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.len() > limit as u64
        {
            return Err("Invalid local apps file".into());
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > limit {
            return Err("Local apps file exceeds size limit".into());
        }
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
    pub(super) fn changed(&self) -> Result<bool, String> {
        let mut changed = false;
        // A full queue already contains a rescan request. Capture the newest
        // atomic file after draining; the datagram never carries policy bytes.
        for _ in 0..32 {
            match self.socket.recv(&mut [0; 1]) {
                Ok(_) => changed = true,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(changed)
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "otrios-local-apps-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("apps.json")
        }
        fn command(&self, action: &str, root: Option<&str>) -> Command {
            let mut command = Command::new("python3");
            command
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../scripts/device-apps.py"
                ))
                .arg("--file")
                .arg(self.path())
                .arg(action)
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            if let Some(root) = root {
                command.arg(root);
            }
            command
        }
        fn run(&self, action: &str, root: Option<&str>) {
            let result = self.command(action, root).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn independent_writer_processes_notify_atomic_latest_snapshot_without_lost_changes() {
        let fixture = Fixture::new();
        fixture.run("init", None);
        let observer = Observer::bind(&fixture.path()).unwrap();
        assert_eq!(observer.capture().unwrap().generation, 1);
        assert!(!observer.changed().unwrap());
        assert!(
            Observer::bind(&fixture.path()).is_err(),
            "second owner must not steal notification socket"
        );
        let mut first = fixture.command("remove", Some("counter")).spawn().unwrap();
        let mut second = fixture
            .command("remove", Some("brass-relay"))
            .spawn()
            .unwrap();
        assert!(first.wait().unwrap().success());
        assert!(second.wait().unwrap().success());
        assert!(observer.changed().unwrap());
        let latest = observer.capture().unwrap();
        assert_eq!(latest.generation, 3);
        assert_eq!(latest.removed, ["brass-relay", "counter"]);
        assert!(!observer.changed().unwrap());
        drop(observer);
        // Offline mutation is durable and picked up after binding, so no wake
        // between subscription and initial capture can be lost.
        fixture.run("restore", Some("counter"));
        let observer = Observer::bind(&fixture.path()).unwrap();
        assert_eq!(observer.capture().unwrap().removed, ["brass-relay"]);
        fixture.run("restore", Some("brass-relay"));
        assert!(observer.changed().unwrap());
        assert!(observer.capture().unwrap().removed.is_empty());
    }

    #[test]
    fn adapter_rejects_nonprivate_oversized_symlink_and_non_socket_paths() {
        let fixture = Fixture::new();
        fixture.run("init", None);
        let observer = Observer::bind(&fixture.path()).unwrap();
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(observer.capture().is_err());
        assert!(
            !fixture
                .command("remove", Some("counter"))
                .status()
                .unwrap()
                .success()
        );
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(fixture.path(), vec![b' '; 8193]).unwrap();
        assert!(observer.capture().is_err());
        fs::remove_file(fixture.path()).unwrap();
        std::os::unix::fs::symlink("missing", fixture.path()).unwrap();
        assert!(observer.capture().is_err());
        assert!(!fixture.command("init", None).status().unwrap().success());
        drop(observer);
        let socket = PathBuf::from(format!("{}.notify", fixture.path().display()));
        fs::write(&socket, b"do not replace").unwrap();
        assert!(Observer::bind(&fixture.path()).is_err());
        assert_eq!(fs::read(socket).unwrap(), b"do not replace");
    }
}
