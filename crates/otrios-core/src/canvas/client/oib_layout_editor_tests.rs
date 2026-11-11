// SPDX-License-Identifier: LGPL-3.0-only
//! Inspector controls author canonical layout and publish through real receipts.
use super::tests::{focus_address, mounted_fixture_session};
use super::*;
use otrios_interfaces::{
    keyboard::Edit,
    layout::{Direction, Expr, Placement, Shape, Sizing},
    oib::designer::{DefinitionId, NodeId, StructuralProperty as P, StructuralTarget},
};

fn settle(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
    let mount = client.active.as_mut().unwrap();
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = mount.doc.generate().unwrap() {
            authority.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = authority.generate(session).unwrap() {
            mount.doc.receive(&bytes).unwrap();
            mount.refresh().unwrap();
            progress = true;
        }
        if !progress {
            assert!(mount.doc.confirmed());
            return;
        }
    }
    panic!("layout receipt did not settle");
}
fn scroll_to(client: &mut Client, y: f32) {
    let viewport = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .panels
        .last()
        .unwrap()
        .viewport;
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(viewport)).unwrap();
    let p = path.last().unwrap();
    let point = [p.origin[0] + 10., p.origin[1] + 10.];
    let old = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .panels
        .last()
        .unwrap()
        .scroll
        .info()
        .offset[1];
    client
        .scroll_at(client.epoch(), y - 8. - old, point)
        .unwrap();
}
fn control(client: &mut Client, label: &str) -> u32 {
    let (id, y) = {
        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        let n = d
            .panels
            .last()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.semantics.as_ref().is_some_and(|s| s.label == label))
            .unwrap_or_else(|| panic!("Missing layout control {label}"));
        (n.id, n.frame.y)
    };
    scroll_to(client, y);
    assert!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .any(|c| c.node.id == id)
    );
    id
}
fn activate(client: &mut Client, label: &str) {
    let id = control(client, label);
    assert!(client.activate(client.epoch(), id).unwrap(), "{label}");
}
fn visual(
    client: &Client,
    definition: DefinitionId,
    node: NodeId,
) -> otrios_interfaces::oib::graph::Visual {
    client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_authoring_seed()
        .unwrap()
        .graph
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == node)
        .unwrap()
        .visual
        .clone()
        .unwrap()
}
fn address(definition: DefinitionId, node: NodeId, property: P) -> PropertyAddress {
    PropertyAddress::Structural {
        target: StructuralTarget::Node { definition, node },
        property,
    }
}
fn replace(client: &mut Client, address: PropertyAddress, value: &str) {
    let field = focus_address(client, address);
    client
        .edit_text_input(
            client.epoch(),
            field,
            &Edit::Replace {
                text: value.into(),
                cursor: value.len() as u32,
                anchor: value.len() as u32,
                selection_only: false,
            },
        )
        .unwrap();
    client
        .designer_commit_property(
            &client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .drafts
                .focused()
                .unwrap()
                .address
                .clone(),
            true,
        )
        .unwrap();
}
fn blank() -> (
    Client,
    crate::canvas::Document,
    Session,
    DefinitionId,
    NodeId,
    u64,
) {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.active.as_mut().unwrap().font = Some(Arc::new(crate::dashboard::font::test_font()));
    // This fixture deliberately fills its admitted six-root canvas. Reuse one
    // disposable duplicate slot rather than changing the production limit.
    let duplicate = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|instance| instance.id != instance.prototype)
        .unwrap()
        .id;
    assert!(client.remove_oib(duplicate).unwrap());
    settle(&mut client, &mut authority, &mut session);
    client
        .designer_action(LocalAction::Workspace(Task::Library))
        .unwrap();
    let operation = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .structure_tools
        .iter()
        .find(|(label, _)| label == "New component")
        .unwrap()
        .1
        .clone();
    assert!(
        client
            .designer_action(LocalAction::Structure(operation))
            .unwrap()
    );
    settle(&mut client, &mut authority, &mut session);
    let graph = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_authoring_seed()
        .unwrap()
        .graph
        .unwrap();
    let definition = graph
        .definitions
        .iter()
        .find(|d| d.name == "New component")
        .unwrap();
    let ids = (definition.id, definition.root);
    let preset = graph
        .presets
        .iter()
        .find(|p| p.definition == ids.0)
        .unwrap()
        .id;
    let before = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_allocations()
        .unwrap()
        .into_iter()
        .map(|allocation| allocation.id)
        .collect::<Vec<_>>();
    assert!(
        client.insert_oib(preset, None).unwrap(),
        "status={} confirmed={} busy={} blocked={} dirty={}",
        client.active.as_ref().unwrap().doc.oib_status_text(),
        client.active.as_ref().unwrap().doc.confirmed(),
        client.input_busy(),
        client.active.as_ref().unwrap().oib_activation_blocked(),
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .drafts
            .drafts()
            .iter()
            .any(|d| d.dirty)
    );
    settle(&mut client, &mut authority, &mut session);
    let instance = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_allocations()
        .unwrap()
        .into_iter()
        .find(|a| a.prototype == preset && !before.contains(&a.id))
        .unwrap()
        .id;
    client
        .inspect_oib_selection(Selection::Definition(ids.0))
        .unwrap();
    (client, authority, session, ids.0, ids.1, instance)
}
fn frame(client: &mut Client, instance: u64) -> Rect {
    let node=client.active.as_ref().unwrap().designer.as_ref().unwrap().objects.iter().find_map(|object|matches!(&object.selection,Selection::Object(address) if address.instance.0==instance && address.references.is_empty()).then_some(object.layout_node).flatten()).unwrap();
    client
        .scene()
        .unwrap()
        .path(u32::from(node))
        .unwrap()
        .last()
        .unwrap()
        .node
        .frame
}

