// SPDX-License-Identifier: LGPL-3.0-only
//! Target-to-parent responder delivery with bounded, snapshot-pinned pointer capture.
use super::scene::{Located, Scene};
use otrios_interfaces::scene::*;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, PartialEq)]
pub struct Visit {
    pub node: NodeId,
    pub local: [f32; 2],
    pub phase: PointerPhase,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Gesture {
    pub action: Action,
    pub local: [f32; 2],
    pub extent: [f32; 2],
    pub phase: PointerPhase,
}
#[derive(Default, Debug)]
pub struct Delivery {
    pub target: Option<NodeId>,
    pub responder: Option<NodeId>,
    pub visits: Vec<Visit>,
    pub action: Option<Action>,
    pub gesture: Option<Gesture>,
}
struct Capture {
    scene: Arc<Scene>,
    target: NodeId,
    owner: NodeId,
    responder: TouchResponder,
}
#[derive(Default)]
pub struct InputRouter {
    captures: BTreeMap<u32, Capture>,
}
fn bubble(path: &[Located<'_>], event: PointerEvent) -> (Delivery, Option<TouchResponder>) {
    let mut result = Delivery {
        target: path.last().map(|n| n.node.id),
        ..Default::default()
    };
    for entry in path.iter().rev() {
        result.visits.push(Visit {
            node: entry.node.id,
            local: [
                (event.position[0] - entry.origin[0]) / entry.scale,
                (event.position[1] - entry.origin[1]) / entry.scale,
            ],
            phase: event.phase,
        });
        if let Some(responder) = entry.node.responder
            && responder != TouchResponder::Bubble
        {
            result.responder = Some(entry.node.id);
            if let TouchResponder::Drag(action) = responder {
                result.gesture = Some(Gesture {
                    action,
                    local: result.visits.last().unwrap().local,
                    extent: [entry.node.frame.width, entry.node.frame.height],
                    phase: event.phase,
                });
            }
            return (result, Some(responder));
        }
    }
    (result, None)
}
impl InputRouter {
    pub fn active(&self) -> bool {
        !self.captures.is_empty()
    }
    pub fn cancel_all(&mut self) {
        self.captures.clear();
    }
    pub fn dispatch(&mut self, scene: Arc<Scene>, mut event: PointerEvent) -> Delivery {
        if event.position.iter().any(|v| !v.is_finite()) {
            event.phase = PointerPhase::Cancel;
            event.position = [0., 0.];
        }
        if event.phase == PointerPhase::Down {
            self.captures.remove(&event.id);
            if self.captures.len() >= 16 {
                return Delivery::default();
            }
            let (result, responder) = bubble(&scene.hit_path(event.position), event);
            if let (Some(target), Some(owner), Some(responder)) =
                (result.target, result.responder, responder)
            {
                self.captures.insert(
                    event.id,
                    Capture {
                        scene,
                        target,
                        owner,
                        responder,
                    },
                );
            }
            return result;
        }
        let Some(capture) = self.captures.remove(&event.id) else {
            return Delivery::default();
        };
        // A removed/rebound responder cannot receive an action from an old gesture.
        let valid = scene
            .path(capture.owner)
            .and_then(|p| p.last().and_then(|v| v.node.responder))
            == Some(capture.responder);
        if !valid {
            event.phase = PointerPhase::Cancel;
        }
        let path = capture.scene.path(capture.target).unwrap_or_default();
        let (mut result, _) = bubble(&path, event);
        if event.phase == PointerPhase::Up
            && capture
                .scene
                .hit_path(event.position)
                .iter()
                .any(|n| n.node.id == capture.owner)
            && let TouchResponder::Activate(action) = capture.responder
        {
            result.action = Some(action);
        }
        if event.phase == PointerPhase::Move {
            self.captures.insert(event.id, capture);
        }
        result
    }
    /// Keyboard/assistive activation follows the same responder ancestry, without a pointer.
    pub fn activate(scene: &Scene, id: NodeId) -> Delivery {
        let path = scene.path(id).unwrap_or_default();
        let (mut result, responder) = bubble(
            &path,
            PointerEvent {
                id: 0,
                phase: PointerPhase::Up,
                position: [0., 0.],
            },
        );
        if let Some(TouchResponder::Activate(action) | TouchResponder::Drag(action)) = responder {
            result.action = Some(action);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::components::*;
    fn scene() -> Arc<Scene> {
        let mut outer = panel(
            10,
            Rect::new(100., 200., 300., 240.),
            CARD,
            vec![canvas(
                20,
                Rect::new(20., 30., 100., 100.),
                vec![node(
                    21,
                    Rect::new(10., 10., 80., 80.),
                    Primitive::Circle { color: WHITE },
                )],
            )],
        );
        outer.responder = Some(TouchResponder::Activate(Action::AppEvent(1)));
        outer.children[1].responder = Some(TouchResponder::Bubble);
        Arc::new(Scene::new(canvas(0, Rect::new(0., 0., 1280., 800.), vec![outer])).unwrap())
    }
    fn event(id: u32, phase: PointerPhase, position: [f32; 2]) -> PointerEvent {
        PointerEvent {
            id,
            phase,
            position,
        }
    }
    #[test]
    fn dragging_bubbles_captures_outside_and_cancels_if_the_owner_changes() {
        let mut root = scene().root().clone();
        root.children[0].responder = Some(TouchResponder::Drag(Action::AppEvent(2)));
        let scene = Arc::new(Scene::new(root).unwrap());
        let mut input = InputRouter::default();
        let down = input.dispatch(scene.clone(), event(3, PointerPhase::Down, [170., 280.]));
        assert_eq!(down.responder, Some(10));
        assert_eq!(down.gesture.unwrap().local, [70., 80.]);
        let moved = input.dispatch(scene.clone(), event(3, PointerPhase::Move, [900., 900.]));
        let moved = moved.gesture.unwrap();
        assert_eq!(moved.local, [800., 700.]);
        assert_eq!(moved.phase, PointerPhase::Move);
        let empty =
            Arc::new(Scene::new(canvas(0, Rect::new(0., 0., 1280., 800.), vec![])).unwrap());
        assert_eq!(
            input
                .dispatch(empty, event(3, PointerPhase::Up, [900., 900.]))
                .gesture
                .unwrap()
                .phase,
            PointerPhase::Cancel
        );
        assert!(
            input
                .dispatch(scene, event(3, PointerPhase::Up, [170., 280.]))
                .gesture
                .is_none()
        );
    }
    #[test]
    fn deepest_child_bubbles_to_parent_in_local_coordinates_and_activates_once() {
        let s = scene();
        let mut input = InputRouter::default();
        let p = [170., 280.];
        let down = input.dispatch(s.clone(), event(1, PointerPhase::Down, p));
        assert_eq!(down.target, Some(21));
        assert_eq!(down.responder, Some(10));
        assert_eq!(
            down.visits.iter().map(|v| v.node).collect::<Vec<_>>(),
            [21, 20, 10]
        );
        assert_eq!(down.visits[0].local, [40., 40.]);
        assert_eq!(down.visits[2].local, [70., 80.]);
        assert_eq!(down.action, None);
        assert_eq!(
            input
                .dispatch(s.clone(), event(1, PointerPhase::Up, p))
                .action,
            Some(Action::AppEvent(1))
        );
        assert_eq!(
            input.dispatch(s, event(1, PointerPhase::Up, p)).action,
            None
        );
    }
    #[test]
    fn captured_move_never_retargets_and_release_outside_does_not_activate() {
        let s = scene();
        let mut input = InputRouter::default();
        input.dispatch(s.clone(), event(9, PointerPhase::Down, [1000., 700.]));
        assert_eq!(
            input
                .dispatch(s.clone(), event(9, PointerPhase::Up, [170., 280.]))
                .action,
            None
        );
        input.dispatch(s.clone(), event(1, PointerPhase::Down, [170., 280.]));
        let moved = input.dispatch(s.clone(), event(1, PointerPhase::Move, [1000., 700.]));
        assert_eq!(moved.target, Some(21));
        assert_eq!(moved.responder, Some(10));
        assert_eq!(
            input
                .dispatch(s, event(1, PointerPhase::Up, [1000., 700.]))
                .action,
            None
        );
    }
    #[test]
    fn cancellation_and_removed_responder_discard_gestures() {
        let s = scene();
        let mut input = InputRouter::default();
        let p = [170., 280.];
        input.dispatch(s.clone(), event(1, PointerPhase::Down, p));
        assert_eq!(
            input
                .dispatch(s.clone(), event(1, PointerPhase::Cancel, p))
                .action,
            None
        );
        assert_eq!(
            input
                .dispatch(s.clone(), event(1, PointerPhase::Up, p))
                .action,
            None
        );
        input.dispatch(s, event(2, PointerPhase::Down, p));
        let empty =
            Arc::new(Scene::new(canvas(0, Rect::new(0., 0., 1280., 800.), vec![])).unwrap());
        let result = input.dispatch(empty, event(2, PointerPhase::Up, p));
        assert_eq!(result.action, None);
        assert!(
            result
                .visits
                .iter()
                .all(|v| v.phase == PointerPhase::Cancel)
        );
    }
    #[test]
    fn frontmost_sibling_consumes_without_delivering_to_obscured_switch() {
        let s = scene();
        let mut root = s.root().clone();
        let mut cover = background(99, 300., 240., WHITE, 0.);
        cover.frame.x = 100.;
        cover.frame.y = 200.;
        cover.responder = Some(TouchResponder::Consume);
        root.children.push(cover);
        let s = Arc::new(Scene::new(root).unwrap());
        let mut input = InputRouter::default();
        let down = input.dispatch(s.clone(), event(1, PointerPhase::Down, [170., 280.]));
        assert_eq!(down.target, Some(99));
        assert_eq!(down.visits.len(), 1);
        assert_eq!(
            input
                .dispatch(s, event(1, PointerPhase::Up, [170., 280.]))
                .action,
            None
        );
    }
    #[test]
    fn independent_pointers_capture_separately_and_keyboard_uses_parent_chain() {
        let s = scene();
        let mut input = InputRouter::default();
        for id in [1, 2] {
            input.dispatch(s.clone(), event(id, PointerPhase::Down, [170., 280.]));
        }
        for id in [2, 1] {
            assert_eq!(
                input
                    .dispatch(s.clone(), event(id, PointerPhase::Up, [170., 280.]))
                    .action,
                Some(Action::AppEvent(1))
            );
        }
        let keyboard = InputRouter::activate(&s, 21);
        assert_eq!(keyboard.responder, Some(10));
        assert_eq!(keyboard.action, Some(Action::AppEvent(1)));
    }
    #[test]
    fn immutable_scene_replacement_between_press_and_release_preserves_capture() {
        let old = scene();
        let mut next = old.root().clone();
        next.children[0].frame.x += 1.;
        let next = Arc::new(Scene::new(next).unwrap());
        let mut router = InputRouter::default();
        router.dispatch(old, event(1, PointerPhase::Down, [170., 280.]));
        let result = router.dispatch(next, event(1, PointerPhase::Up, [170., 280.]));
        assert_eq!(result.action, Some(Action::AppEvent(1)));
    }
}
