// SPDX-License-Identifier: Apache-2.0
//! File observation only; typed validation, ownership and projection stay in core.
use super::*;
use std::path::PathBuf;
pub(super) fn local_as_of(generated: i64) -> Option<String> {
    // Format at the host boundary using Linux's configured timezone database.
    // Invoked only for a new collection/mount, not on every presentation tick.
    let output = std::process::Command::new("date")
        .args(["-d", &format!("@{generated}"), "+%b %d %H:%M %Z"])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let label = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (label.len() <= 32 && !label.is_empty() && label.bytes().all(|b| (32..127).contains(&b)))
        .then_some(label)
}
pub(super) struct Owner {
    root: PathBuf,
    next: Instant,
    stamp: Option<(std::time::SystemTime, u64, std::time::SystemTime, u64)>,
}
impl Owner {
    pub fn configured() -> Option<Self> {
        Some(Self {
            root: std::env::var_os("OTRIOS_MONITORING_DIR")?.into(),
            next: Instant::now(),
            stamp: None,
        })
    }
    pub fn poll(&mut self, client: &mut Client) -> Result<bool, String> {
        if Instant::now() < self.next {
            return Ok(false);
        }
        self.next = Instant::now() + Duration::from_millis(250);
        let path = self.root.join("data.merkle");
        let ui = self.root.join("ui.flex");
        // A freshly admitted signed UI can refresh the explicit local install.
        if let Some(bytes) = client.monitoring_ui()?
            && fs::read(&ui).ok().as_deref() != Some(&bytes)
        {
            otrios_core::canvas::monitoring::store::atomic(&ui, &bytes)?;
        }
        let metadata = fs::metadata(&path).map_err(|_| "Local Monitoring data unavailable")?;
        let ui_metadata = fs::metadata(&ui).map_err(|_| "Local Monitoring UI unavailable")?;
        if !metadata.is_file()
            || metadata.len() > otrios_interfaces::monitoring::MAX_SNAPSHOT_BYTES as u64
            || !ui_metadata.is_file()
            || ui_metadata.len() > otrios_core::canvas::MAX_BYTES as u64
        {
            return Err("Local Monitoring file bound".into());
        }
        let stamp = (
            metadata.modified().map_err(|e| e.to_string())?,
            metadata.len(),
            ui_metadata.modified().map_err(|e| e.to_string())?,
            ui_metadata.len(),
        );
        if self.stamp.as_ref() == Some(&stamp) {
            return Ok(false);
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let description = fs::read(&ui).map_err(|e| e.to_string())?;
        let changed = client.own_monitoring(&description, &bytes)?;
        self.stamp = Some(stamp);
        Ok(changed)
    }
}
