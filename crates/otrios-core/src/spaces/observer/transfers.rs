// SPDX-License-Identifier: LGPL-3.0-only
//! Replacement reconnects query the fresh CID; they never replay a stage commit.
use super::*;
use otrios_interfaces::transfers::{DataReview, ReplacementOutcome};
pub(super) struct ImportWatch {
    pub actor: String,
    operation: String,
    pub due: bool,
}
pub(super) struct ReplacementWatch {
    pub actor: String,
    space: String,
    replacement: String,
    pub checking: bool,
    pub blocked: bool,
}
impl Observer {
    pub fn import_app(&mut self, review: &otrios_interfaces::transfers::AppReview) -> Result<u32> {
        let snapshot = self
            .snapshot
            .as_ref()
            .filter(|_| self.fresh && !self.unavailable)
            .ok_or(Error::Unavailable)?;
        if self.pending.is_some()
            || self.move_watch.is_some()
            || self.replacement_watch.is_some()
            || self.import_watch.is_some()
        {
            return Err(Error::Capacity);
        }
        let actor = snapshot.identity.clone().ok_or(Error::Denied)?;
        if snapshot.generation != review.expected_generation || snapshot.catalog != review.catalog {
            return Err(Error::Conflict);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let command = Command::ImportApp {
            version: VERSION,
            id,
            stage: review.stage.clone(),
        };
        encode_command(&command)?;
        self.next_id = id;
        self.pending = Some(id);
        self.queued = Some(command);
        self.outcome = None;
        self.import_watch = Some(ImportWatch {
            actor,
            operation: review.stage.clone(),
            due: false,
        });
        Ok(id)
    }
    pub(super) fn import_query(&mut self) -> Result<Option<Vec<u8>>> {
        if self.pending.is_some() || !self.fresh {
            return Ok(None);
        }
        let Some(watch) = self.import_watch.as_mut().filter(|w| w.due) else {
            return Ok(None);
        };
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let command = Command::ImportStatus {
            version: VERSION,
            id,
            operation: watch.operation.clone(),
        };
        let bytes = encode_command(&command)?;
        watch.due = false;
        self.next_id = id;
        self.pending = Some(id);
        Ok(Some(bytes))
    }
    pub(super) fn import_result(
        &mut self,
        id: u32,
        operation: &str,
        status: ManagementStatus,
        state: Option<otrios_interfaces::transfers::AppImportState>,
    ) -> bool {
        use otrios_interfaces::transfers::AppImportPhase as Phase;
        if self.pending != Some(id)
            || self
                .import_watch
                .as_ref()
                .is_none_or(|w| w.operation != operation)
        {
            return false;
        }
        self.pending = None;
        if status != ManagementStatus::Applied
            || state
                .as_ref()
                .is_none_or(|s| matches!(s.phase, Phase::Applied | Phase::Rejected))
        {
            self.import_watch = None;
        }
        self.outcome = Some(Outcome::AppImported { id, status, state });
        true
    }
    pub fn share_app(&mut self, space: &str, app: &str, digest: &str) -> Result<u32> {
        let snapshot = self
            .snapshot
            .as_ref()
            .filter(|_| self.fresh && !self.unavailable)
            .ok_or(Error::Unavailable)?;
        if self.pending.is_some()
            || self.move_watch.is_some()
            || self.replacement_watch.is_some()
            || self.import_watch.is_some()
        {
            return Err(Error::Capacity);
        }
        if snapshot.identity.is_none() {
            return Err(Error::Denied);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let command = Command::ShareApp {
            version: VERSION,
            id,
            space: space.into(),
            app: app.into(),
            digest: digest.into(),
            expected_generation: snapshot.generation,
            catalog: snapshot.catalog.clone(),
        };
        encode_command(&command)?;
        self.next_id = id;
        self.pending = Some(id);
        self.sharing = true;
        self.queued = Some(command);
        self.outcome = None;
        Ok(id)
    }
    pub fn replace_data(&mut self, review: &DataReview) -> Result<u32> {
        let snapshot = self
            .snapshot
            .as_ref()
            .filter(|_| self.fresh && !self.unavailable)
            .ok_or(Error::Unavailable)?;
        if self.pending.is_some()
            || self.move_watch.is_some()
            || self.replacement_watch.is_some()
            || self.import_watch.is_some()
        {
            return Err(Error::Capacity);
        }
        let actor = snapshot.identity.clone().ok_or(Error::Denied)?;
        if review.replacement.expected_generation != snapshot.generation {
            return Err(Error::Conflict);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let command = Command::ReplaceData {
            version: VERSION,
            id,
            stage: review.stage.clone(),
        };
        encode_command(&command)?;
        if !valid_id(&review.replacement.space) || !valid_id(&review.replacement.replacement) {
            return Err(Error::Invalid);
        }
        self.next_id = id;
        self.pending = Some(id);
        self.queued = Some(command);
        self.outcome = None;
        self.replacement_watch = Some(ReplacementWatch {
            actor,
            space: review.replacement.space.clone(),
            replacement: review.replacement.replacement.clone(),
            checking: false,
            blocked: false,
        });
        Ok(id)
    }
    pub fn cancel_data(&mut self, stage: &str) -> Result<u32> {
        self.cancel_stage(stage, false)
    }
    pub fn cancel_app(&mut self, stage: &str) -> Result<u32> {
        self.cancel_stage(stage, true)
    }
    fn cancel_stage(&mut self, stage: &str, app: bool) -> Result<u32> {
        if !self.fresh || self.unavailable {
            return Err(Error::Unavailable);
        }
        if self.pending.is_some()
            || self.move_watch.is_some()
            || self.replacement_watch.is_some()
            || self.import_watch.is_some()
        {
            return Err(Error::Capacity);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let command = if app {
            Command::CancelApp {
                version: VERSION,
                id,
                stage: stage.into(),
            }
        } else {
            Command::CancelData {
                version: VERSION,
                id,
                stage: stage.into(),
            }
        };
        encode_command(&command)?;
        self.next_id = id;
        self.pending = Some(id);
        self.queued = Some(command);
        self.outcome = None;
        Ok(id)
    }
    pub(super) fn replacement_query(&mut self) -> Result<Option<Vec<u8>>> {
        if self.pending.is_some() || !self.fresh {
            return Ok(None);
        }
        let Some(watch) = self.replacement_watch.as_mut().filter(|w| !w.blocked) else {
            return Ok(None);
        };
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let command = Command::ReplacementStatus {
            version: VERSION,
            id,
            space: watch.space.clone(),
            replacement: watch.replacement.clone(),
        };
        let bytes = encode_command(&command)?;
        self.next_id = id;
        self.pending = Some(id);
        watch.checking = true;
        Ok(Some(bytes))
    }
    pub(super) fn replacement_result(
        &mut self,
        id: u32,
        replacement: &str,
        status: ManagementStatus,
        outcome: Option<ReplacementOutcome>,
    ) -> bool {
        let Some(watch) = self.replacement_watch.as_mut() else {
            return false;
        };
        if self.pending != Some(id) || !watch.checking || watch.replacement != replacement {
            return false;
        }
        self.pending = None;
        watch.checking = false;
        if status == ManagementStatus::Unavailable {
            watch.blocked = true;
        } else {
            self.replacement_watch = None;
        }
        self.outcome = Some(Outcome::Replacement {
            id,
            status,
            outcome,
        });
        true
    }
}
