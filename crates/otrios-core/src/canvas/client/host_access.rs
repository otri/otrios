// SPDX-License-Identifier: LGPL-3.0-only
//! Local access drafts publish one bounded, generation-checked Space ACL change.
use super::*;
use crate::canvas::host_ui::{HostAction, Pane};
use otrios_interfaces::spaces::{Change, Member, MembershipEdit, Role};

impl Client {
    pub(super) fn host_access_action(&mut self, action: HostAction) -> Result<bool> {
        let current = self.host.access.as_ref().is_some_and(|draft| {
            self.spaces.snapshot().is_some_and(|snapshot| {
                snapshot.generation == draft.generation
                    && snapshot.cursor.authority == draft.cursor.authority
                    && snapshot.catalog == draft.catalog
                    && snapshot
                        .spaces
                        .iter()
                        .any(|s| s.id == draft.space && s.role == Some(Role::Admin))
            })
        });
        if !current {
            self.host.message = "The Space changed. Close and review access again.".into();
            self.build_host(true)?;
            return Ok(true);
        }
        let entry = if matches!(action, HostAction::AccessAdd) {
            self.typed_value(1)?
        } else {
            Value::Text(String::new())
        };
        let draft = self.host.access.as_mut().ok_or("Missing access draft")?;
        match action {
            HostAction::AccessAudience(audience) => {
                draft.audience = audience;
                self.host.pane = Pane::Members;
            }
            HostAction::AccessRole(identity) => {
                if let Some(member) = draft.members.iter_mut().find(|m| m.identity == identity) {
                    member.role = if member.role == Role::Admin {
                        Role::Member
                    } else {
                        Role::Admin
                    };
                }
            }
            HostAction::AccessRemove(identity) => draft.members.retain(|m| m.identity != identity),
            HostAction::AccessAdd => {
                if let Value::Text(identity) = entry {
                    let identity = identity.trim();
                    if !crate::spaces::valid_identity(identity) {
                        self.host.message = "Enter an email or username.".into();
                    } else if draft.members.len() >= 128 {
                        self.host.message = "This Space has reached its member limit.".into();
                    } else if !draft.members.iter().any(|m| m.identity == identity) {
                        draft.members.push(Member {
                            identity: identity.into(),
                            role: Role::Member,
                            joined_generation: 0,
                        });
                        self.host.field_owner = None;
                    }
                }
            }
            HostAction::AccessSave | HostAction::AccessApply => {
                if !draft.members.iter().any(|m| m.role == Role::Admin) {
                    self.host.message = "Keep at least one Admin.".into();
                } else {
                    let mut changes = vec![];
                    for member in &draft.original_members {
                        let role = draft
                            .members
                            .iter()
                            .find(|m| m.identity == member.identity)
                            .map(|m| m.role);
                        if role != Some(member.role) {
                            changes.push(MembershipEdit {
                                identity: member.identity.clone(),
                                role,
                            });
                        }
                    }
                    for member in &draft.members {
                        if !draft
                            .original_members
                            .iter()
                            .any(|m| m.identity == member.identity)
                        {
                            changes.push(MembershipEdit {
                                identity: member.identity.clone(),
                                role: Some(member.role),
                            });
                        }
                    }
                    if changes.len() > 16 {
                        self.host.message = "Save at most 16 membership changes at a time.".into();
                    } else if matches!(action, HostAction::AccessSave)
                        && draft.audience != draft.original_audience
                    {
                        self.host.pane = Pane::AudienceConfirm {
                            space: draft.space.clone(),
                            audience: draft.audience,
                        };
                        self.host.panel_scroll = 0.;
                    } else if changes.is_empty() && draft.audience == draft.original_audience {
                        self.host.pane = Pane::Home;
                        self.host.access = None;
                    } else {
                        let change = Change::Access {
                            space: draft.space.clone(),
                            audience: draft.audience,
                            changes,
                        };
                        if self.spaces.request(change).is_ok() {
                            self.host.pane = Pane::Home;
                            self.host.access = None;
                        } else {
                            self.host.message =
                                "Wait for the current change, then try again.".into();
                        }
                    }
                }
            }
            _ => return Ok(false),
        }
        self.build_host(true)?;
        Ok(true)
    }
}
