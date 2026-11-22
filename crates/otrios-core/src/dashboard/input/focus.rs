// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded first-responder ownership, independent of a host's focus implementation.
use otrios_interfaces::scene::{NodeId, TouchResponder};

/// Reusing a node number for a different control must not inherit its focus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub id: NodeId,
    pub label: String,
    pub responder: TouchResponder,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct State {
    pub id: Option<NodeId>,
    pub visible: bool,
    pub modal: bool,
    pub revision: u64,
}
struct Scope {
    id: NodeId,
    opener: Option<Target>,
    visible: bool,
}
#[derive(Default)]
pub struct Focus {
    domain: String,
    scopes: Vec<Scope>,
    target: Option<Target>,
    state: State,
}
impl Focus {
    pub fn state(&self) -> State {
        self.state
    }
    fn publish(&mut self) {
        let id = self.target.as_ref().map(|t| t.id);
        let modal = !self.scopes.is_empty();
        if self.state.id != id || self.state.modal != modal {
            self.state.revision = self.state.revision.wrapping_add(1);
        }
        self.state.id = id;
        self.state.modal = modal;
    }
    fn visible(&mut self, visible: bool) {
        if self.state.visible != visible {
            self.state.visible = visible;
            self.state.revision = self.state.revision.wrapping_add(1);
        }
    }
    /// The caller supplies only controls eligible in the innermost modal scope.
    /// Scene admission bounds both this list and the scope chain.
    pub fn reconcile(
        &mut self,
        domain: &str,
        scopes: &[NodeId],
        eligible: &[Target],
        preferred: Option<NodeId>,
    ) {
        if self.domain != domain {
            self.domain = domain.into();
            self.scopes.clear();
            self.target = None;
            self.visible(false);
            self.state.revision = self.state.revision.wrapping_add(1);
        }
        let common = self
            .scopes
            .iter()
            .zip(scopes)
            .take_while(|(a, b)| a.id == **b)
            .count();
        while self.scopes.len() > common {
            let previous = self.scopes.pop().unwrap();
            self.target = previous.opener;
            self.visible(previous.visible);
        }
        let entering = scopes.len() > common;
        for id in scopes.iter().skip(common) {
            self.scopes.push(Scope {
                id: *id,
                opener: self.target.take(),
                visible: self.state.visible,
            });
        }
        if self
            .target
            .as_ref()
            .is_some_and(|target| !eligible.contains(target))
        {
            self.target = None;
        }
        if self.target.is_none() {
            self.target = preferred
                .and_then(|id| eligible.iter().find(|t| t.id == id))
                .cloned();
            if self.target.is_none() && (entering || !scopes.is_empty()) {
                self.target = eligible.first().cloned();
            }
        }
        self.publish();
    }
    pub fn select(&mut self, eligible: &[Target], id: Option<NodeId>, visible: bool) -> bool {
        let before = self.state;
        let target = id
            .and_then(|id| eligible.iter().find(|t| t.id == id))
            .cloned();
        if id.is_some() && target.is_none() {
            return false;
        }
        if self.target != target {
            self.state.revision = self.state.revision.wrapping_add(1);
        }
        self.target = target;
        self.visible(visible);
        self.publish();
        before != self.state
    }
    pub fn next(&mut self, eligible: &[Target], backward: bool) -> Option<NodeId> {
        if eligible.is_empty() {
            return None;
        }
        let at = self
            .target
            .as_ref()
            .and_then(|t| eligible.iter().position(|candidate| candidate == t));
        let index = match at {
            Some(at) if backward => (at + eligible.len() - 1) % eligible.len(),
            Some(at) => (at + 1) % eligible.len(),
            None if backward => eligible.len() - 1,
            None => 0,
        };
        let id = eligible[index].id;
        self.select(eligible, Some(id), true);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otrios_interfaces::scene::Action;
    fn target(id: u32) -> Target {
        Target {
            id,
            label: format!("Control {id}"),
            responder: TouchResponder::Activate(Action::AppEvent(id as u8)),
        }
    }
    #[test]
    fn modal_focus_traverses_restores_and_rejects_rebound_openers() {
        let mut focus = Focus::default();
        let root = vec![target(1), target(2)];
        focus.reconcile("app", &[], &root, None);
        assert_eq!(focus.next(&root, false), Some(1));
        focus.reconcile("app", &[10], &[target(3), target(4)], None);
        assert_eq!(focus.state().id, Some(3));
        assert_eq!(focus.next(&[target(3), target(4)], true), Some(4));
        focus.reconcile("app", &[10, 11], &[target(5)], None);
        assert_eq!(focus.state().id, Some(5));
        focus.reconcile("app", &[10], &[target(3), target(4)], None);
        assert_eq!(focus.state().id, Some(4));
        focus.reconcile("app", &[], &root, None);
        assert_eq!(focus.state().id, Some(1));
        assert!(focus.state().visible);
        focus.select(&root, Some(2), false);
        focus.reconcile("app", &[10], &[target(3)], None);
        let mut rebound = root.clone();
        rebound[1].responder = TouchResponder::Activate(Action::AppEvent(99));
        focus.reconcile("app", &[], &rebound, None);
        assert_eq!(focus.state().id, None);
        assert!(!focus.state().visible);
        focus.reconcile("other app", &[], &root, None);
        assert_eq!(focus.state().id, None);
    }
}
