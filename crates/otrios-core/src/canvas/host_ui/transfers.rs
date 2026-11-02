// SPDX-License-Identifier: LGPL-3.0-only
//! Local transfer review state. Files and URLs cannot mint authority actions.
use super::*;
use otrios_interfaces::transfers::*;
#[derive(Clone, Default)]
pub(in crate::canvas) struct Capabilities {
    pub origin: String,
    pub picker: bool,
    pub export: bool,
    pub clipboard: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::canvas) enum Purpose {
    Share,
    Upload,
    Replace,
    Export,
}
#[derive(Clone)]
pub(in crate::canvas) enum Step {
    Input,
    Waiting { id: u32, cancelled: bool },
    Files { files: Vec<TransferFile> },
    App(AppReview),
    Data(DataReview),
    Share(AppShare),
    Publishing(AppShare),
    Shared(String),
    Exported(String),
}
#[derive(Clone)]
pub(in crate::canvas) struct Draft {
    pub actor: String,
    pub space: String,
    pub name: String,
    pub generation: u64,
    pub catalog: Vec<u8>,
    pub audience: otrios_interfaces::spaces::Audience,
    pub purpose: Purpose,
    pub target: TransferTarget,
    pub step: Step,
    pub back: Pane,
}
impl Draft {
    pub fn current(&self, snapshot: &Discovery) -> bool {
        snapshot.identity.as_deref() == Some(self.actor.as_str())
            && snapshot.generation == self.generation
            && snapshot.catalog == self.catalog
            && snapshot
                .selected
                .as_ref()
                .is_some_and(|s| s.space == self.space)
            && snapshot.spaces.iter().any(|s| {
                s.id == self.space
                    && !s.archived
                    && s.role == Some(otrios_interfaces::spaces::Role::Admin)
                    && s.audience == self.audience
            })
    }
}
