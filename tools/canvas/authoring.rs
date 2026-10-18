// SPDX-License-Identifier: Apache-2.0
//! Declarative source builder. Numeric node/action/binding IDs are compiled details.
use otrios_interfaces::{canvas::*, layout::*};
pub const BG: [u8; 4] = [16, 28, 41, 255];
pub const CARD: [u8; 4] = [27, 44, 61, 255];
pub const WHITE: [u8; 4] = [239, 244, 236, 255];
pub const ACCENT: [u8; 4] = [185, 238, 143, 255];
const SCREEN: [f32; 4] = [0., 0., 1280., 800.];
const NORMALIZED_MAX: f32 = 1000.;

/// Stable schema identity, unlike the compiler's per-UI node/action indices.
#[derive(Clone, Copy)]
pub struct StateField {
    id: u16,
    name: &'static str,
    default: u16,
}
impl StateField {
    pub const fn normalized(id: u16, name: &'static str, default: u16) -> Self {
        Self { id, name, default }
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
                default: f.default,
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
}
impl Event {
    pub fn slider(field: StateField) -> Self {
        Self {
            kind: GestureKind::Slider,
            fields: vec![field],
            behavior: None,
        }
    }
    pub fn dial(field: StateField) -> Self {
        Self {
            kind: GestureKind::Dial,
            fields: vec![field],
            behavior: None,
        }
    }
    pub fn pad(x: StateField, y: StateField) -> Self {
        Self {
            kind: GestureKind::Pad,
            fields: vec![x, y],
            behavior: None,
        }
    }
    pub fn press(field: StateField) -> Self {
        Self {
            kind: GestureKind::Activate,
            fields: vec![field],
            behavior: None,
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
    Text { value: ValueExpr, font_scale: f32 },
    Round { radius: ValueExpr, color: [u8; 4] },
    Circle([u8; 4]),
}
#[derive(Clone)]
pub struct Element {
    frame: [ValueExpr; 4],
    paint: Paint,
    children: Vec<Element>,
    label: Option<String>,
    interaction: Option<Interaction>,
    fill_parent: bool,
}
impl Element {
    fn new(paint: Paint, frame: [f32; 4]) -> Self {
        Self {
            frame: frame.map(ValueExpr::from),
            paint,
            children: vec![],
            label: None,
            interaction: None,
            fill_parent: false,
        }
    }
    pub fn frame(mut self, frame: [f32; 4]) -> Self {
        self.frame = frame.map(ValueExpr::from);
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
    pub fn font_scale(mut self, scale: f32) -> Self {
        match &mut self.paint {
            Paint::Text { font_scale, .. } => *font_scale = scale,
            _ => panic!("font_scale requires a Text primitive"),
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
        },
        [0., 0., 1100., 48.],
    )
}
pub fn rounded_rect(color: [u8; 4]) -> Element {
    Element::new(
        Paint::Round {
            radius: 16.0.into(),
            color,
        },
        [0., 0., 100., 100.],
    )
}
pub fn circle(color: [u8; 4]) -> Element {
    Element::new(Paint::Circle(color), [0., 0., 100., 100.])
}
pub fn canvas(children: Vec<Element>) -> Element {
    let mut n = Element::new(Paint::Canvas, SCREEN);
    n.children = children;
    n
}
pub fn button(label: &str) -> Element {
    let mut background = rounded_rect(CARD);
    background.fill_parent = true;
    let mut b =
        canvas(vec![background, text(label).position([24., 26.])]).frame([0., 0., 200., 80.]);
    b.label = Some(label.into());
    b
}
pub struct AppUi {
    id: String,
    title: String,
    schema: StateSchema,
    behavior: Option<BehaviorRef>,
}
impl AppUi {
    pub fn new(id: &str, title: &str, schema: StateSchema) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            schema,
            behavior: None,
        }
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
        let root = compiler.node(canvas(body));
        let layout = otrios_core::dashboard::layout::Layout::encode(&Blueprint {
            schema: "otrios-layout-v1".into(),
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
            format: "otrios-canvas-v1".into(),
            ui_ref: format!("app:{}/ui", self.id),
            title: self.title,
            state_schema: self.schema,
            layout,
            selectors: compiler.selectors,
            handlers: compiler.handlers,
            navigation: compiler.navigation,
            behavior: self.behavior,
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
    fn node(&mut self, n: Element) -> TemplateNode {
        let id = self.next_node;
        self.next_node = self.next_node.checked_add(1).expect("node budget");
        let own_frame = n.frame.clone();
        let frame = n.frame.map(|v| self.value(v));
        let shape = match n.paint {
            Paint::Canvas => Shape::Canvas { clip: true },
            Paint::Text { value, font_scale } => Shape::Text {
                value: self.value(value),
                scale: Expr::Literal(Value::Number(font_scale)),
                color: tint(WHITE),
            },
            Paint::Round { radius, color } => Shape::Superellipse {
                radius: self.value(radius),
                exponent: 4.,
                color: tint(color),
            },
            Paint::Circle(color) => Shape::Circle { color: tint(color) },
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
            frame,
            visible: yes(),
            shape,
            responder,
            semantics,
            children: n
                .children
                .into_iter()
                .map(|mut child| {
                    if child.fill_parent {
                        child.frame = [
                            0.0.into(),
                            0.0.into(),
                            own_frame[2].clone(),
                            own_frame[3].clone(),
                        ];
                    }
                    self.node(child)
                })
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
