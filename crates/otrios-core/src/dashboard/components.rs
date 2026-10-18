// SPDX-License-Identifier: LGPL-3.0-only
//! Components are factories, never special compositor/hit-test node kinds.
use super::{Result, scene::Scene};
use otrios_interfaces::{DashboardView, ViewContent, scene::*};
pub const BG: Color = [16, 28, 41, 255];
pub const CARD: Color = [27, 44, 61, 255];
pub const WHITE: Color = [239, 244, 236, 255];
pub const MUTED: Color = [150, 171, 185, 255];
pub const ACCENT: Color = [185, 238, 143, 255];
pub fn node(id: NodeId, frame: Rect, primitive: Primitive) -> SceneNode {
    SceneNode {
        id,
        frame,
        primitive,
        children: vec![],
        responder: None,
        semantics: None,
    }
}
pub fn canvas(id: NodeId, frame: Rect, children: Vec<SceneNode>) -> SceneNode {
    let mut n = node(id, frame, Primitive::Canvas { clip: true });
    n.children = children;
    n
}
pub fn background(id: NodeId, w: f32, h: f32, color: Color, radius: f32) -> SceneNode {
    node(
        id,
        Rect::new(0., 0., w, h),
        Primitive::Superellipse {
            color,
            radius,
            exponent: 4.,
        },
    )
}
pub fn text(id: NodeId, frame: Rect, value: &str, scale: u32, color: Color) -> SceneNode {
    node(
        id,
        frame,
        Primitive::Text {
            value: value.into(),
            scale,
            color,
        },
    )
}
/// Reserve id..id+999 for one component; descendants stay stable across state edits.
pub fn panel(id: NodeId, frame: Rect, color: Color, children: Vec<SceneNode>) -> SceneNode {
    let mut all = vec![background(id + 1, frame.width, frame.height, color, 16.)];
    all.extend(children);
    canvas(id, frame, all)
}
pub fn switch(id: NodeId, frame: Rect, label: &str, checked: bool, action: Action) -> SceneNode {
    let fg = if checked { BG } else { WHITE };
    let w = frame.width;
    let h = frame.height;
    let track = canvas(
        id + 20,
        Rect::new(w - 164., 84., 132., 64.),
        vec![
            background(
                id + 21,
                132.,
                64.,
                if checked {
                    [56, 84, 55, 255]
                } else {
                    [74, 94, 113, 255]
                },
                32.,
            ),
            node(
                id + 22,
                Rect::new(if checked { 72. } else { 8. }, 8., 48., 48.),
                Primitive::Circle { color: WHITE },
            ),
        ],
    );
    let mut n = panel(
        id,
        frame,
        if checked { ACCENT } else { CARD },
        vec![
            text(id + 2, Rect::new(32., 32., w - 64., 24.), label, 3, fg),
            text(
                id + 3,
                Rect::new(32., 88., 160., 40.),
                if checked { "ON" } else { "OFF" },
                5,
                fg,
            ),
            track,
            text(
                id + 4,
                Rect::new(32., h - 42., w - 64., 16.),
                "TAP TO CHANGE",
                2,
                fg,
            ),
        ],
    );
    n.responder = Some(TouchResponder::Activate(action));
    n.semantics = Some(SwitchSemantics {
        label: label.into(),
        checked,
    });
    n
}
fn graph(id: NodeId, frame: Rect, label: &str, samples: &[u32]) -> SceneNode {
    let w = frame.width;
    let h = frame.height;
    let gw = w - 80.;
    let gh = h - 184.;
    let mut children = vec![
        text(id + 2, Rect::new(32., 32., w - 64., 24.), label, 3, WHITE),
        text(
            id + 3,
            Rect::new(32., 76., w - 64., 16.),
            "DEMO DATA  /  LAST 32 EVENTS",
            2,
            MUTED,
        ),
    ];
    for i in 0..=4 {
        children.push(node(
            id + 10 + i,
            Rect::new(40., 136. + i as f32 * gh / 4., gw, 1.),
            Primitive::Superellipse {
                radius: 0.,
                exponent: 2.,
                color: [48, 68, 85, 255],
            },
        ));
    }
    let points: Vec<_> = samples
        .iter()
        .enumerate()
        .map(|(i, v)| {
            [
                i as f32 * gw / (samples.len().max(2) - 1) as f32,
                gh - *v as f32 * gh / 100.,
            ]
        })
        .collect();
    children.push(node(
        id + 30,
        Rect::new(40., 136., gw, gh),
        Primitive::Path {
            points: points.clone(),
            width: 3.,
            color: ACCENT,
        },
    ));
    if let Some(p) = points.last() {
        children.push(node(
            id + 31,
            Rect::new(40. + p[0] - 7., 136. + p[1] - 7., 14., 14.),
            Primitive::Circle { color: ACCENT },
        ));
    }
    panel(id, frame, CARD, children)
}
pub fn dashboard(view: &DashboardView) -> Result<Scene> {
    let mut children = vec![
        background(2, 1280., 800., BG, 0.),
        text(
            1,
            Rect::new(64., 25., 1152., 16.),
            "OTRIOS  /  LIVE SPACES",
            2,
            MUTED,
        ),
    ];
    for n in &view.nodes {
        let id =
            n.id.checked_mul(1000)
                .filter(|id| *id >= 1000 && *id <= u32::MAX - 999)
                .ok_or("Invalid component identity")?;
        let [x, y, w, h] = n.bounds;
        let frame = Rect::new(x as f32, y as f32, w as f32, h as f32);
        let component = match &n.content {
            ViewContent::Label(value) => {
                let scale = if n.id == 1 {
                    6
                } else if n.id == 4 {
                    4
                } else {
                    2
                };
                let inset = if n.id == 4 { 24. } else { 0. };
                let label = text(
                    id + 2,
                    Rect::new(
                        inset,
                        (h as f32 - 8. * scale as f32) / 2.,
                        w as f32 - inset,
                        8. * scale as f32,
                    ),
                    value,
                    scale,
                    if n.id == 5 { MUTED } else { WHITE },
                );
                if n.id == 4 {
                    panel(id, frame, CARD, vec![label])
                } else {
                    canvas(id, frame, vec![label])
                }
            }
            ViewContent::Switch { label, enabled } => {
                switch(id, frame, label, *enabled, Action::ToggleFocus)
            }
            ViewContent::Graph { label, samples } => graph(id, frame, label, samples),
        };
        children.push(component);
    }
    let mut root = canvas(0, Rect::new(0., 0., 1280., 800.), children);
    root.responder = Some(TouchResponder::Bubble);
    Scene::new(root)
}