#[test]
fn layout_editor_blank_and_padding_edit_change_measured_geometry_with_receipts() {
    let (mut client, mut authority, mut session, definition, node, instance) = blank();
    let initial = visual(&client, definition, node);
    let layout = initial.layout.unwrap();
    assert_eq!(layout.width.mode, Sizing::Fixed);
    assert_eq!(layout.height.mode, Sizing::Hug);
    assert_eq!(layout.height.min, 48.);
    let flow = layout.container.unwrap();
    assert_eq!(flow.direction, Direction::Vertical);
    assert_eq!(flow.padding, [16.; 4]);
    assert_eq!(flow.gap, 8.);
    assert_eq!(frame(&mut client, instance).height, 48.);
    let values = [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap());
    replace(&mut client, address(definition, node, P::Padding), "40");
    assert!(!client.active.as_ref().unwrap().doc.confirmed());
    settle(&mut client, &mut authority, &mut session);
    assert_eq!(
        visual(&client, definition, node)
            .layout
            .unwrap()
            .container
            .unwrap()
            .padding,
        [40.; 4]
    );
    assert_eq!(frame(&mut client, instance).height, 80.);
    assert_eq!(
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap()),
        values
    );
}
#[test]
fn layout_editor_axis_modes_are_independent_and_old_controls_do_not_retarget() {
    let (mut client, mut authority, mut session, definition, node, _) = blank();
    let root = client.active.as_ref().unwrap().doc.oib_root();
    let fixed = control(&mut client, "Width Fixed");
    assert!(!client.activate(client.epoch(), fixed).unwrap());
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    let panel = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .panels
        .last()
        .unwrap();
    assert!(
        !panel.nodes.iter().any(|node| node
            .semantics
            .as_ref()
            .is_some_and(|s| s.label == "Design height")),
        "Hug height must not expose an ineffective fixed value"
    );
    let align = panel
        .nodes
        .iter()
        .find(|node| {
            node.semantics
                .as_ref()
                .is_some_and(|s| s.label == "Align top left")
        })
        .unwrap();
    let padding = panel
        .nodes
        .iter()
        .find(|node| {
            node.semantics
                .as_ref()
                .is_some_and(|s| s.label == "Padding")
        })
        .unwrap();
    assert!(padding.frame.x >= 164. && padding.frame.y < align.frame.y + 156.);
    let stale = control(&mut client, "Width Fill");
    activate(&mut client, "Height Fixed");
    settle(&mut client, &mut authority, &mut session);
    assert!(!client.activate(client.epoch(), stale).unwrap());
    let policy = visual(&client, definition, node).layout.unwrap();
    assert_eq!(policy.width.mode, Sizing::Fixed);
    assert_eq!(policy.height.mode, Sizing::Fixed);
    activate(&mut client, "Width Hug");
    settle(&mut client, &mut authority, &mut session);
    let policy = visual(&client, definition, node).layout.unwrap();
    assert_eq!(policy.width.mode, Sizing::Hug);
    assert_eq!(policy.height.mode, Sizing::Fixed);
    activate(&mut client, "Free");
    settle(&mut client, &mut authority, &mut session);
    assert_eq!(
        visual(&client, definition, node).layout.unwrap().placement,
        Placement::Free
    );
    replace(&mut client, address(definition, node, P::X), "-12");
    settle(&mut client, &mut authority, &mut session);
    assert_eq!(
        visual(&client, definition, node).frame[0],
        Expr::Literal(Value::Number(-12.))
    );
}
#[test]
fn layout_editor_per_edge_padding_and_bounds_keep_source_and_drafts_bounded() {
    let (mut client, mut authority, mut session, definition, node, _) = blank();
    let generation = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_structural_generation();
    activate(&mut client, "Independent padding");
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_structural_generation(),
        generation
    );
    replace(&mut client, address(definition, node, P::PaddingLeft), "8");
    settle(&mut client, &mut authority, &mut session);
    replace(
        &mut client,
        address(definition, node, P::PaddingRight),
        "24",
    );
    settle(&mut client, &mut authority, &mut session);
    assert_eq!(
        visual(&client, definition, node)
            .layout
            .unwrap()
            .container
            .unwrap()
            .padding,
        [8., 16., 24., 16.]
    );
    activate(&mut client, "Independent padding");
    settle(&mut client, &mut authority, &mut session);
    assert_eq!(
        visual(&client, definition, node)
            .layout
            .unwrap()
            .container
            .unwrap()
            .padding,
        [8.; 4]
    );
    activate(&mut client, "Size bounds");
    replace(&mut client, address(definition, node, P::MinWidth), "120");
    settle(&mut client, &mut authority, &mut session);
    let root = client.active.as_ref().unwrap().doc.oib_root();
    replace(&mut client, address(definition, node, P::MaxWidth), "80");
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    let designer = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(
        designer
            .editor_notice
            .as_ref()
            .is_some_and(|n| n.contains("Minimum exceeds maximum"))
    );
    assert!(designer.drafts.drafts().len() <= super::super::oib_property_editor::MAX_DRAFTS);
    let scene = client.scene().unwrap();
    assert!(scene.controls().len() <= 128);
}

