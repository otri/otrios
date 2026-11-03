// SPDX-License-Identifier: LGPL-3.0-only
//! Local tooling geometry. Authored preview geometry and stable IDs are retained.
use crate::dashboard::{
    components::{CARD, MUTED, WHITE, background, canvas, text},
    scene::Scene,
};
use otrios_interfaces::{
    oib::Seed,
    scene::{Action, Primitive, Rect, SceneNode, SwitchSemantics, TouchResponder},
};

// Below the monotonically allocated designer controls and above authored u16 IDs.
pub(super) const LAYERS: u32 = 2_690_000_001;
const CHROME: u32 = 2_690_000_000;
pub(super) const HEADER: f32 = 64.;
pub(super) const DOCK: f32 = 352.;
pub(super) const DRAWER: f32 = 320.;

fn find(node: &SceneNode, id: u32) -> Option<&SceneNode> {
    if node.id == id {
        Some(node)
    } else {
        node.children.iter().find_map(|n| find(n, id))
    }
}
fn back(node: &SceneNode, action: Option<u8>) -> Option<&SceneNode> {
    if action.is_some_and(|a| node.responder == Some(TouchResponder::Activate(Action::AppEvent(a))))
    {
        Some(node)
    } else {
        node.children.iter().find_map(|n| back(n, action))
    }
}

/// Transform both the layout/hit-test scene and its later paint projection.
/// The closed drawer remains clipped outside the root so local scroll identity
/// and bounds survive opening it. It contributes no visible or semantic target.
pub(super) fn project(
    scene: &Scene,
    seed: &Seed,
    back_action: Option<u8>,
    open: bool,
    keyboard: bool,
) -> Result<Scene, String> {
    let doc = seed.designer.as_ref().ok_or("Missing designer metadata")?;
    let hierarchy = doc.hierarchy_viewport.ok_or("Missing hierarchy viewport")?;
    let preview = seed
        .instance_surface
        .as_ref()
        .ok_or("Missing designer preview")?
        .preview_viewport;
    let mut preview = find(scene.root(), u32::from(preview))
        .ok_or("Missing preview viewport")?
        .clone();
    let mut inspector = find(scene.root(), u32::from(seed.inspector_viewport))
        .ok_or("Missing inspector viewport")?
        .clone();
    let mut hierarchy = find(scene.root(), u32::from(hierarchy))
        .ok_or("Missing hierarchy viewport")?
        .clone();
    let width = scene.root().frame.width;
    let height = scene.root().frame.height - if keyboard { 240. } else { 0. };
    // Keep a usable canvas even on narrow windows. Tooling remains physical-size;
    // this is not a scale transform of the authored document or its text.
    let dock = DOCK.min((width - 240.).max(240.));
    let canvas_width = (width - dock - 48.).max(1.);
    let body_height = (height - HEADER - 32.).max(1.);
    preview.frame = Rect::new(16., HEADER + 16., canvas_width, body_height);
    inspector.frame = Rect::new(16., 56., (dock - 32.).max(1.), (body_height - 72.).max(1.));
    hierarchy.frame = Rect::new(16., 64., DRAWER - 32., (height - HEADER - 80.).max(1.));
    let mut layers = canvas(
        LAYERS,
        Rect::new(80., 8., 128., 48.),
        vec![text(
            CHROME + 2,
            Rect::new(12., 14., 112., 28.),
            "Layers",
            2,
            WHITE,
        )],
    );
    layers.responder = Some(TouchResponder::Activate(Action::KeyboardKey(u16::MAX)));
    layers.semantics = Some(SwitchSemantics {
        label: "Layers".into(),
        checked: open,
    });
    let mut header = vec![
        background(CHROME + 3, width, HEADER, CARD, 0.),
        layers,
        text(
            CHROME + 4,
            Rect::new(232., 22., (width - 248.).max(1.), 28.),
            "OIB Designer",
            2,
            WHITE,
        ),
    ];
    if let Some(original) = back(scene.root(), back_action) {
        // Keep the signed responder and semantic identity; only local chrome
        // geometry/decoration changes. The authority still admits the action.
        let mut button = original.clone();
        button.frame = Rect::new(16., 8., 48., 48.);
        button.children = vec![text(
            CHROME + 5,
            Rect::new(14., 14., 28., 28.),
            "<",
            2,
            WHITE,
        )];
        button.primitive = Primitive::Canvas { clip: true };
        header.push(button);
    }
    let dock = canvas(
        CHROME + 6,
        Rect::new(width - dock - 16., HEADER + 16., dock, body_height),
        vec![
            background(CHROME + 7, dock, body_height, CARD, 12.),
            text(
                CHROME + 8,
                Rect::new(16., 16., dock - 32., 28.),
                "Properties",
                2,
                WHITE,
            ),
            inspector,
        ],
    );
    let mut drawer = canvas(
        CHROME + 9,
        Rect::new(
            if open { 0. } else { -DRAWER },
            HEADER,
            DRAWER,
            (height - HEADER).max(1.),
        ),
        vec![
            background(CHROME + 10, DRAWER, (height - HEADER).max(1.), CARD, 0.),
            text(
                CHROME + 11,
                Rect::new(16., 20., DRAWER - 32., 28.),
                "Layers",
                2,
                MUTED,
            ),
            hierarchy,
        ],
    );
    drawer.responder = Some(TouchResponder::Consume);
    let mut root = canvas(scene.root().id, scene.root().frame, vec![preview, dock]);
    root.children.extend(header);
    root.children.push(drawer);
    Scene::new(root)
}
