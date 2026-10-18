// SPDX-License-Identifier: LGPL-3.0-only
//! Per-device navigation, focused input and immutable system scene composition.
use super::{
    Result,
    input::InputRouter,
    keyboard::{Keyboard, TextBuffer},
    layout::Layout,
    presentation::Model,
    scene::Scene,
};
use otrios_interfaces::{ContentId, keyboard::Edit, layout::Value, scene::*};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};
const SYSTEM: u32 = 4_100_000_000;
fn assets() -> Result<&'static [Layout; 6]> {
    static ASSETS: OnceLock<Result<[Layout; 6]>> = OnceLock::new();
    ASSETS
        .get_or_init(|| {
            Ok([
                Layout::from_bytes(include_bytes!("shell-assets/home.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/navigation.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/editor.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/keyboard.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/motion.flex"))?,
                Layout::from_bytes(include_bytes!("shell-assets/playground.flex"))?,
            ])
        })
        .as_ref()
        .map_err(Clone::clone)
}
fn view_enabled(replica: &impl Model) -> bool {
    replica.enabled()
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
    ink: super::ink::Ink,
    screen: Screen,
    controls: super::controls::Interactions,
    monitoring_allowed: bool,
    controls_layout: bool,
    keyboard: Keyboard,
    fields: [TextBuffer; 2],
    focused: Option<u8>,
    input: InputRouter,
    revision: u64,
    stress_hz: u8,
    cached: Option<(ContentId, u64, Arc<Scene>)>,
}
impl Default for Shell {
    fn default() -> Self {
        Self {
            ink: Default::default(),
            screen: Screen::Home,
            controls: Default::default(),
            monitoring_allowed: true,
            controls_layout: false,
            keyboard: Keyboard::default(),
            fields: Default::default(),
            focused: None,
            input: InputRouter::default(),
            revision: 0,
            stress_hz: 0,
            cached: None,
        }
    }
}
impl Shell {
    /// Local presentation guard only. The host must authorize private data at
    /// its source; this flag is never a server capability or shared CRDT field.
    pub fn set_monitoring_access(&mut self, allowed: bool) -> bool {
        if self.monitoring_allowed == allowed {
            return false;
        }
        self.monitoring_allowed = allowed;
        self.changed();
        true
    }
    pub fn screen(&self) -> Screen {
        self.screen
    }
    pub fn screen_name(&self) -> &'static str {
        match self.screen {
            Screen::Home => "home",
            Screen::Monitoring => "monitoring",
            Screen::Controls => "controls",
            Screen::Keyboard => "keyboard",
            Screen::Animation => "animation",
            Screen::ThreeD => "3d",
        }
    }
    pub fn stress_hz(&self) -> u8 {
        if matches!(self.screen, Screen::Animation | Screen::ThreeD) {
            if self.stress_hz == 31 {
                30
            } else {
                self.stress_hz
            }
        } else {
            0
        }
    }
    pub fn stress_stall_ms(&self) -> u64 {
        if self.stress_hz == 31 && self.stress_hz() != 0 {
            250
        } else {
            0
        }
    }
    pub fn set_stress(&mut self, hz: u8) {
        self.stress_hz = hz;
        self.changed();
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
            "animation" => Screen::Animation,
            "3d" => Screen::ThreeD,
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
        self.controls_layout = false;
        self.screen = screen;
        self.changed();
        true
    }
    pub fn controls_layout(&self) -> bool {
        self.screen == Screen::Controls && self.controls_layout
    }
    pub fn show_controls_layout(&mut self, value: bool) {
        if self.controls_layout != value {
            self.cancel();
            self.controls_layout = value;
            self.changed();
        }
    }
    pub fn back(&mut self) -> bool {
        if self.controls_layout() {
            self.show_controls_layout(false);
            return true;
        }
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
        self.ink.cancel();
        self.input.cancel_all();
        self.controls.cancel();
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
        let ink = self.ink.tick(now) && self.screen == Screen::Controls && !self.controls_layout;
        if ink {
            self.changed();
        }
        let edited = if let Some(edit) = self.keyboard.tick(now) {
            self.edit(&edit)
        } else {
            false
        };
        ink | edited
    }
    pub fn ink_outgoing(&mut self, now: u64) -> Vec<u8> {
        self.ink.outgoing(now)
    }
    pub fn receive_ink(&mut self, bytes: &[u8], now: u64) -> Result<bool> {
        if bytes.is_empty()
            || bytes.len() > 1024 * otrios_interfaces::ink::SERVER_BYTES
            || !bytes
                .len()
                .is_multiple_of(otrios_interfaces::ink::SERVER_BYTES)
        {
            return Err("Invalid ink batch".into());
        }
        let mut changed = false;
        for point in bytes.chunks_exact(otrios_interfaces::ink::SERVER_BYTES) {
            changed |= self.ink.receive(point, now)?;
        }
        let changed = changed && self.screen == Screen::Controls && !self.controls_layout;
        if changed {
            self.changed();
        }
        Ok(changed)
    }
    fn apply(&mut self, replica: &mut impl Model, action: Option<Action>) -> Result<bool> {
        let changed = match action {
            Some(Action::Control(control)) if self.screen == Screen::Controls => {
                let mut value = replica.controls()?.value(control)?;
                value[0] = if value[0] >= 1000 {
                    0
                } else {
                    (value[0] + 100).min(1000)
                };
                replica.set_control(control, value)?
            }
            Some(Action::LayoutLab) if self.screen == Screen::Controls => {
                self.show_controls_layout(true);
                true
            }
            Some(Action::ResetControls) if self.screen == Screen::Controls => {
                self.cancel();
                self.controls.trail.clear();
                self.controls.feedback = "RESET / SHARED DEFAULTS".into();
                let state = super::controls::State::default();
                let mut changed = false;
                for i in 0..3 {
                    changed |= replica.set_control(i, state.value(i)?)?;
                }
                changed
            }

            Some(Action::Motion(id))
                if matches!(self.screen, Screen::Animation | Screen::ThreeD) =>
            {
                let mut intent = replica.motion()?;
                match id {
                    0 => intent.playing = !intent.playing,
                    1 => {
                        intent.speed = if intent.speed >= 4. {
                            0.25
                        } else {
                            intent.speed * 2.
                        }
                    }
                    2 => {
                        intent.count = if intent.count >= 64 {
                            12
                        } else {
                            (intent.count * 2).min(64)
                        }
                    }
                    _ => return Ok(false),
                }
                replica.set_motion(intent)?
            }
            Some(Action::Stress) if matches!(self.screen, Screen::Animation | Screen::ThreeD) => {
                self.stress_hz = match self.stress_hz {
                    0 => 10,
                    10 => 30,
                    30 => 31,
                    _ => 0,
                };
                self.changed();
                true
            }
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
        };
        if changed {
            self.changed();
        }
        Ok(changed)
    }
    pub fn activate(&mut self, replica: &mut impl Model, id: u32) -> Result<bool> {
        let scene = self.scene(replica)?;
        self.apply(replica, InputRouter::activate(&scene, id).action)
    }
    pub fn pointer(
        &mut self,
        replica: &mut impl Model,
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
        if self.screen == Screen::Controls
            && let Some(gesture) = &delivery.gesture
        {
            self.ink.gesture(event.id, gesture, now);
            let changed = self.controls.gesture(replica, event.id, gesture, now)?;
            if changed {
                self.changed();
            }
            return Ok(changed);
        }
        // Keyboard contacts use the same hit/responder ancestry but own slide/repeat semantics.
        if !matches!(delivery.action, Some(Action::KeyboardKey(_))) {
            changed |= self.apply(replica, delivery.action)?;
        }
        Ok(changed)
    }
    pub fn flush_controls(&mut self, replica: &mut impl Model, now: u64) -> Result<bool> {
        let changed = self.controls.flush(replica, now)?;
        if changed {
            self.changed();
        }
        Ok(changed)
    }
    pub fn adjust_control(
        &mut self,
        replica: &mut impl Model,
        id: u32,
        dx: i16,
        dy: i16,
    ) -> Result<bool> {
        let scene = self.scene(replica)?;
        let Some(path) = scene.path(id) else {
            return Ok(false);
        };
        let Some(TouchResponder::Drag(Action::Control(control))) =
            path.last().and_then(|n| n.node.responder)
        else {
            return Ok(false);
        };
        let mut value = replica.controls()?.value(control)?;
        let previous = value;
        value[0] =
            (i32::from(value[0]) + i32::from(dx) + if control < 2 { i32::from(dy) } else { 0 })
                .clamp(0, 1000) as u16;
        if control == 2 {
            value[1] = (i32::from(value[1]) + i32::from(dy)).clamp(0, 1000) as u16;
            self.ink.step(previous, value);
            self.changed();
        }
        replica.set_control(control, value)
    }
    pub fn control_value(&mut self, replica: &impl Model, id: u32) -> Result<Option<f64>> {
        let scene = self.scene(replica)?;
        let Some(path) = scene.path(id) else {
            return Ok(None);
        };
        Ok(match path.last().and_then(|n| n.node.responder) {
            Some(TouchResponder::Drag(Action::Control(control))) if control < 2 => {
                Some(f64::from(self.controls.preview(replica.controls()?).value(control)?[0]) / 10.)
            }
            _ => None,
        })
    }
    pub fn scene(&mut self, replica: &impl Model) -> Result<Arc<Scene>> {
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
            Screen::Controls => {
                if self.controls_layout {
                    "LAYOUT LAB"
                } else {
                    "LIVE CONTROLS"
                }
            }
            Screen::Keyboard => "INPUT LAB",
            Screen::Animation => "MOTION LAB",
            Screen::ThreeD => "3D LAB",
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
        let intent = replica.motion()?;
        local.insert(
            500,
            Value::Text(if intent.playing { "PAUSE" } else { "PLAY" }.into()),
        );
        local.insert(501, Value::Text(format!("SPEED {}x", intent.speed)));
        local.insert(502, Value::Text(format!("OBJECTS {}", intent.count)));
        local.insert(
            503,
            Value::Text(if self.stress_hz == 31 {
                "STALL 250ms".into()
            } else {
                format!("CRDT LOAD {}Hz", self.stress_hz)
            }),
        );
        local.insert(
            504,
            Value::Text(
                if self.screen == Screen::ThreeD {
                    "Perspective tetrahedra / CPU projection + GPU triangles"
                } else {
                    "Orbiting circles / local clock + smooth speed changes"
                }
                .into(),
            ),
        );
        local.insert(
            506,
            Value::Text(
                if self.monitoring_allowed {
                    "Private / available"
                } else {
                    "Private / sign in"
                }
                .into(),
            ),
        );
        let controls = self.controls.preview(replica.controls()?);
        for (id, value) in [
            (600, controls.level),
            (601, controls.dial),
            (602, controls.pad[0]),
            (603, controls.pad[1]),
        ] {
            local.insert(id, Value::Number(f32::from(value) / 1000.));
        }
        local.insert(712, Value::Boolean(controls.level > 0));
        for i in 0..3 {
            local.insert(720 + u16::from(i), Value::Boolean(self.controls.active(i)));
        }
        let angle = (f32::from(controls.dial) / 1000. - 0.5) * std::f32::consts::PI * 1.5;
        local.insert(604, Value::Number(0.5 + angle.sin() * 0.32));
        local.insert(605, Value::Number(0.5 - angle.cos() * 0.32));
        local.insert(610, Value::Text(format!("{}%", controls.level / 10)));
        local.insert(611, Value::Text(format!("{}%", controls.dial / 10)));
        local.insert(
            612,
            Value::Text(format!(
                "X {} / Y {}",
                controls.pad[0] / 10,
                controls.pad[1] / 10
            )),
        );
        local.insert(
            613,
            Value::Text(if self.controls.feedback.is_empty() {
                "DRAG A CONTROL / VALUES FOLLOW ACROSS DEVICES".into()
            } else {
                self.controls.feedback.clone()
            }),
        );
        local.insert(
            616,
            Value::Text(
                if view_enabled(replica) {
                    "OUTPUT ON"
                } else {
                    "OUTPUT OFF"
                }
                .into(),
            ),
        );
        local.insert(
            617,
            Value::Text(format!(
                "Touch pad / X {} / Y {} / arrow keys move the point",
                controls.pad[0] / 10,
                controls.pad[1] / 10
            )),
        );
        for i in 0..12 {
            local.insert(
                700 + i as u16,
                Value::Boolean(f32::from(controls.dial) / 1000. >= i as f32 / 11.),
            );
        }
        let assets = assets()?;
        let view = replica.snapshot().view();
        let mut page = match self.screen {
            Screen::Home => remap(
                assets[0].resolve_local(view, &local)?.root().clone(),
                SYSTEM + 1_000_000,
            )?,
            Screen::Keyboard => remap(
                assets[2].resolve_local(view, &local)?.root().clone(),
                SYSTEM + 2_000_000,
            )?,
            Screen::Animation | Screen::ThreeD => remap(
                assets[4].resolve_local(view, &local)?.root().clone(),
                SYSTEM + 6_000_000,
            )?,
            Screen::Controls => {
                if self.controls_layout {
                    let mut node = super::components::node(
                        SYSTEM + 4_000_000,
                        Rect::new(64., 80., 1152., 720.),
                        Primitive::Viewport { scale: 0.9 },
                    );
                    node.children
                        .push(replica.base_layout()?.resolve(view)?.root().clone());
                    node
                } else {
                    remap(
                        assets[5].resolve_local(view, &local)?.root().clone(),
                        SYSTEM + 7_000_000,
                    )?
                }
            }
            Screen::Monitoring => {
                let content = if self.screen == Screen::Monitoring {
                    if self.monitoring_allowed && replica.presentation().is_some() {
                        replica.snapshot().scene().as_ref().clone()
                    } else {
                        use super::components::*;
                        Scene::new(canvas(
                            0,
                            Rect::new(0., 0., 1280., 800.),
                            vec![text(
                                1,
                                Rect::new(48., 100., 1184., 32.),
                                if self.monitoring_allowed {
                                    "Waiting for the monitoring stream"
                                } else {
                                    "Sign in to open Monitoring"
                                },
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
        if self.screen == Screen::Controls && !self.controls_layout {
            fn ink(node: &mut SceneNode, layer: &super::ink::Ink) {
                if node.responder == Some(TouchResponder::Drag(Action::Control(2))) {
                    node.children.extend(
                        layer.nodes([node.frame.width, node.frame.height], SYSTEM + 8_000_000),
                    );
                } else {
                    for child in &mut node.children {
                        ink(child, layer);
                    }
                }
            }
            ink(&mut page, &self.ink);
        }
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
    use crate::dashboard::Replica;
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
    fn a_locked_monitoring_scene_never_renders_a_retained_private_view() {
        use otrios_interfaces::layout::{Expr, Property, Value};
        let mut blueprint = Layout::bootstrap().unwrap().blueprint().clone();
        let label = blueprint
            .root
            .children
            .iter_mut()
            .find(|i| i.id == 2)
            .unwrap();
        label.properties.retain(|p| p.key != 1);
        label.properties.push(Property {
            key: 1,
            value: Expr::Literal(Value::Text("PRIVATE VIEW CANARY".into())),
        });
        let mut replica = Replica::new().unwrap();
        replica
            .set_presentation(Some(&Layout::encode(&blueprint).unwrap()))
            .unwrap();
        let mut shell = Shell::default();
        shell.open(Screen::Monitoring);
        fn labels(node: &SceneNode) -> String {
            let mut text = match &node.primitive {
                Primitive::Text { value, .. } => value.clone(),
                _ => String::new(),
            };
            for child in &node.children {
                text.push_str(&labels(child));
            }
            text
        }
        assert!(labels(shell.scene(&replica).unwrap().root()).contains("PRIVATE VIEW CANARY"));
        shell.set_monitoring_access(false);
        let locked = labels(shell.scene(&replica).unwrap().root());
        assert!(!locked.contains("PRIVATE VIEW CANARY"));
        assert!(locked.contains("Sign in to open Monitoring"));
    }
    #[test]
    fn local_launcher_and_back_do_not_change_or_follow_shared_state() {
        let mut r = Replica::new().unwrap();
        let before = r.save();
        let mut s = Shell::default();
        tap(&mut s, &mut r, [180., 450.]);
        assert_eq!(s.screen(), Screen::Controls);
        assert!(s.back());
        // The last card's decorative lock still bubbles to its Open responder.
        tap(&mut s, &mut r, [795., 525.]);
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
        let p = [
            c.origin[0] + c.node.frame.width * 0.5 * c.scale,
            c.origin[1] + c.node.frame.height * 0.5 * c.scale,
        ];
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
    fn demo_responders_separate_shared_intent_from_local_stress_and_navigation() {
        let mut r = Replica::new().unwrap();
        let mut s = Shell::default();
        s.open(Screen::Animation);
        let before = r.save();
        for expected in [(10, 0), (30, 0), (30, 250), (0, 0)] {
            tap(&mut s, &mut r, [1080., 220.]);
            assert_eq!((s.stress_hz(), s.stress_stall_ms()), expected);
        }
        assert_eq!(r.save(), before);
        tap(&mut s, &mut r, [120., 220.]);
        assert!(!r.motion().unwrap().playing);
        let mut remote = Replica::new().unwrap();
        remote.merge(&r.save()).unwrap();
        assert_eq!(remote.motion().unwrap(), r.motion().unwrap());
        let saved = r.save();
        s.open(Screen::ThreeD);
        s.back();
        assert_eq!(r.save(), saved);
    }
    #[test]
    fn playground_drag_uses_responder_coordinates_and_layout_back_is_local() {
        let mut r = Replica::new().unwrap();
        let mut s = Shell::default();
        s.open(Screen::Controls);
        let scene = s.scene(&r).unwrap();
        let slider = scene
            .controls()
            .into_iter()
            .find(|c| c.node.responder == Some(TouchResponder::Drag(Action::Control(0))))
            .unwrap();
        let at = |x, y| {
            [
                slider.origin[0] + x * slider.scale,
                slider.origin[1] + y * slider.scale,
            ]
        };
        for (phase, position, now) in [
            (PointerPhase::Down, at(20., 40.), 0),
            (PointerPhase::Move, at(160., 40.), 10),
            (PointerPhase::Up, at(1000., 40.), 11),
        ] {
            s.pointer(
                &mut r,
                PointerEvent {
                    id: 9,
                    phase,
                    position,
                },
                now,
            )
            .unwrap();
        }
        assert_eq!(r.controls().unwrap().level, 1000);
        let before = r.save();
        s.show_controls_layout(true);
        assert!(s.controls_layout());
        s.scene(&r).unwrap();
        assert!(s.back());
        assert!(!s.controls_layout());
        assert_eq!(s.screen(), Screen::Controls);
        assert_eq!(r.save(), before);
        assert!(s.back());
        assert_eq!(s.screen(), Screen::Home);
    }
    #[test]
    fn playground_extremes_and_small_nonzero_fills_remain_valid_scenes() {
        let mut r = Replica::new().unwrap();
        let mut s = Shell::default();
        s.open(Screen::Controls);
        for value in [0, 1, 1000] {
            for control in 0..3 {
                r.set_control(
                    control,
                    if control == 2 {
                        [value, 1000 - value]
                    } else {
                        [value, 0]
                    },
                )
                .unwrap();
            }
            let scene = s.scene(&r).unwrap();
            assert!(!super::super::render_scene(&scene).vertices.is_empty());
        }
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
            Screen::Animation,
            Screen::ThreeD,
        ] {
            s.open(screen);
            s.scene(&r).unwrap();
        }
        s.focus(0);
        s.scene(&r).unwrap();
    }
}
