// SPDX-License-Identifier: Apache-2.0
//! Declarative source builder. Numeric node/action/binding IDs are compiled details.
pub use otrios_interfaces::layout::AxisResize;
use otrios_interfaces::{canvas::*, layout::*};
/// Advanced six-segment spring configuration; convenience pin methods cover common cases.
#[derive(Clone, Copy)]
pub struct Resizing {
    pub horizontal: AxisResize,
    pub vertical: AxisResize,
}
pub const BG: [u8; 4] = [16, 28, 41, 255];
pub const CARD: [u8; 4] = [27, 44, 61, 255];
pub const WHITE: [u8; 4] = [239, 244, 236, 255];
pub const MUTED: [u8; 4] = [150, 171, 185, 255];
pub const TRACK: [u8; 4] = [45, 65, 78, 255];
pub const TEAL: [u8; 4] = [84, 174, 183, 255];
pub const ACCENT: [u8; 4] = [185, 238, 143, 255];
const SCREEN: [f32; 4] = [0., 0., 1280., 800.];
const NORMALIZED_MAX: f32 = 1000.;

/// Stable schema identity, unlike the compiler's per-UI node/action indices.
#[derive(Clone, Copy)]
pub struct StateField {
    id: u16,
    name: &'static str,
    default: u16,
    kind: FieldKind,
    writable: bool,
}
#[derive(Clone, Copy)]
enum FieldKind {
    Number,
    Text,
    Series,
    TimedSeries,
}
impl StateField {
    pub const fn normalized(id: u16, name: &'static str, default: u16) -> Self {
        Self {
            id,
            name,
            default,
            kind: FieldKind::Number,
            writable: true,
        }
    }
    pub const fn text(id: u16, name: &'static str) -> Self {
        Self {
            id,
            name,
            default: 0,
            kind: FieldKind::Text,
            writable: true,
        }
    }
    pub const fn series(id: u16, name: &'static str) -> Self {
        Self {
            id,
            name,
            default: 0,
            kind: FieldKind::Series,
            writable: false,
        }
    }
    pub const fn timed_series(id: u16, name: &'static str) -> Self {
        Self {
            id,
            name,
            default: 0,
            kind: FieldKind::TimedSeries,
            writable: false,
        }
    }
    pub const fn id(self) -> u16 {
        self.id
    }
    pub fn value(self) -> ValueExpr {
        ValueExpr::State(self, Projection::Identity)
    }
    pub fn format_number(self, prefix: &str, suffix: &str, scale: f32, offset: f32) -> ValueExpr {
        self.formatted(prefix, suffix, scale, offset, 2)
    }
    pub fn format_integer(self, prefix: &str, suffix: &str, scale: f32, offset: f32) -> ValueExpr {
        self.formatted(prefix, suffix, scale, offset, 0)
    }
    fn formatted(
        self,
        prefix: &str,
        suffix: &str,
        scale: f32,
        offset: f32,
        precision: u8,
    ) -> ValueExpr {
        ValueExpr::State(
            self,
            Projection::NumberText {
                prefix: prefix.into(),
                suffix: suffix.into(),
                scale,
                offset,
                precision,
            },
        )
    }
    pub fn choose_text(self, zero: &str, nonzero: &str) -> ValueExpr {
        ValueExpr::State(
            self,
            Projection::ChooseText {
                zero: zero.into(),
                nonzero: nonzero.into(),
            },
        )
    }
    pub fn choose_color(self, zero: [u8; 4], nonzero: [u8; 4]) -> ValueExpr {
        ValueExpr::State(self, Projection::ChooseColor { zero, nonzero })
    }
    pub fn turn_x(self, center: f32, radius: f32) -> ValueExpr {
        ValueExpr::State(self, Projection::TurnX { center, radius })
    }
    pub fn turn_y(self, center: f32, radius: f32) -> ValueExpr {
        ValueExpr::State(self, Projection::TurnY { center, radius })
    }
    pub fn window_signal(self, seconds: u32) -> ValueExpr {
        ValueExpr::State(self, Projection::WindowSignal { seconds })
    }
    pub fn mapped(self, min: f32, max: f32) -> ValueExpr {
        ValueExpr::State(
            self,
            Projection::Number {
                scale: (max - min) / NORMALIZED_MAX,
                offset: min,
            },
        )
    }
    pub fn percent(self) -> ValueExpr {
        ValueExpr::State(self, Projection::Percent)
    }
    pub fn on_off(self) -> ValueExpr {
        ValueExpr::State(self, Projection::EnabledText)
    }
}
pub fn schema(id: &str, fields: &[StateField]) -> StateSchema {
    StateSchema {
        id: id.into(),
        fields: fields
            .iter()
            .map(|f| Field {
                id: f.id,
                name: f.name.into(),
                default: match f.kind {
                    FieldKind::Number => Value::Number(f32::from(f.default)),
                    FieldKind::Text => Value::Text(String::new()),
                    FieldKind::Series => Value::Series(vec![]),
                    FieldKind::TimedSeries => Value::TimedSeries(vec![]),
                },
                writable: f.writable,
            })
            .collect(),
    }
}
#[derive(Clone)]
pub enum ValueExpr {
    Literal(Expr),
    State(StateField, Projection),
}
impl From<f32> for ValueExpr {
    fn from(n: f32) -> Self {
        Self::Literal(Expr::Literal(Value::Number(n)))
    }
}
impl From<[u8; 4]> for ValueExpr {
    fn from(c: [u8; 4]) -> Self {
        Self::Literal(tint(c))
    }
}
impl From<&str> for ValueExpr {
    fn from(s: &str) -> Self {
        Self::Literal(Expr::Literal(Value::Text(s.into())))
    }
}
fn tint(c: [u8; 4]) -> Expr {
    Expr::Literal(Value::Color(c))
}
fn yes() -> Expr {
    Expr::Literal(Value::Boolean(true))
}
fn no() -> Expr {
    Expr::Literal(Value::Boolean(false))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    ui_ref: String,
    state_ref: String,
}
impl Destination {
    pub fn app(id: &str) -> Self {
        Self {
            ui_ref: format!("app:{id}/ui"),
            state_ref: format!("app:{id}/state"),
        }
    }
}
/// A versioned guest ABI event, not a node ID, action index or state field ID.
#[derive(Clone, Copy)]
pub struct BehaviorEvent(pub u16);
#[derive(Clone)]
pub struct Event {
    kind: GestureKind,
    fields: Vec<StateField>,
    behavior: Option<BehaviorEvent>,
    operation: Operation,
}
impl Event {
    pub fn slider(field: StateField) -> Self {
        Self {
            kind: GestureKind::Slider,
            fields: vec![field],
            behavior: None,
            operation: Operation::Set,
        }
    }
    pub fn dial(field: StateField) -> Self {
        Self {
            kind: GestureKind::Dial,
            fields: vec![field],
            behavior: None,
            operation: Operation::Set,
        }
    }
    pub fn pad(x: StateField, y: StateField) -> Self {
        Self {
            kind: GestureKind::Pad,
            fields: vec![x, y],
            behavior: None,
            operation: Operation::Set,
        }
    }
    pub fn press(field: StateField) -> Self {
        Self {
            kind: GestureKind::Activate,
            fields: vec![field],
            behavior: None,
            operation: Operation::Set,
        }
    }
    pub fn toggle(field: StateField) -> Self {
        let mut e = Self::press(field);
        e.operation = Operation::Toggle;
        e
    }
    pub fn cycle(field: StateField, values: &[u16]) -> Self {
        let mut e = Self::press(field);
        e.operation = Operation::Cycle(values.to_vec());
        e
    }
    pub fn focus(field: StateField) -> Self {
        let mut e = Self::press(field);
        e.operation = Operation::Focus;
        e
    }
    pub fn key(key: u16) -> Self {
        Self {
            kind: GestureKind::Activate,
            fields: vec![],
            behavior: None,
            operation: Operation::Key(key),
        }
    }
    pub fn back() -> Self {
        Self {
            kind: GestureKind::Activate,
            fields: vec![],
            behavior: None,
            operation: Operation::Back,
        }
    }
    pub fn sign_out() -> Self {
        Self {
            kind: GestureKind::Activate,
            fields: vec![],
            behavior: None,
            operation: Operation::SignOut,
        }
    }
    pub fn stress() -> Self {
        Self {
            kind: GestureKind::Activate,
            fields: vec![],
            behavior: None,
            operation: Operation::Stress,
        }
    }
    pub fn reduced_by(mut self, event: BehaviorEvent) -> Self {
        self.behavior = Some(event);
        self
    }
}
#[derive(Clone)]
enum Interaction {
    Navigate(Destination),
    Event(Event),
}
#[derive(Clone)]
enum Paint {
    Canvas,
    Text {
        value: ValueExpr,
        font_scale: f32,
        color: ValueExpr,
    },
    Round {
        radius: ValueExpr,
        color: ValueExpr,
    },
    Circle(ValueExpr),
    Series {
        value: ValueExpr,
        timed: bool,
        color: [u8; 4],
    },
}
#[derive(Clone)]
pub struct Element {
    frame: [ValueExpr; 4],
    paint: Paint,
    children: Vec<Element>,
    label: Option<String>,
    interaction: Option<Interaction>,
    resize: Option<[AxisResize; 2]>,
    frame_size: [f32; 2],
    design_size: Option<[f32; 2]>,
    visible: ValueExpr,
}
impl Element {
    fn new(paint: Paint, frame: [f32; 4]) -> Self {
        Self {
            frame: frame.map(ValueExpr::from),
            paint,
            children: vec![],
            label: None,
            interaction: None,
            resize: None,
            frame_size: [frame[2], frame[3]],
            design_size: None,
            visible: ValueExpr::Literal(yes()),
        }
    }
    pub fn visible(mut self, value: ValueExpr) -> Self {
        self.visible = value;
        self
    }
    pub fn semantic_label(mut self, label: &str) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn frame(mut self, frame: [f32; 4]) -> Self {
        self.frame = frame.map(ValueExpr::from);
        self.frame_size = [frame[2], frame[3]];
        self
    }
    pub fn position(mut self, xy: [f32; 2]) -> Self {
        self.frame[0] = xy[0].into();
        self.frame[1] = xy[1].into();
        self
    }
    pub fn x(mut self, value: ValueExpr) -> Self {
        self.frame[0] = value;
        self
    }
    pub fn y(mut self, value: ValueExpr) -> Self {
        self.frame[1] = value;
        self
    }
    pub fn width(mut self, value: ValueExpr) -> Self {
        self.frame[2] = value;
        self
    }
    fn pin_axis(&mut self, axis: usize, leading: bool, inset: f32) {
        let rules = self.resize.get_or_insert([AxisResize::Fixed; 2]);
        let current = match rules[axis] {
            AxisResize::Fixed => AxisResize::Pins {
                leading: None,
                trailing: None,
            },
            v @ AxisResize::Pins { .. } => v,
            _ => panic!("Pins cannot be combined with centering or springs on the same axis"),
        };
        let AxisResize::Pins {
            leading: mut a,
            trailing: mut b,
        } = current
        else {
            unreachable!()
        };
        if leading {
            a = Some(inset)
        } else {
            b = Some(inset)
        }
        rules[axis] = AxisResize::Pins {
            leading: a,
            trailing: b,
        };
    }
    pub fn pin_left(mut self, inset: f32) -> Self {
        self.pin_axis(0, true, inset);
        self
    }
    pub fn pin_right(mut self, inset: f32) -> Self {
        self.pin_axis(0, false, inset);
        self
    }
    pub fn pin_top(mut self, inset: f32) -> Self {
        self.pin_axis(1, true, inset);
        self
    }
    pub fn pin_bottom(mut self, inset: f32) -> Self {
        self.pin_axis(1, false, inset);
        self
    }
    pub fn pin_edges(self, inset: f32) -> Self {
        self.pin_left(inset)
            .pin_right(inset)
            .pin_top(inset)
            .pin_bottom(inset)
    }
    fn set_axis(&mut self, axis: usize, rule: AxisResize) {
        let rules = self.resize.get_or_insert([AxisResize::Fixed; 2]);
        assert!(
            matches!(rules[axis], AxisResize::Fixed),
            "Choose one resize policy per axis"
        );
        rules[axis] = rule;
    }
    pub fn resizing(mut self, rules: Resizing) -> Self {
        if !matches!(rules.horizontal, AxisResize::Fixed) {
            self.set_axis(0, rules.horizontal)
        }
        if !matches!(rules.vertical, AxisResize::Fixed) {
            self.set_axis(1, rules.vertical)
        }
        self
    }
    pub fn center_x(mut self) -> Self {
        self.set_axis(0, AxisResize::Center { offset: 0. });
        self
    }
    pub fn center_y(mut self) -> Self {
        self.set_axis(1, AxisResize::Center { offset: 0. });
        self
    }
    pub fn spring_width(mut self) -> Self {
        self.set_axis(
            0,
            AxisResize::Springs {
                leading: false,
                length: true,
                trailing: false,
            },
        );
        self
    }
    pub fn spring_height(mut self) -> Self {
        self.set_axis(
            1,
            AxisResize::Springs {
                leading: false,
                length: true,
                trailing: false,
            },
        );
        self
    }
    pub fn font_scale(mut self, scale: f32) -> Self {
        match &mut self.paint {
            Paint::Text { font_scale, .. } => *font_scale = scale,
            _ => panic!("font_scale requires a Text primitive"),
        }
        self
    }
    pub fn color(mut self, value: impl Into<ValueExpr>) -> Self {
        match &mut self.paint {
            Paint::Text { color, .. } | Paint::Round { color, .. } | Paint::Circle(color) => {
                *color = value.into()
            }
            _ => panic!("color requires a painted primitive"),
        }
        self
    }
    pub fn corner_radius(mut self, value: ValueExpr) -> Self {
        match &mut self.paint {
            Paint::Round { radius, .. } => *radius = value,
            _ => panic!("corner_radius requires a Superellipse primitive"),
        }
        self
    }
    pub fn navigate_to(mut self, destination: Destination) -> Self {
        self.interaction = Some(Interaction::Navigate(destination));
        self
    }
    pub fn on_drag(mut self, label: &str, event: Event) -> Self {
        self.label = Some(label.into());
        self.interaction = Some(Interaction::Event(event));
        self
    }
    pub fn on_press(mut self, event: Event) -> Self {
        self.interaction = Some(Interaction::Event(event));
        self
    }
}
pub fn text(value: impl Into<ValueExpr>) -> Element {
    Element::new(
        Paint::Text {
            value: value.into(),
            font_scale: 2.,
            color: WHITE.into(),
        },
        [0., 0., 1100., 48.],
    )
}
pub fn rounded_rect(color: [u8; 4]) -> Element {
    Element::new(
        Paint::Round {
            radius: 16.0.into(),
            color: color.into(),
        },
        [0., 0., 100., 100.],
    )
}
pub fn rectangle(color: [u8; 4]) -> Element {
    rounded_rect(color).corner_radius(0.0.into())
}
pub fn circle(color: [u8; 4]) -> Element {
    Element::new(Paint::Circle(color.into()), [0., 0., 100., 100.])
}
pub fn series(value: ValueExpr, color: [u8; 4]) -> Element {
    Element::new(
        Paint::Series {
            value,
            timed: false,
            color,
        },
        [0., 0., 100., 100.],
    )
}
pub fn timed_series(value: ValueExpr, color: [u8; 4]) -> Element {
    Element::new(
        Paint::Series {
            value,
            timed: true,
            color,
        },
        [0., 0., 100., 100.],
    )
}
pub fn session_controls_visible() -> ValueExpr {
    ValueExpr::State(
        StateField::normalized(0, "session", 0),
        Projection::SessionControls,
    )
}
pub fn keyboard_visible() -> ValueExpr {
    ValueExpr::State(
        StateField::normalized(0, "keyboard", 0),
        Projection::KeyboardVisible,
    )
}
pub fn keyboard_label(key: u16) -> Element {
    text(ValueExpr::State(
        StateField::normalized(0, "keyboard", 0),
        Projection::KeyboardLabel { key },
    ))
}
pub fn canvas(children: Vec<Element>) -> Element {
    let mut n = Element::new(Paint::Canvas, SCREEN);
    n.children = children;
    n
}
pub fn button(label: &str) -> Element {
    let mut b = canvas(vec![
        rounded_rect(CARD).pin_edges(0.),
        text(label)
            .frame([24., 0., 152., 16.])
            .pin_left(24.)
            .pin_right(24.)
            .center_y(),
    ])
    .frame([0., 0., 200., 80.]);
    b.label = Some(label.into());
    b
}
/// A reusable component's design size is retained when an instance gets a new frame.
pub fn component(design_size: [f32; 2], children: Vec<Element>) -> Element {
    let mut n = canvas(children).frame([0., 0., design_size[0], design_size[1]]);
    n.design_size = Some(design_size);
    n
}
pub struct AppUi {
    id: String,
    title: String,
    schema: StateSchema,
    behavior: Option<BehaviorRef>,
    presentation: Option<Presentation>,
}
impl AppUi {
    pub fn new(id: &str, title: &str, schema: StateSchema) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            schema,
            behavior: None,
            presentation: None,
        }
    }
    pub fn with_presentation(mut self, presentation: Presentation) -> Self {
        self.presentation = Some(presentation);
        self
    }
    pub fn with_behavior(mut self, code: &[u8], writes: &[StateField]) -> Self {
        self.behavior = Some(BehaviorRef {
            sha256: otrios_core::canvas::hash(code),
            abi: 1,
            writes: writes.iter().map(|f| f.id).collect(),
        });
        self
    }
    pub fn build(self, heading: &str, mut children: Vec<Element>) -> Ui {
        let mut body = vec![
            rounded_rect(BG).frame(SCREEN),
            text(heading).position([52., 45.]).font_scale(4.),
        ];
        body.append(&mut children);
        let mut compiler = Compiler {
            next_node: 0,
            selectors: vec![],
            handlers: vec![],
            navigation: vec![],
            has_behavior: self.behavior.is_some(),
        };
        let root = compiler.node(canvas(body), [SCREEN[2], SCREEN[3]]);
        let layout = otrios_core::dashboard::layout::Layout::encode(&Blueprint {
            schema: "otrios-layout-v2".into(),
            templates: vec![Template {
                id: 1,
                name: "App canvas".into(),
                parameters: vec![],
                root,
            }],
            root: Instance {
                id: 0,
                template: 1,
                frame: SCREEN,
                properties: vec![],
                children: vec![],
            },
        })
        .expect("valid authored app primitives");
        Ui {
            format: "otrios-canvas-v2".into(),
            ui_ref: format!("app:{}/ui", self.id),
            title: self.title,
            state_schema: self.schema,
            layout,
            selectors: compiler.selectors,
            handlers: compiler.handlers,
            navigation: compiler.navigation,
            behavior: self.behavior,
            presentation: self.presentation,
        }
    }
}
struct Compiler {
    next_node: u16,
    selectors: Vec<Selector>,
    handlers: Vec<Handler>,
    navigation: Vec<Navigation>,
    has_behavior: bool,
}
impl Compiler {
    fn value(&mut self, value: ValueExpr) -> Expr {
        match value {
            ValueExpr::Literal(e) => e,
            ValueExpr::State(field, projection) => {
                let local = if let Some(s) = self
                    .selectors
                    .iter()
                    .find(|s| s.field == field.id && s.projection == projection)
                {
                    s.local
                } else {
                    let local = u16::try_from(self.selectors.len() + 1).expect("selector index");
                    self.selectors.push(Selector {
                        local,
                        field: field.id,
                        projection,
                    });
                    local
                };
                Expr::State(Binding::Local(local))
            }
        }
    }
    fn node(&mut self, n: Element, parent_reference: [f32; 2]) -> TemplateNode {
        let id = self.next_node;
        self.next_node = self.next_node.checked_add(1).expect("node budget");
        let child_reference = n.design_size.unwrap_or(n.frame_size);
        let frame = n.frame.map(|v| self.value(v));
        let shape = match n.paint {
            Paint::Canvas => Shape::Canvas { clip: true },
            Paint::Text {
                value,
                font_scale,
                color,
            } => Shape::Text {
                value: self.value(value),
                scale: Expr::Literal(Value::Number(font_scale)),
                color: self.value(color),
            },
            Paint::Round { radius, color } => Shape::Superellipse {
                radius: self.value(radius),
                exponent: 4.,
                color: self.value(color),
            },
            Paint::Circle(color) => Shape::Circle {
                color: self.value(color),
            },
            Paint::Series {
                value,
                timed: false,
                color,
            } => Shape::SeriesPath {
                series: self.value(value),
                width: 2.,
                color: tint(color),
            },
            Paint::Series {
                value,
                timed: true,
                color,
            } => Shape::TimedSeriesPath {
                series: self.value(value),
                width: 2.,
                color: tint(color),
            },
        };
        let responder = n.interaction.map(|i| match i {
            Interaction::Navigate(d) => {
                let action = if let Some(n) = self
                    .navigation
                    .iter()
                    .find(|n| n.ui_ref == d.ui_ref && n.state_ref == d.state_ref)
                {
                    n.action
                } else {
                    let action = u8::try_from(self.navigation.len()).expect("navigation budget");
                    self.navigation.push(Navigation {
                        action,
                        ui_ref: d.ui_ref,
                        state_ref: d.state_ref,
                    });
                    action
                };
                Responder::AppNavigate(action)
            }
            Interaction::Event(e) => {
                let control = u8::try_from(self.handlers.len()).expect("control budget");
                assert_eq!(
                    e.behavior.is_some(),
                    self.has_behavior,
                    "custom behavior events must declare their guest ABI mapping"
                );
                let drag = e.kind != GestureKind::Activate;
                self.handlers.push(Handler {
                    control,
                    event: e.behavior.map(|e| e.0).unwrap_or(0),
                    kind: e.kind,
                    operation: e.operation,
                    fields: e.fields.iter().map(|f| f.id).collect(),
                });
                Responder::AppEvent { control, drag }
            }
        });
        let semantics = if responder.is_some() {
            Some(Semantics {
                label: Expr::Literal(Value::Text(
                    n.label.expect("interactive element needs a semantic label"),
                )),
                checked: no(),
            })
        } else {
            None
        };
        TemplateNode {
            id,
            resize: n.resize.map(|r| ResizeRule {
                reference_size: parent_reference,
                horizontal: r[0],
                vertical: r[1],
            }),
            frame,
            visible: self.value(n.visible),
            shape,
            responder,
            semantics,
            children: n
                .children
                .into_iter()
                .map(|child| self.node(child, child_reference))
                .collect(),
        }
    }
}
pub fn app(id: &str, title: &str, status: &str, deps: &[&str]) -> App {
    App {
        id: id.into(),
        title: title.into(),
        ui_ref: format!("app:{id}/ui"),
        state_ref: format!("app:{id}/state"),
        status: status.into(),
        dependencies: deps.iter().map(|v| (*v).into()).collect(),
    }
}
