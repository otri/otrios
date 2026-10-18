// SPDX-License-Identifier: LGPL-3.0-only
//! Per-device navigation, focused input and immutable system scene composition.
use super::{
    Replica, Result,
    input::InputRouter,
    keyboard::{Keyboard, TextBuffer},
    layout::Layout,
    scene::Scene,
};
use otrios_interfaces::{ContentId, keyboard::Edit, layout::Value, scene::*};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};
const SYSTEM: u32 = 4_100_000_000;
fn assets() -> Result<&'static [Layout; 4]> {
    static ASSETS: OnceLock<Result<[Layout; 4]>> = OnceLock::new();
    ASSETS
        .get_or_init(|| {
            Ok([
                Layout::from_bytes(include_bytes!("shell-assets/home.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/navigation.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/editor.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/keyboard.flex"))?,
            ])
        })
        .as_ref()
        .map_err(Clone::clone)
}
fn remap(mut node: SceneNode, offset: u32) -> Result<SceneNode> {
    node.id = node
        .id
        .checked_add(offset)
        .ok_or("System scene ID overflow")?;
    node.children = node
        .children
        .into_iter()
        .map(|n| remap(n, offset))
        .collect::<Result<_>>()?;
    Ok(node)
}
pub struct Shell {
    screen: Screen,
    keyboard: Keyboard,
    fields: [TextBuffer; 2],
    focused: Option<u8>,
    input: InputRouter,
    revision: u64,
    cached: Option<(ContentId, u64, Arc<Scene>)>,
}
impl Default for Shell {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
            keyboard: Keyboard::default(),
            fields: Default::default(),
            focused: None,
            input: InputRouter::default(),
            revision: 0,
            cached: None,
        }
    }
}
impl Shell {
    pub fn screen(&self) -> Screen {
        self.screen
    }
    pub fn screen_name(&self) -> &'static str {
        match self.screen {
            Screen::Home => "home",
            Screen::Monitoring => "monitoring",
            Screen::Controls => "controls",
            Screen::Keyboard => "keyboard",
        }
    }
    pub fn keyboard_visible(&self) -> bool {
        self.screen == Screen::Keyboard && self.keyboard.visible
    }
    pub fn show_keyboard(&mut self) {
        if self.screen == Screen::Keyboard {
            self.focus(self.focused.unwrap_or(0));
        }
    }
    pub fn editing(&self) -> bool {
        self.screen == Screen::Keyboard && self.focused.is_some()
    }
    pub fn open_name(&mut self, name: &str) -> bool {
        self.open(match name {
            "monitoring" => Screen::Monitoring,
            "controls" => Screen::Controls,
            "keyboard" => Screen::Keyboard,
            _ => Screen::Home,
        })
    }
    pub fn open(&mut self, screen: Screen) -> bool {
        if self.screen == screen {
            return false;
        }
        self.cancel();
        self.keyboard.hide();
        self.focused = None;
        self.screen = screen;
        self.changed();
        true
    }
    pub fn back(&mut self) -> bool {
        if self.keyboard_visible() {
            self.keyboard.hide();
            self.input.cancel_all();
            self.changed();
            true
        } else {
            self.open(Screen::Home)
        }
    }
    pub fn cancel(&mut self) {
        self.input.cancel_all();
        self.keyboard.cancel();
        self.changed();
    }
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.cached = None;
    }
    fn focus(&mut self, field: u8) {
        if field < 2 {
            self.input.cancel_all();
            self.keyboard.focus();
            self.focused = Some(field);
            self.changed();
        }
    }
    fn edit(&mut self, edit: &Edit) -> bool {
        if !self.editing() {
            return false;
        }
        match edit {
            Edit::Tab { backward: _ } => {
                self.focus((self.focused.unwrap() + 1) % 2);
                true
            }
            Edit::Enter => {
                self.keyboard.hide();
                self.changed();
                true
            }
            _ => {
                let changed = self.fields[self.focused.unwrap() as usize].edit(edit);
                if changed {
                    self.changed();
                }
                changed
            }
        }
    }
    pub fn hardware_edit(&mut self, edit: &Edit) -> bool {
        self.edit(edit)
    }
    pub fn tick(&mut self, now: u64) -> bool {
        if let Some(edit) = self.keyboard.tick(now) {
            self.edit(&edit)
        } else {
            false
        }
    }
    fn apply(&mut self, replica: &mut Replica, action: Option<Action>) -> Result<bool> {
        Ok(match action {
            Some(Action::Open(s)) => self.open(s),
            Some(Action::Home) => self.open(Screen::Home),
            Some(Action::Back) => self.back(),
            Some(Action::FocusField(id)) if self.screen == Screen::Keyboard => {
                self.focus(id);
                true
            }
            Some(Action::KeyboardKey(id)) if self.keyboard_visible() => {
                let edit = self.keyboard.press(id);
                if let Some(e) = edit {
                    self.edit(&e);
                }
                self.changed();
                true
            }
            Some(Action::ToggleFocus) if self.screen == Screen::Controls => {
                replica.apply_action(action)?
            }
            _ => false,
        })
    }
    pub fn activate(&mut self, replica: &mut Replica, id: u32) -> Result<bool> {
        let scene = self.scene(replica)?;
        self.apply(replica, InputRouter::activate(&scene, id).action)
    }
    pub fn pointer(
        &mut self,
        replica: &mut Replica,
        event: PointerEvent,
        now: u64,
    ) -> Result<bool> {
        if event.position.iter().any(|p| !p.is_finite()) {
            self.cancel();
            return Ok(true);
        }
        let scene = self.scene(replica)?;
        let key = scene.hit_path(event.position).iter().rev().find_map(|n| {
            if let Some(TouchResponder::Activate(Action::KeyboardKey(k))) = n.node.responder {
                Some(k)
            } else {
                None
            }
        });
        let owned = self.keyboard.touching(event.id);
        let active = |k: &Keyboard| {
            (
                k.shift,
                k.page,
                k.visible,
                (0..super::keyboard::KEY_COUNT).find(|key| k.pressed(*key)),
            )
        };
        let before = active(&self.keyboard);
        let edit = match event.phase {
            PointerPhase::Down => key.and_then(|k| self.keyboard.down(event.id, k, now)),
            PointerPhase::Move => {
                self.keyboard.motion(event.id, key);
                None
            }
            PointerPhase::Up => {
                if owned {
                    self.keyboard.motion(event.id, key);
                }
                self.keyboard.up(event.id)
            }
            PointerPhase::Cancel => {
                if owned {
                    self.keyboard.cancel();
                }
                None
            }
        };
        let mut changed = if let Some(edit) = edit {
            self.edit(&edit)
        } else {
            false
        };
        if before != active(&self.keyboard) {
            self.changed();
            changed = true;
        }
        let delivery = self.input.dispatch(scene, event);
        // Keyboard contacts use the same hit/responder ancestry but own slide/repeat semantics.
        if !matches!(delivery.action, Some(Action::KeyboardKey(_))) {
            changed |= self.apply(replica, delivery.action)?;
        }
        Ok(changed)
    }
    pub fn scene(&mut self, replica: &Replica) -> Result<Arc<Scene>> {
        let id = replica.snapshot().id();
        if let Some((old, revision, scene)) = &self.cached
            && *old == id
            && *revision == self.revision
        {
            return Ok(scene.clone());
        }
        let mut local = BTreeMap::new();
        let title = match self.screen {
            Screen::Home => "HOME",
            Screen::Monitoring => "MONITORING",
            Screen::Controls => "LIVE CONTROLS",
            Screen::Keyboard => "INPUT LAB",
        };
        local.insert(0, Value::Text(title.into()));
        local.insert(
            1,
            Value::Text(
                if self.keyboard_visible() {
                    "HIDE KEYS"
                } else {
                    "< BACK"
                }
                .into(),
            ),
        );
        for i in 0..2 {
            let value = self.fields[i].display(self.focused == Some(i as u8));
            local.insert(
                100 + i as u16,
                Value::Text(if value.is_empty() {
                    format!("Field {} / tap to type", i + 1)
                } else {
                    value
                }),
            );
            local.insert(
                110 + i as u16,
                Value::Color(if self.focused == Some(i as u8) {
                    [45, 68, 80, 255]
                } else {
                    [27, 44, 61, 255]
                }),
            );
        }
        local.insert(
            102,
            Value::Text(format!(
                "Shift: {:?} / literal input / text stays on this device",
                self.keyboard.shift
            )),
        );
        for key in 0..super::keyboard::KEY_COUNT {
            let pressed = self.keyboard.pressed(key);
            local.insert(1000 + key, Value::Text(self.keyboard.label(key)));
            local.insert(
                4000 + key,
                Value::Number(self.keyboard.label(key).chars().count() as f32 * 18.),
            );
            local.insert(
                2000 + key,
                Value::Color(if pressed {
                    [185, 238, 143, 255]
                } else {
                    [38, 57, 75, 255]
                }),
            );
            local.insert(
                3000 + key,
                Value::Color(if pressed {
                    [16, 28, 41, 255]
                } else {
                    [239, 244, 236, 255]
                }),
            );
        }
        let assets = assets()?;
        let view = replica.snapshot().view();
        let page = match self.screen {
            Screen::Home => remap(
                assets[0].resolve_local(view, &local)?.root().clone(),
                SYSTEM + 1_000_000,
            )?,
            Screen::Keyboard => remap(
                assets[2].resolve_local(view, &local)?.root().clone(),
                SYSTEM + 2_000_000,
            )?,
            Screen::Monitoring | Screen::Controls => {
                let content = if self.screen == Screen::Monitoring {
                    if replica.presentation().is_some() {
                        replica.snapshot().scene().as_ref().clone()
                    } else {
                        use super::components::*;
                        Scene::new(canvas(
                            0,
                            Rect::new(0., 0., 1280., 800.),
                            vec![text(
                                1,
                                Rect::new(48., 100., 1184., 32.),
                                "Waiting for the monitoring stream",
                                3,
                                WHITE,
                            )],
                        ))?
                    }
                } else {
                    replica.base_layout()?.resolve(view)?
                };
                let mut node = super::components::node(
                    SYSTEM + 4_000_000,
                    Rect::new(64., 80., 1152., 720.),
                    Primitive::Viewport { scale: 0.9 },
                );
                node.children.push(content.root().clone());
                node
            }
        };
        let navigation = remap(
            assets[1].resolve_local(view, &local)?.root().clone(),
            SYSTEM,
        )?;
        let mut children = vec![page, navigation];
        if self.keyboard_visible() {
            children.push(remap(
                assets[3].resolve_local(view, &local)?.root().clone(),
                SYSTEM + 3_000_000,
            )?);
        }
        let scene = Arc::new(Scene::new(super::components::canvas(
            SYSTEM + 5_000_000,
            Rect::new(0., 0., 1280., 800.),
            children,
        ))?);
        self.cached = Some((id, self.revision, scene.clone()));
        Ok(scene)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tap(s: &mut Shell, r: &mut Replica, p: [f32; 2]) {
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            s.pointer(
                r,
                PointerEvent {
                    id: 1,
                    phase,
                    position: p,
                },
                0,
            )
            .unwrap();
        }
    }
    #[test]
    fn viewport_paint_and_responder_coordinates_use_the_same_scale() {
        use super::super::components::*;
        let mut button = background(3, 100., 80., WHITE, 0.);
        button.frame.x = 20.;
        button.frame.y = 30.;
        button.responder = Some(TouchResponder::Activate(Action::ToggleFocus));
        let mut viewport = node(
            1,
            Rect::new(100., 200., 300., 200.),
            Primitive::Viewport { scale: 0.5 },
        );
        viewport.children = vec![canvas(2, Rect::new(0., 0., 600., 400.), vec![button])];
        let scene = Arc::new(
            Scene::new(canvas(0, Rect::new(0., 0., 1280., 800.), vec![viewport])).unwrap(),
        );
        let mut router = InputRouter::default();
        let result = router.dispatch(
            scene.clone(),
            PointerEvent {
                id: 1,
                phase: PointerPhase::Down,
                position: [120., 225.],
            },
        );
        assert_eq!(result.responder, Some(3));
        assert_eq!(result.visits[0].local, [20., 20.]);
        assert_eq!(
            router
                .dispatch(
                    scene.clone(),
                    PointerEvent {
                        id: 1,
                        phase: PointerPhase::Up,
                        position: [120., 225.]
                    }
                )
                .action,
            Some(Action::ToggleFocus)
        );
        let mesh = super::super::render_scene(&scene);
        assert!(!mesh.vertices.is_empty());
        for vertex in mesh.vertices.chunks_exact(6) {
            assert!((110. ..=160.).contains(&vertex[0]));
            assert!((215. ..=255.).contains(&vertex[1]));
        }
        assert!(scene.hit_path([410., 225.]).is_empty());
    }
    #[test]
    fn local_launcher_and_back_do_not_change_or_follow_shared_state() {
        let mut r = Replica::new().unwrap();
        let before = r.save();
        let mut s = Shell::default();
        tap(&mut s, &mut r, [180., 450.]);
        assert_eq!(s.screen(), Screen::Monitoring);
        assert!(s.back());
        assert_eq!(s.screen(), Screen::Home);
        assert_eq!(r.save(), before);
        s.open(Screen::Controls);
        let scene = s.scene(&r).unwrap();
        let c = scene
            .controls()
            .into_iter()
            .find(|c| {
                matches!(
                    c.node.responder,
                    Some(TouchResponder::Activate(Action::ToggleFocus))
                )
            })
            .unwrap();
        let p = [c.origin[0] + 100. * c.scale, c.origin[1] + 100. * c.scale];
        tap(&mut s, &mut r, p);
        assert_ne!(r.save(), before);
        assert_eq!(Shell::default().screen(), Screen::Home);
    }
    #[test]
    fn keyboard_fields_back_and_remote_refresh_preserve_focus_boundaries() {
        let mut r = Replica::new().unwrap();
        let mut s = Shell::default();
        s.open(Screen::Keyboard);
        tap(&mut s, &mut r, [140., 280.]);
        assert!(s.keyboard_visible());
        tap(&mut s, &mut r, [50., 520.]);
        assert_eq!(s.fields[0].text(), "q");
        s.keyboard.down(5, 10, 0);
        tap(&mut s, &mut r, [140., 380.]);
        assert_eq!(s.keyboard.tick(1000), None);
        assert_eq!(s.keyboard.up(5), None);
        assert_eq!(s.fields[1].text(), "");
        s.hardware_edit(&Edit::Commit("second".into()));
        assert!(s.back());
        assert!(!s.keyboard_visible());
        assert_eq!(s.screen(), Screen::Keyboard);
        assert!(s.back());
        assert_eq!(s.screen(), Screen::Home);
        s.open(Screen::Monitoring);
        let before = s.scene(&r).unwrap();
        r.set_enabled(false).unwrap();
        let after = s.scene(&r).unwrap();
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(s.screen(), Screen::Monitoring);
    }
    #[test]
    fn all_embedded_binary_screens_resolve_and_fit_scene_limits() {
        let r = Replica::new().unwrap();
        let mut s = Shell::default();
        for screen in [
            Screen::Home,
            Screen::Monitoring,
            Screen::Controls,
            Screen::Keyboard,
        ] {
            s.open(screen);
            s.scene(&r).unwrap();
        }
        s.focus(0);
        s.scene(&r).unwrap();
    }
}
