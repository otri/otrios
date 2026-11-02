// SPDX-License-Identifier: LGPL-3.0-only
//! Shared local reviews and bounded trusted-host effects. Transport never commits.
use super::*;
use crate::canvas::host_ui::{
    HostAction, Pane,
    transfers::{Capabilities, Draft, Purpose, Step},
};
use otrios_interfaces::{spaces::Role, transfers::*};
impl Client {
    pub fn set_transfer_host(
        &mut self,
        origin: &str,
        picker: bool,
        export: bool,
        clipboard: bool,
    ) -> Result<()> {
        let normalized = if origin.starts_with("https://") {
            crate::transfers::AppSource::parse(&format!("{origin}/apps/sample/{}", "0".repeat(64)))?
                .download_url()
                .split("/apps/")
                .next()
                .ok_or("Invalid transfer origin")?
                .to_string()
        } else if let Some(port) = origin
            .strip_prefix("http://127.0.0.1:")
            .filter(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            if port.parse::<u16>().ok().is_none_or(|p| p == 0) {
                return Err("Invalid local transfer origin".into());
            }
            origin.into()
        } else {
            return Err("Use a secure public transfer origin".into());
        };
        self.transfer_caps = Capabilities {
            origin: normalized,
            picker,
            export,
            clipboard,
        };
        if self.host_active {
            self.build_host(true)?;
        }
        Ok(())
    }
    pub fn take_transfer_task(&mut self) -> Option<TransferTask> {
        self.transfer_task.take()
    }
    pub fn transfer_actor(&self) -> Option<&str> {
        self.spaces.snapshot().and_then(|s| s.identity.as_deref())
    }
    pub fn transfer_drop_kind(&self) -> Option<&'static str> {
        let draft = self.host.transfer.as_ref()?;
        if self.host.pane != Pane::Transfer
            || !matches!(draft.step, Step::Input)
            || !self.transfer_current(draft)
        {
            return None;
        }
        match draft.purpose {
            Purpose::Upload => Some("app"),
            Purpose::Replace => Some("data"),
            _ => None,
        }
    }
    pub fn drop_transfer_file(&mut self) -> Result<bool> {
        if self.transfer_drop_kind().is_none() {
            return Ok(false);
        }
        self.host_transfer_action(HostAction::TransferPick)
    }
    pub fn drop_transfer_url(&mut self, url: &str) -> Result<bool> {
        if self.transfer_drop_kind() != Some("app") {
            return Ok(false);
        }
        match crate::transfers::AppSource::parse(url.trim()).and_then(|source| {
            self.transfer_io(TransferIo::Url {
                url: source.download_url(),
            })
        }) {
            Ok(()) => (),
            Err(error) => self.host.message = error,
        }
        self.build_host(true)?;
        Ok(true)
    }
    fn transfer_current(&self, draft: &Draft) -> bool {
        self.spaces.snapshot().is_some_and(|s| draft.current(s)) && !self.spaces.unavailable()
    }
    fn transfer_start(
        &mut self,
        purpose: Purpose,
        app: Option<&str>,
        collection: Option<&str>,
    ) -> Result<()> {
        if self
            .host
            .transfer
            .as_ref()
            .is_some_and(|d| matches!(d.step, Step::Waiting { .. } | Step::Publishing(_)))
        {
            return Err("Finish or cancel the current transfer.".into());
        }
        self.commit_host_label()?;
        if !self.confirmed() || self.spaces.pending() || self.input_busy() {
            return Err("Wait for confirmed changes and finished input.".into());
        }
        let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
        let selected = snapshot.selected.as_ref().ok_or("Select a Space")?;
        let space = snapshot
            .spaces
            .iter()
            .find(|s| s.id == selected.space && s.role == Some(Role::Admin) && !s.archived)
            .ok_or("Space Admin access is required.")?;
        let actor = snapshot.identity.clone().ok_or("Sign in first.")?;
        let target = if matches!(purpose, Purpose::Export | Purpose::Replace) {
            let app = app.ok_or("App unavailable")?;
            let cid = collection.ok_or("Dataset unavailable")?;
            let available = selected.apps.iter().any(|a| {
                a.app == app
                    && a.collection == cid
                    && a.availability == otrios_interfaces::spaces::AppAvailability::Available
            }) || selected.retained.iter().any(|d| {
                d.app == app
                    && d.collection == cid
                    && d.availability == otrios_interfaces::spaces::AppAvailability::Available
            });
            if !available {
                return Err("A compatible installed app is required for this dataset.".into());
            }
            let installed = selected
                .installed
                .iter()
                .find(|p| p.policy.app == app)
                .ok_or("Installed codec unavailable")?;
            TransferTarget::Data {
                dataset: DatasetRef {
                    space: space.id.clone(),
                    app: app.into(),
                    collection: cid.into(),
                    schema: installed.policy.schema.clone(),
                },
            }
        } else {
            TransferTarget::App {
                space: space.id.clone(),
            }
        };
        let step = if purpose == Purpose::Share {
            let code = selected
                .installed
                .iter()
                .find(|p| Some(p.policy.app.as_str()) == app)
                .ok_or("Installed code unavailable")?;
            let digest = code
                .bundle_sha256
                .clone()
                .ok_or("Only verified signed app code can be shared.")?;
            if self.transfer_caps.origin.is_empty() {
                return Err("The host's public app origin is unavailable.".into());
            }
            Step::Share(AppShare {
                app: code.policy.app.clone(),
                title: code.title.clone(),
                digest,
            })
        } else {
            Step::Input
        };
        self.host.transfer = Some(Draft {
            actor,
            space: space.id.clone(),
            name: space.name.clone(),
            generation: snapshot.generation,
            catalog: snapshot.catalog.clone(),
            audience: space.audience,
            purpose,
            target,
            step,
            back: self.host.pane.clone(),
        });
        self.host.pane = Pane::Transfer;
        self.host.field_owner = None;
        self.host.panel_scroll = 0.;
        if purpose == Purpose::Export {
            if !self.transfer_caps.export {
                return Err("Data downloads are unavailable on this host.".into());
            }
            self.transfer_io(TransferIo::Export)?;
        }
        Ok(())
    }
    fn transfer_io(&mut self, io: TransferIo) -> Result<()> {
        let draft = self.host.transfer.as_ref().ok_or("No transfer selected")?;
        if !self.transfer_current(draft) {
            return Err("The Space changed. Close and review this transfer again.".into());
        }
        if self.transfer_task.is_some() {
            return Err("Transfer host is busy.".into());
        }
        let id = self
            .next_transfer
            .checked_add(1)
            .ok_or("Transfer sequence exhausted")?;
        self.next_transfer = id;
        let target = draft.target.clone();
        self.host.transfer.as_mut().ok_or("No transfer")?.step = Step::Waiting {
            id,
            cancelled: false,
        };
        self.transfer_task = Some(TransferTask { id, target, io });
        self.host.field_owner = None;
        Ok(())
    }
    fn discard_transfer_review(&mut self, reply: &TransferReply) {
        // Disposal is never a publication or replacement. The authority also
        // expires abandoned stages if admission or the session was lost.
        match reply {
            TransferReply::App { review } => {
                let _ = self.spaces.cancel_app(&review.stage);
            }
            TransferReply::Data { review } => {
                let _ = self.spaces.cancel_data(&review.stage);
            }
            _ => (),
        }
    }
    pub fn receive_transfer(&mut self, id: u32, reply: TransferReply) -> Result<bool> {
        if self.transfer_copy == Some(id) {
            self.transfer_copy = None;
            if let TransferReply::Failed { .. } = reply {
                self.host.message =
                    "Couldn't copy the link. Select the displayed URL instead.".into();
                self.build_host(true)?;
                return Ok(true);
            }
            return Ok(false);
        }
        let Some(draft) = self.host.transfer.as_ref() else {
            return Ok(false);
        };
        let Step::Waiting {
            id: expected,
            cancelled,
        } = draft.step
        else {
            return Ok(false);
        };
        if id != expected {
            return Ok(false);
        }
        if cancelled || !self.transfer_current(draft) {
            let same_actor = self.spaces.snapshot().and_then(|s| s.identity.as_deref())
                == Some(draft.actor.as_str());
            if same_actor {
                self.discard_transfer_review(&reply);
            }
            self.host.transfer = None;
            if self.host.pane == Pane::Transfer {
                self.host.pane = Pane::Home;
            }
            return self.build_host(true).map(|_| true);
        }
        let draft = self.host.transfer.as_mut().ok_or("No transfer")?;
        draft.step = match reply {
            TransferReply::App { review }
                if draft.purpose == Purpose::Upload
                    && review.space == draft.space
                    && review.expected_generation == draft.generation
                    && review.catalog == draft.catalog
                    && review.audience == draft.audience
                    && crate::spaces::valid_id(&review.stage) =>
            {
                Step::App(review)
            }
            TransferReply::Data { review }
                if draft.purpose == Purpose::Replace
                    && matches!(&draft.target, TransferTarget::Data { dataset } if review.replacement.space == dataset.space && review.replacement.app == dataset.app && review.replacement.collection == dataset.collection && review.schema_sha256 == dataset.schema)
                    && review.replacement.expected_generation == draft.generation
                    && review.audience == draft.audience
                    && crate::spaces::valid_id(&review.stage) =>
            {
                Step::Data(review)
            }
            TransferReply::Files { files }
                if files.len() <= 64
                    && files.iter().all(|f| {
                        crate::spaces::valid_id(&f.id)
                            && !f.name.is_empty()
                            && f.name.len() <= 96
                            && !f.name.chars().any(char::is_control)
                    }) =>
            {
                Step::Files { files }
            }
            TransferReply::Exported { name }
                if draft.purpose == Purpose::Export
                    && !name.is_empty()
                    && name.len() <= 128
                    && !name.chars().any(char::is_control) =>
            {
                Step::Exported(name)
            }
            TransferReply::Cancelled => {
                let back = draft.back.clone();
                self.host.transfer = None;
                self.host.pane = back;
                self.host.field_owner = None;
                return self.build_host(false).map(|_| true);
            }
            TransferReply::Failed { message } => {
                self.host.message = message
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect();
                Step::Input
            }
            _ => {
                self.host.message =
                    "The transfer reply did not match this review. Close and try again.".into();
                Step::Input
            }
        };
        self.host.pane = Pane::Transfer;
        self.host.field_owner = None;
        self.host.panel_scroll = 0.;
        self.build_host(false)?;
        Ok(true)
    }
    pub(super) fn host_transfer_action(&mut self, action: HostAction) -> Result<bool> {
        let result = (|| -> Result<()> {
            match action {
                HostAction::TransferCopy => {
                    let Some(Draft {
                        step: Step::Shared(text),
                        target,
                        ..
                    }) = &self.host.transfer
                    else {
                        return Err("No app link to copy.".into());
                    };
                    if !self.transfer_caps.clipboard || self.transfer_task.is_some() {
                        return Err("Copy is unavailable on this host.".into());
                    }
                    self.next_transfer = self
                        .next_transfer
                        .checked_add(1)
                        .ok_or("Transfer sequence exhausted")?;
                    self.transfer_copy = Some(self.next_transfer);
                    self.transfer_task = Some(TransferTask {
                        id: self.next_transfer,
                        target: target.clone(),
                        io: TransferIo::Copy { text: text.clone() },
                    });
                }
                HostAction::TransferUpload => self.transfer_start(Purpose::Upload, None, None)?,
                HostAction::TransferShare(app) => {
                    self.transfer_start(Purpose::Share, Some(&app), None)?
                }
                HostAction::TransferExport(app, cid) => {
                    self.transfer_start(Purpose::Export, Some(&app), Some(&cid))?
                }
                HostAction::TransferReplace(app, cid) => {
                    self.transfer_start(Purpose::Replace, Some(&app), Some(&cid))?
                }
                HostAction::TransferPick => {
                    if !self.transfer_caps.picker {
                        return Err("File selection is unavailable on this host.".into());
                    }
                    self.transfer_io(TransferIo::Pick)?;
                }
                HostAction::TransferUrl => {
                    let Value::Text(url) = self.typed_value(1)? else {
                        return Err("Enter a shared app URL.".into());
                    };
                    let source = crate::transfers::AppSource::parse(url.trim())?;
                    if self
                        .host
                        .transfer
                        .as_ref()
                        .is_none_or(|d| d.purpose != Purpose::Upload)
                    {
                        return Err("URL intake is for app code only.".into());
                    }
                    self.transfer_io(TransferIo::Url {
                        url: source.download_url(),
                    })?;
                }
                HostAction::TransferFile(file) => {
                    let draft = self.host.transfer.as_ref().ok_or("No file review")?;
                    if !matches!(&draft.step, Step::Files { files,.. } if files.iter().any(|f| f.id == file))
                    {
                        return Err("File selection changed.".into());
                    }
                    self.transfer_io(TransferIo::File { file })?;
                }
                HostAction::TransferCommit => {
                    let draft = self.host.transfer.as_ref().ok_or("No transfer review")?;
                    if !self.transfer_current(draft) || !self.confirmed() || self.input_busy() {
                        return Err("Review current details after changes are confirmed.".into());
                    }
                    let step = self.host.transfer.as_ref().ok_or("No review")?.step.clone();
                    match step {
                        Step::Share(code) => {
                            let space = self
                                .host
                                .transfer
                                .as_ref()
                                .ok_or("No review")?
                                .space
                                .clone();
                            self.spaces
                                .share_app(&space, &code.app, &code.digest)
                                .map_err(err)?;
                            self.host.transfer.as_mut().ok_or("No review")?.step =
                                Step::Publishing(code);
                        }
                        Step::App(review) => {
                            self.spaces.import_app(&review).map_err(err)?;
                            self.host.transfer = None;
                            self.host.pane = Pane::Home;
                        }
                        Step::Data(review) => {
                            self.spaces.replace_data(&review).map_err(err)?;
                            self.host.transfer = None;
                            self.host.pane = Pane::Home;
                        }
                        _ => return Err("Choose and review a payload first.".into()),
                    }
                }
                HostAction::TransferCancel => {
                    if let Some(draft) = self.host.transfer.as_mut() {
                        let back = draft.back.clone();
                        match &mut draft.step {
                            Step::Waiting { cancelled, .. } => *cancelled = true,
                            Step::App(review) => {
                                let _ = self.spaces.cancel_app(&review.stage);
                                self.host.transfer = None;
                            }
                            Step::Data(review) => {
                                let _ = self.spaces.cancel_data(&review.stage);
                                self.host.transfer = None;
                            }
                            _ => self.host.transfer = None,
                        }
                        self.host.pane = back;
                        self.host.field_owner = None;
                    } else {
                        self.host.pane = Pane::Home;
                    }
                }
                _ => return Err("Not a transfer action".into()),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.host.message = error;
        }
        self.build_host(true)?;
        Ok(true)
    }
    pub(super) fn transfer_share_outcome(
        &mut self,
        status: otrios_interfaces::spaces::ManagementStatus,
        share: Option<&AppShare>,
    ) {
        let Some(draft) = self.host.transfer.as_mut() else {
            return;
        };
        let Step::Publishing(expected) = &draft.step else {
            return;
        };
        if status == otrios_interfaces::spaces::ManagementStatus::Applied
            && let Some(share) =
                share.filter(|s| s.app == expected.app && s.digest == expected.digest)
        {
            draft.step = Step::Shared(format!(
                "{}/apps/{}/{}",
                self.transfer_caps.origin, share.app, share.digest
            ));
            self.host.message.clear();
        } else {
            draft.step = Step::Share(expected.clone());
        }
    }
}