#[test]
fn layout_captions_and_checkbox_render_with_admitted_font_and_keep_input() {
    let (mut client, mut authority, mut session, definition, node, _) = blank();
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    for heading in ["Width", "Height", "Gap", "Padding", "Distribution"] {
        let label = d
            .panels
            .last()
            .unwrap()
            .nodes
            .iter()
            .find(|n| matches!(&n.primitive, Primitive::Text { value, .. } if value == heading))
            .unwrap();
        assert!(matches!(label.primitive, Primitive::Text { scale, .. } if scale >= 2));
    }
    let usage = client.resource_usage().designer.unwrap();
    assert_eq!(usage.task, "Inspect");
    assert!(usage.authoring);
    assert_eq!(
        usage.selection,
        format!(
            "{:?}",
            client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .selection
        )
    );
    let id = control(&mut client, "Clip content");
    let scene = client.scene().unwrap();
    let checkbox = scene.path(id).unwrap().last().unwrap().node;
    assert!(checkbox.semantics.as_ref().unwrap().checked);
    assert!(
        checkbox
            .children
            .iter()
            .any(|n| matches!(n.primitive, Primitive::Path { .. })
                && n.responder == Some(TouchResponder::PassThrough))
    );
    assert!(checkbox.children.iter().all(
        |n| !matches!(&n.primitive, Primitive::Text { value, .. } if value.contains(['✓','□']))
    ));
    assert_eq!(client.designer_activate(id).unwrap(), Some(true));
    settle(&mut client, &mut authority, &mut session);
    assert!(matches!(
        visual(&client, definition, node).shape,
        Shape::Canvas { clip: false }
    ));
}
