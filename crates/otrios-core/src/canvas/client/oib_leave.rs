// SPDX-License-Identifier: LGPL-3.0-only
//! A pending navigation owns no state transport; only its exact durable Apply
//! receipt can release a working copy before leaving the Designer.
use super::*;
use oib_workspace::{MODAL_ACTION_START, Modal, ModalAction};
use otrios_interfaces::oib::{authoring::ApplyResult, designer::DesignerMode};

pub(super) const LEAVE_APPLY: u32 = MODAL_ACTION_START;
pub(super) const LEAVE_DISCARD: u32 = MODAL_ACTION_START + 1;
pub(super) const LEAVE_STAY: u32 = MODAL_ACTION_START + 2;
#[derive(Clone)]
pub(super) enum Target {
    Back,
    Navigate(String),
    Spaces,
    Open { ui_ref: String, state_ref: String },
}
pub(super) struct Pending {
    target: Target,
    submitted: Option<Vec<u8>>,
    keyboard: bool,
    pointer: Option<(u32, u32)>,
    message: Option<String>,
}
impl Mount {
    pub(super) fn leave_modal(&self) -> Option<Modal> {
        let leave = self.leave.as_ref()?;
        let waiting = leave.submitted.is_some();
        let invalid = self
            .designer
            .as_ref()
            .is_some_and(|d| d.drafts.drafts().iter().any(|f| f.error.is_some()));
        let editable = self.binding.as_ref().is_none_or(|b| b.ui_editable);
        let blocked = self
            .working_copy
            .as_ref()
            .is_some_and(|w| w.error.is_some() || w.stale(&self.doc));
        Some(Modal {
            scroll: None,
            title: if waiting {"Applying changes"} else {"Leave component editor?"}.into(),
            lines: vec![leave.message.clone().unwrap_or_else(||if waiting {"Your changes will finish applying before you leave."} else if invalid {"Some values need correction. Stay to review them, or discard your local changes."} else {"Apply your changes, discard them, or stay in the editor."}.into())],
            actions: vec![
                ModalAction{id:LEAVE_APPLY,label:"Apply".into(),enabled:!waiting&&!invalid&&editable&&!blocked&&self.doc.confirmed()},
                ModalAction{id:LEAVE_DISCARD,label:"Discard".into(),enabled:!waiting},
                ModalAction{id:LEAVE_STAY,label:"Stay".into(),enabled:true},
            ],
        })
    }
    fn leave_has_work(&self) -> bool {
        self.designer.is_some()
            && (self
                .working_copy
                .as_ref()
                .is_some_and(|w| w.dirty() || w.applying() || w.error.is_some())
                || self.designer.as_ref().is_some_and(|d| {
                    d.drafts
                        .drafts()
                        .iter()
                        .any(|f| f.dirty || f.error.is_some())
                }))
    }
    fn leave_cancel_captures(&mut self) {
        if let Some(d) = &mut self.designer {
            d.cancel();
        }
        if let Some(s) = &mut self.preview_scroll {
            s.cancel();
        }
        self.input.cancel_all();
        self.captured_handlers.clear();
        self.keyboard.cancel();
        self.keyboard.hide();
        self.keyboard_pointer = None;
        self.order_drag = None;
        self.invalidate();
    }
}
impl Client {
    /// True means navigation was retained as a pending user decision.
    pub(super) fn request_leave(&mut self, target: Target) -> Result<bool> {
        if self.host_active {
            return Ok(false);
        };
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        if m.leave.is_some() {
            return Ok(true);
        };
        if !m.leave_has_work() {
            return Ok(false);
        };
        let submitted = m.working_copy.as_ref().and_then(|w| w.submitted.clone());
        m.leave = Some(Pending {
            target,
            submitted,
            keyboard: m.keyboard.visible,
            pointer: None,
            message: None,
        });
        m.leave_cancel_captures();
        Ok(true)
    }
    pub(super) fn leave_activate(&mut self, id: u32) -> Result<Option<bool>> {
        if self.active.as_ref().is_none_or(|m| m.leave.is_none()) {
            return self.history_review_activate(id);
        };
        let enabled = self
            .active
            .as_ref()
            .and_then(Mount::leave_modal)
            .is_some_and(|m| m.actions.iter().any(|a| a.id == id && a.enabled));
        if !enabled {
            return Ok(Some(false));
        };
        match id {
            LEAVE_STAY => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let pending = m.leave.take().ok_or("Missing leave decision")?;
                if pending.keyboard && m.designer.as_ref().is_some_and(|d| d.editor_focused()) {
                    m.keyboard.focus();
                }
                m.invalidate();
                Ok(Some(true))
            }
            LEAVE_DISCARD => {
                if !self.designer_discard_working_copy()? {
                    return Ok(Some(false));
                };
                let target = self.active.as_mut().unwrap().leave.take().unwrap().target;
                self.resume_leave(target)?;
                Ok(Some(true))
            }
            LEAVE_APPLY => {
                self.set_oib_designer_mode(DesignerMode::Edit)?;
                let result = self.designer_apply_working_copy();
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let pending = m.leave.as_mut().ok_or("Missing leave decision")?;
                pending.submitted = m.working_copy.as_ref().and_then(|w| w.submitted.clone());
                pending.message=match result {
                    Err(message)=>Some(message),
                    Ok(false)=>Some("Changes could not be applied. Stay to review them, or discard your local changes.".into()),
                    Ok(true)=>None,
                };
                m.leave_cancel_captures();
                Ok(Some(true))
            }
            _ => Ok(Some(false)),
        }
    }
    pub(super) fn leave_pointer(&mut self, event: PointerEvent) -> Result<Option<bool>> {
        if self.active.as_ref().is_none_or(|m| m.leave.is_none()) {
            return self.history_review_pointer(event);
        };
        let hit = self.scene()?.hit_path(event.position).iter().find_map(|n| {
            [LEAVE_APPLY, LEAVE_DISCARD, LEAVE_STAY]
                .contains(&n.node.id)
                .then_some(n.node.id)
        });
        let m = self.active.as_mut().unwrap();
        let pending = m.leave.as_mut().unwrap();
        let action = match event.phase {
            PointerPhase::Down => {
                if pending.pointer.is_none() {
                    pending.pointer = hit.map(|id| (event.id, id));
                }
                None
            }
            PointerPhase::Up
                if pending
                    .pointer
                    .is_some_and(|(pointer, _)| pointer == event.id) =>
            {
                let (_, id) = pending.pointer.take().unwrap();
                (hit == Some(id)).then_some(id)
            }
            PointerPhase::Cancel
                if pending
                    .pointer
                    .is_some_and(|(pointer, _)| pointer == event.id) =>
            {
                pending.pointer = None;
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            self.leave_activate(action)?;
        }
        Ok(Some(true))
    }
    fn resume_leave(&mut self, target: Target) -> Result<()> {
        match target {
            Target::Back => {
                self.back()?;
            }
            Target::Navigate(path) => self.navigate(&path)?,
            Target::Spaces => {
                self.show_spaces()?;
            }
            Target::Open { ui_ref, state_ref } => self.open(&ui_ref, &state_ref),
        };
        Ok(())
    }
    pub(super) fn poll_leave(&mut self) -> Result<bool> {
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        let Some(digest) = m.leave.as_ref().and_then(|l| l.submitted.clone()) else {
            return Ok(false);
        };
        if !m.doc.confirmed() {
            return Ok(false);
        };
        let Some(receipt) = m.doc.oib_apply_result()? else {
            return Ok(false);
        };
        if receipt.digest != digest {
            return Ok(false);
        };
        match receipt.result {
            ApplyResult::Accepted { .. } => {
                m.sync_working_copy()?;
                let target = m.leave.take().unwrap().target;
                self.resume_leave(target)?;
            }
            ApplyResult::Rejected { message } => {
                let leave = m.leave.as_mut().unwrap();
                leave.submitted = None;
                leave.message = Some(message.chars().take(160).collect());
                m.invalidate();
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::client::oib_designer::tests::{
        editor_address, focus_address, mounted_fixture_session,
    };
    use otrios_interfaces::keyboard::Edit;
    fn destination() -> Target {
        Target::Open {
            ui_ref: "app:after-designer/ui".into(),
            state_ref: "after-designer".into(),
        }
    }
    fn pump(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
        for _ in 0..40 {
            let mut progress = false;
            while let Some(bytes) = client.active.as_mut().unwrap().doc.generate().unwrap() {
                authority.receive(session, &bytes).unwrap();
                progress = true;
            }
            if let Some(bytes) = authority.generate(session).unwrap() {
                client
                    .receive(
                        &encode_server(&ServerMessage::Sync {
                            epoch: client.epoch(),
                            bytes,
                        })
                        .unwrap(),
                    )
                    .unwrap();
                progress = true;
            }
            if !progress {
                return;
            }
        }
        panic!("leave receipt did not settle");
    }
    fn edit_raw(client: &mut Client, text: &str) -> u32 {
        let address = editor_address(3);
        let otrios_interfaces::oib::designer::PropertyAddress::Occurrence { instance, path } =
            address.clone()
        else {
            panic!("occurrence fixture")
        };
        client
            .inspect_oib_selection(otrios_interfaces::oib::designer::Selection::Object(
                otrios_interfaces::oib::designer::ObjectAddress {
                    instance,
                    references: path.references,
                    node: path.node,
                },
            ))
            .unwrap();
        let id = focus_address(client, address);
        assert!(
            client
                .edit_text_input(
                    client.epoch(),
                    id,
                    &Edit::Replace {
                        text: text.into(),
                        cursor: text.len() as u32,
                        anchor: text.len() as u32,
                        selection_only: false
                    }
                )
                .unwrap()
        );
        id
    }
    #[test]
    fn stay_preserves_raw_input_and_modal_blocks_underlying_activation_and_text() {
        let (mut client, _authority, _session) = mounted_fixture_session(true);
        let field = edit_raw(&mut client, "Unapplied label");
        assert!(client.back().unwrap());
        assert!(client.active.as_ref().unwrap().leave.is_some());
        assert!(!client.keyboard_visible());
        assert!(client.text_inputs().unwrap().is_empty());
        assert!(
            !client
                .edit_text_input(client.epoch(), field, &Edit::Enter)
                .unwrap()
        );
        assert!(!client.hardware_edit(&Edit::Enter).unwrap());
        assert!(
            !client
                .activate(client.epoch(), oib_workspace::MODE_TEST)
                .unwrap()
        );
        assert_eq!(
            client.designer_activate(oib_workspace::MODE_TEST).unwrap(),
            Some(false)
        );
        let scene = client.scene().unwrap();
        let path = scene.path(oib_workspace::MODE_TEST).unwrap();
        let n = path.last().unwrap();
        let point = [n.origin[0] + 24., n.origin[1] + 24.];
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            assert!(
                client
                    .pointer_at(
                        client.epoch(),
                        PointerEvent {
                            id: 72,
                            phase,
                            position: point
                        },
                        1
                    )
                    .unwrap()
            );
        }
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .workspace_mode(),
            DesignerMode::Edit
        );
        assert!(
            client
                .active
                .as_mut()
                .unwrap()
                .doc
                .generate()
                .unwrap()
                .is_none()
        );
        let scene = client.scene().unwrap();
        let path = scene.path(LEAVE_STAY).unwrap();
        let n = path.last().unwrap();
        let stay = [n.origin[0] + 24., n.origin[1] + 24.];
        for (id, phase, position) in [
            (91, PointerPhase::Down, stay),
            (92, PointerPhase::Up, stay),
            (91, PointerPhase::Cancel, stay),
            (91, PointerPhase::Up, stay),
            (91, PointerPhase::Down, stay),
            (91, PointerPhase::Up, point),
        ] {
            assert!(
                client
                    .pointer_at(
                        client.epoch(),
                        PointerEvent {
                            id,
                            phase,
                            position
                        },
                        2
                    )
                    .unwrap()
            );
            assert!(client.active.as_ref().unwrap().leave.is_some());
        }
        assert!(client.activate(client.epoch(), LEAVE_STAY).unwrap());
        assert!(client.active.as_ref().unwrap().leave.is_none());
        assert!(client.keyboard_visible());
        assert!(
            client
                .text_inputs()
                .unwrap()
                .iter()
                .any(|f| f.id == field && f.focused && f.value == "Unapplied label")
        );
    }
    #[test]
    fn apply_waits_for_matching_durable_receipt_before_navigation() {
        let (mut client, mut authority, mut session) = mounted_fixture_session(true);
        client
            .active
            .as_mut()
            .unwrap()
            .stage_property(
                &editor_address(3),
                Some(Value::Text("Earlier Apply".into())),
            )
            .unwrap();
        assert!(client.designer_apply_working_copy().unwrap());
        pump(&mut client, &mut authority, &mut session);
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .doc
                .oib_apply_result()
                .unwrap()
                .is_some()
        );
        edit_raw(&mut client, "Apply then leave");
        let root = client.active.as_ref().unwrap().doc.oib_root();
        assert!(client.request_leave(destination()).unwrap());
        assert!(client.activate(client.epoch(), LEAVE_APPLY).unwrap());
        assert!(client.requested.is_none());
        assert!(client.navigation.is_none());
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .leave
                .as_ref()
                .unwrap()
                .submitted
                .is_some()
        );
        assert!(!client.poll_leave().unwrap());
        assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
        assert!(!client.activate(client.epoch(), LEAVE_DISCARD).unwrap());
        pump(&mut client, &mut authority, &mut session);
        assert!(client.active.as_ref().unwrap().leave.is_none());
        assert_eq!(
            client.navigation.as_ref(),
            Some(&("app:after-designer/ui".into(), "after-designer".into()))
        );
        assert!(
            !client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .dirty()
        );
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .doc
                .oib_property_register(&editor_address(3))
                .unwrap()
                .alternatives,
            vec![Some(Value::Text("Apply then leave".into()))]
        );
    }
    #[test]
    fn rejected_apply_stays_then_discard_uses_current_live_source() {
        let (mut client, mut authority, mut session) = mounted_fixture_session(true);
        let ui = client.active.as_ref().unwrap().ui.clone();
        let mut rival = StateAdapter::for_ui(&ui, "designer-mount", &[], [72; 16]).unwrap();
        let (mut rival_session, _) = authority.open_session(&[72; 16], 0).unwrap();
        rival_session.set_oib_editable(true);
        rival_session
            .set_selection(StateSelection::from_ui(&ui).unwrap())
            .unwrap();
        rival
            .receive(&authority.generate(&mut rival_session).unwrap().unwrap())
            .unwrap();
        while let Some(bytes) = rival.generate().unwrap() {
            authority.receive(&mut rival_session, &bytes).unwrap();
        }
        client
            .active
            .as_mut()
            .unwrap()
            .stage_property(
                &editor_address(3),
                Some(Value::Text("Private label".into())),
            )
            .unwrap();
        assert!(client.request_leave(destination()).unwrap());
        assert!(client.activate(client.epoch(), LEAVE_APPLY).unwrap());
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .leave
                .as_ref()
                .unwrap()
                .submitted
                .is_some()
        );
        assert!(
            rival
                .edit_oib_property(&editor_address(3), Some(Value::Text("Other client".into())))
                .unwrap()
        );
        for _ in 0..20 {
            let mut progress = false;
            while let Some(bytes) = rival.generate().unwrap() {
                authority.receive(&mut rival_session, &bytes).unwrap();
                progress = true;
            }
            if let Some(bytes) = authority.generate(&mut rival_session).unwrap() {
                rival.receive(&bytes).unwrap();
                progress = true;
            }
            if !progress {
                break;
            }
        }
        pump(&mut client, &mut authority, &mut session);
        assert!(client.active.as_ref().unwrap().leave.is_some());
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .leave
                .as_ref()
                .unwrap()
                .submitted
                .is_none()
        );
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .dirty()
        );
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .error
                .is_some()
        );
        assert!(client.navigation.is_none());
        assert!(!client.activate(client.epoch(), LEAVE_APPLY).unwrap());
        assert!(client.activate(client.epoch(), LEAVE_DISCARD).unwrap());
        assert_eq!(
            client.navigation.as_ref(),
            Some(&("app:after-designer/ui".into(), "after-designer".into()))
        );
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .doc
                .oib_property_register(&editor_address(3))
                .unwrap()
                .alternatives,
            vec![Some(Value::Text("Other client".into()))]
        );
        assert!(
            !client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .dirty()
        );
        assert!(
            client
                .active
                .as_mut()
                .unwrap()
                .doc
                .generate()
                .unwrap()
                .is_none()
        );
    }
}
