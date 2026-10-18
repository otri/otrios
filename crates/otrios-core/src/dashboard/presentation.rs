// SPDX-License-Identifier: LGPL-3.0-only
//! Presentation owns pending intent; reconciliation and persistence may run on
//! another thread. Only acknowledged immutable replicas cross back to the view.
use super::{Replica, Result, ViewSnapshot, controls::State, layout::Layout, motion::Intent};
use otrios_interfaces::{ViewContent, scene::Action};
use std::sync::Arc;

pub trait Model {
    fn snapshot(&self) -> &ViewSnapshot;
    fn controls(&self) -> Result<State>;
    fn motion(&self) -> Result<Intent>;
    fn toggle_signal(&self) -> Result<super::signal::ToggleSignal>;
    fn presentation(&self) -> Option<&Layout>;
    fn set_control(&mut self, control: u8, value: [u16; 2]) -> Result<bool>;
    fn set_motion(&mut self, intent: Intent) -> Result<bool>;
    fn set_enabled(&mut self, enabled: bool) -> Result<bool>;
    fn enabled(&self) -> bool {
        self.snapshot()
            .view()
            .nodes
            .iter()
            .any(|n| matches!(n.content, ViewContent::Switch { enabled: true, .. }))
    }
    fn apply_action(&mut self, action: Option<Action>) -> Result<bool> {
        if action == Some(Action::ToggleFocus) {
            self.set_enabled(!self.enabled())
        } else {
            Ok(false)
        }
    }
}
impl Model for Replica {
    fn snapshot(&self) -> &ViewSnapshot {
        Replica::snapshot(self)
    }
    fn controls(&self) -> Result<State> {
        Replica::controls(self)
    }
    fn motion(&self) -> Result<Intent> {
        Replica::motion(self)
    }
    fn toggle_signal(&self) -> Result<super::signal::ToggleSignal> {
        Replica::toggle_signal(self)
    }
    fn presentation(&self) -> Option<&Layout> {
        Replica::presentation(self)
    }
    fn set_control(&mut self, c: u8, v: [u16; 2]) -> Result<bool> {
        Replica::set_control(self, c, v)
    }
    fn set_motion(&mut self, v: Intent) -> Result<bool> {
        Replica::set_motion(self, v)
    }
    fn set_enabled(&mut self, v: bool) -> Result<bool> {
        Replica::set_enabled(self, v)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    Control(u8, [u16; 2]),
    Motion(Intent),
    Enabled(bool, i64),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Command {
    pub sequence: u64,
    pub edit: Edit,
}
impl Command {
    pub fn apply(&self, replica: &mut Replica) -> Result<bool> {
        match self.edit {
            Edit::Control(c, v) => replica.set_control(c, v),
            Edit::Motion(v) => replica.set_motion(v),
            Edit::Enabled(v, at) => replica.set_enabled_at(v, at),
        }
    }
}
pub struct Presentation {
    published: Arc<Replica>,
    pending: Vec<Command>,
    issued: u64,
    acknowledged: u64,
    wall_time_seconds: i64,
}
impl Presentation {
    pub fn published(&self) -> &Arc<Replica> {
        &self.published
    }
    pub fn new(published: Arc<Replica>) -> Self {
        Self {
            published,
            pending: vec![],
            issued: 0,
            acknowledged: 0,
            wall_time_seconds: super::signal::wall_time_seconds(),
        }
    }
    pub fn set_wall_time(&mut self, seconds: i64) {
        self.wall_time_seconds = seconds.max(0);
    }
    pub fn sequence(&self) -> u64 {
        self.issued
    }
    pub fn pending(&self) -> &[Command] {
        &self.pending
    }
    pub fn acknowledge(&mut self, published: Arc<Replica>, sequence: u64) -> Result<()> {
        if sequence < self.acknowledged || sequence > self.issued {
            return Err("Invalid presentation acknowledgement".into());
        }
        self.pending.retain(|c| c.sequence > sequence);
        self.published = published;
        self.acknowledged = sequence;
        Ok(())
    }
    fn push(&mut self, edit: Edit) -> Result<bool> {
        // Absolute continuous controls may supersede older values. Discrete
        // commands retain order, so repeated toggles remain separate changes.
        if let Edit::Control(id, _) = edit {
            self.pending
                .retain(|c| !matches!(c.edit, Edit::Control(old,_) if old == id));
        }
        if self.pending.len() >= 64 {
            return Err("Presentation command queue full".into());
        }
        self.issued = self
            .issued
            .checked_add(1)
            .ok_or("Presentation sequence exhausted")?;
        self.pending.push(Command {
            sequence: self.issued,
            edit,
        });
        Ok(true)
    }
}
impl Model for Presentation {
    fn snapshot(&self) -> &ViewSnapshot {
        self.published.snapshot()
    }
    fn controls(&self) -> Result<State> {
        let mut value = self.published.controls()?;
        for c in &self.pending {
            if let Edit::Control(id, v) = c.edit {
                match id {
                    0 => value.level = v[0],
                    1 => value.dial = v[0],
                    2 => value.pad = v,
                    _ => return Err("Invalid pending control".into()),
                }
            }
        }
        Ok(value)
    }
    fn motion(&self) -> Result<Intent> {
        self.pending
            .iter()
            .rev()
            .find_map(|c| {
                if let Edit::Motion(v) = c.edit {
                    Some(Ok(v))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.published.motion())
    }
    fn enabled(&self) -> bool {
        self.pending
            .iter()
            .rev()
            .find_map(|c| {
                if let Edit::Enabled(v, _) = c.edit {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.published.enabled())
    }
    fn toggle_signal(&self) -> Result<super::signal::ToggleSignal> {
        self.published.toggle_signal()
    }
    fn presentation(&self) -> Option<&Layout> {
        self.published.presentation()
    }
    fn set_control(&mut self, c: u8, v: [u16; 2]) -> Result<bool> {
        if c > 2 || v.iter().any(|n| *n > 1000) || (c < 2 && v[1] != 0) {
            return Err("Invalid presentation control".into());
        }
        if self.controls()?.value(c)? == v {
            return Ok(false);
        }
        self.push(Edit::Control(c, v))
    }
    fn set_motion(&mut self, v: Intent) -> Result<bool> {
        v.validate()?;
        if self.motion()? == v {
            Ok(false)
        } else {
            self.push(Edit::Motion(v))
        }
    }
    fn set_enabled(&mut self, v: bool) -> Result<bool> {
        if self.enabled() == v {
            Ok(false)
        } else {
            self.push(Edit::Enabled(v, self.wall_time_seconds))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_toggle_carries_interaction_time_across_worker_delays() {
        let mut authority = Replica::new().unwrap();
        let mut view = Presentation::new(Arc::new(authority.clone()));
        view.set_wall_time(1000);
        view.set_enabled(false).unwrap();
        let command = view.pending()[0];
        view.set_wall_time(1301);
        authority.set_wall_time(1301);
        command.apply(&mut authority).unwrap();
        assert_eq!(
            authority.toggle_signal().unwrap().project(1000),
            vec![[1., 24.]]
        );
        assert!(authority.toggle_signal().unwrap().project(1301).is_empty());
    }
    #[test]
    fn pending_intent_survives_old_ack_and_repeated_toggles_keep_their_order() {
        let mut authority = Replica::new().unwrap();
        let mut view = Presentation::new(Arc::new(authority.clone()));
        let pinned = authority.snapshot().id();
        view.set_control(0, [200, 0]).unwrap();
        let old = view.pending().to_vec();
        for n in 201..1000 {
            view.set_control(0, [n, 0]).unwrap();
        }
        view.set_control(1, [800, 0]).unwrap();
        assert_eq!(view.pending().len(), 2);
        old[0].apply(&mut authority).unwrap();
        view.acknowledge(Arc::new(authority.clone()), old[0].sequence)
            .unwrap();
        assert_eq!(view.controls().unwrap().level, 999);
        assert_eq!(view.controls().unwrap().dial, 800);
        assert_ne!(authority.snapshot().id(), pinned);
        view.apply_action(Some(Action::ToggleFocus)).unwrap();
        view.apply_action(Some(Action::ToggleFocus)).unwrap();
        let commands = view.pending().to_vec();
        assert_eq!(commands.len(), 4);
        for command in &commands {
            command.apply(&mut authority).unwrap();
        }
        view.acknowledge(
            Arc::new(authority.clone()),
            commands.last().unwrap().sequence,
        )
        .unwrap();
        assert!(view.pending().is_empty());
        assert_eq!(authority.controls().unwrap().level, 999);
        assert_eq!(view.enabled(), authority.enabled());
        assert!(view.acknowledge(Arc::new(authority), 0).is_err());
    }
}
