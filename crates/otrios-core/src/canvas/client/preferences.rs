// SPDX-License-Identifier: LGPL-3.0-only
//! Authenticated self-profile settings. Cosmetic developer controls grant no access.
use super::*;
use otrios_interfaces::spaces::{Change, UserPreferences};
impl Client {
    pub fn user_preferences(&self) -> UserPreferences {
        self.spaces
            .snapshot()
            .filter(|s| s.identity.is_some())
            .and_then(|s| s.profile.as_ref())
            .map_or_else(Default::default, |p| p.preferences)
    }
    pub(super) fn change_preferences(
        &mut self,
        action: super::super::host_ui::HostAction,
    ) -> Result<bool> {
        use super::super::host_ui::HostAction;
        let snapshot = self.spaces.snapshot().ok_or("Profile unavailable")?;
        if snapshot.identity.is_none() || self.spaces.pending() {
            return Ok(false);
        }
        let revision = snapshot.profile.as_ref().map_or(0, |p| p.revision);
        let mut preferences = self.user_preferences();
        match action {
            HostAction::DeveloperMode => preferences.developer_mode = !preferences.developer_mode,
            HostAction::HitZones if preferences.developer_mode => {
                preferences.hit_zones_on_touch = !preferences.hit_zones_on_touch
            }
            HostAction::Theme(theme) if preferences.developer_mode => {
                preferences.appearance = theme
            }
            _ => return Ok(false),
        }
        self.spaces
            .request(Change::Preferences {
                preferences,
                expected_revision: revision,
            })
            .map_err(err)?;
        self.build_host(true)?;
        Ok(true)
    }
    pub fn hit_zones_on_touch(&self) -> bool {
        let preferences = self.user_preferences();
        preferences.developer_mode && preferences.hit_zones_on_touch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn developer_controls_are_self_profile_only_and_hidden_until_enabled() {
        use crate::canvas::host_ui::{self, HostAction, Model, Pane};
        use otrios_interfaces::spaces::*;
        for developer_mode in [false, true] {
            let snapshot = Discovery {
                version: VERSION,
                cursor: crate::spaces::discovery_cursor().unwrap(),
                catalog: vec![],
                generation: 1,
                identity: Some("viewer".into()),
                spaces: vec![],
                selected: None,
                profile: Some(UserProfile {
                    name: "Viewer".into(),
                    revision: 1,
                    preferences: UserPreferences {
                        developer_mode,
                        ..Default::default()
                    },
                }),
            };
            let mut model = Model {
                pane: Pane::Profile,
                ..Default::default()
            };
            let rendered =
                host_ui::render(&snapshot, &mut model, "", true, true, [360., 640.], None).unwrap();
            assert!(
                rendered
                    .actions
                    .values()
                    .any(|a| matches!(a, HostAction::DeveloperMode))
            );
            assert_eq!(
                rendered
                    .actions
                    .values()
                    .any(|a| matches!(a, HostAction::HitZones)),
                developer_mode
            );
            assert_eq!(
                rendered
                    .actions
                    .values()
                    .any(|a| matches!(a, HostAction::Theme(_))),
                developer_mode
            );
        }
        assert!(!Client::default().hit_zones_on_touch());
    }
}
