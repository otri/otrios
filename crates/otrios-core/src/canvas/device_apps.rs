// SPDX-License-Identifier: LGPL-3.0-only
//! Local installation policy; never changes authority admission or attachment.
use super::Result;
use otrios_interfaces::{device_apps::*, spaces::Discovery};

#[derive(Default)]
pub(super) struct LocalApps {
    configured: bool,
    valid: bool,
    latest: Option<Namespace>,
}
impl LocalApps {
    /// Invalid or rolled-back input closes the namespace until a valid snapshot
    /// at least as new as the last accepted one arrives. Keep that high-water mark.
    pub(super) fn update(&mut self, snapshot: Option<Namespace>) -> Result<()> {
        self.configured = true;
        self.valid = false;
        let next = snapshot.ok_or("Local apps unavailable")?;
        if next.version != 1
            || next.generation == 0
            || next.removed.len() > MAX_REMOVED
            || next
                .removed
                .iter()
                .any(|id| !otrios_interfaces::canvas::valid_destination(id) || id.contains('/'))
            || next.removed.windows(2).any(|pair| pair[0] >= pair[1])
            || self.latest.as_ref().is_some_and(|old| {
                next.generation < old.generation
                    || next.generation == old.generation && next != *old
            })
        {
            return Err("Invalid or rolled-back local apps namespace".into());
        }
        self.latest = Some(next);
        self.valid = true;
        Ok(())
    }
    pub(super) fn allows(&self, root: &str) -> bool {
        let root = root.split('/').next().unwrap_or(root);
        !self.configured
            || self.valid
                && self
                    .latest
                    .as_ref()
                    .is_some_and(|s| !s.removed.iter().any(|id| id == root))
    }
    pub(super) fn reference(&self, reference: &str) -> bool {
        reference
            .strip_prefix("app:")
            .and_then(|r| r.split_once('/'))
            .is_none_or(|(root, _)| self.allows(root))
    }
    pub(super) fn project(&self, snapshot: &Discovery) -> Discovery {
        let mut snapshot = snapshot.clone();
        for space in &mut snapshot.spaces {
            for data in &mut space.retained {
                if !self.allows(&data.app) {
                    data.availability = otrios_interfaces::spaces::AppAvailability::Unavailable;
                }
            }
            for app in &mut space.apps {
                if !self.allows(&app.app) {
                    app.availability = otrios_interfaces::spaces::AppAvailability::Unavailable;
                }
            }
        }
        if let Some(detail) = &mut snapshot.selected {
            detail.installed.retain(|app| self.allows(&app.policy.app));
            for data in &mut detail.retained {
                if !self.allows(&data.app) {
                    data.availability = otrios_interfaces::spaces::AppAvailability::Unavailable;
                }
            }
            for app in &mut detail.apps {
                if !self.allows(&app.app) {
                    app.launch = None;
                    app.availability = otrios_interfaces::spaces::AppAvailability::Unavailable;
                }
            }
        }
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespace_removes_root_and_children_and_invalid_input_closes_other_roots_too() {
        let mut apps = LocalApps::default();
        assert!(apps.reference("app:counter/ui"));
        let snapshot = Namespace {
            version: 1,
            generation: 1,
            removed: vec!["counter".into()],
        };
        apps.update(Some(snapshot.clone())).unwrap();
        assert!(!apps.reference("app:counter/ui"));
        assert!(!apps.reference("app:counter/nested/ui"));
        assert!(!apps.allows("counter/nested"));
        assert!(apps.allows("other"));
        assert!(apps.reference("host:spaces/ui"));
        for removed in [
            vec!["../counter".into()],
            vec!["other".into(), "counter".into()],
            vec!["counter".into(), "counter".into()],
            vec!["counter".into(); MAX_REMOVED + 1],
        ] {
            assert!(
                apps.update(Some(Namespace {
                    version: 1,
                    generation: 2,
                    removed
                }))
                .is_err()
            );
            assert!(!apps.allows("other"));
            assert!(apps.reference("host:spaces/ui"));
            apps.update(Some(snapshot.clone())).unwrap();
        }
    }
}
