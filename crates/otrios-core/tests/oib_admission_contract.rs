// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;

use otrios_core::canvas::{self, oib};
use otrios_interfaces::{
    canvas::{GestureKind, Presentation, Ui},
    layout::{Expr, Value, ValueType},
    oib::{
        designer::{ClassId, ClassKind, InspectorGroup, PropertyEditor, PropertyId},
        graph::{ActionGrant, ActionOperation, Document},
    },
};

fn ui() -> Ui {
    assert_eq!(app::manifest().id, "oib-designer");
    canvas::decode_ui(&canvas::encode_ui(&app::ui(&[])).unwrap()).unwrap()
}

fn graph(ui: &mut Ui) -> &mut Document {
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("fixture has no OIB graph")
    };
    seed.graph.as_mut().unwrap()
}

fn digest(ui: &Ui) -> Vec<u8> {
    oib::contract::admission(ui).unwrap()
}

#[test]
fn visual_defaults_graph_body_and_compiler_handles_do_not_change_admission() {
    let original = ui();
    let expected = digest(&original);
    let mut changed = original.clone();
    changed.title = "New app label".into();
    changed.layout.clear();
    changed.selectors.clear();
    changed.handlers.clear();
    let source = graph(&mut changed);
    source.name = "New document label".into();
    source.id.0 += 1; // Logical document identity belongs in the epoch instead.
    source.preview_viewport += 1;
    source.inspector_content = [700., 900.];
    source.definitions[0].name = "Revised composition".into();
    source.definitions[0].nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .frame[2] = Expr::Literal(Value::Number(480.));
    source.presets[0].frame = Some([0., 0., 450., 120.]);
    source.instances[0].name = "New instance label".into();
    for class in &mut source.classes {
        class.name.push_str(" renamed");
        for property in &mut class.properties {
            property.name.push_str(" renamed");
            property.label = "New label".into();
            property.default = match property.value_type {
                ValueType::Number => Value::Number(99.),
                ValueType::Boolean => Value::Boolean(true),
                ValueType::Text => Value::Text("New appearance".into()),
                ValueType::Color => Value::Color([1, 2, 3, 255]),
                ValueType::Series => Value::Series(vec![1.]),
                ValueType::TimedSeries => Value::TimedSeries(vec![[0., 1.]]),
            };
            property.number_step = Some(7.);
            property.units = Some("unit".into());
            property.group = InspectorGroup::Identity;
            property.editor = PropertyEditor::ReadOnly;
        }
    }
    assert_eq!(digest(&changed), expected);
    assert_eq!(expected.len(), 32);
}

#[test]
fn semantic_set_order_is_canonical() {
    let original = ui();
    let mut reordered = original.clone();
    reordered.state_schema.fields.reverse();
    let source = graph(&mut reordered);
    source.classes.reverse();
    source.grants.reverse();
    for class in &mut source.classes {
        class.properties.reverse();
        class.owned_child_classes.reverse();
        class.visual_child_classes.reverse();
    }
    assert_eq!(digest(&original), digest(&reordered));
}

#[test]
fn schema_class_constraints_property_contracts_and_grants_change_admission() {
    let original = ui();
    let expected = digest(&original);
    let changes: &[fn(&mut Ui)] = &[
        |ui| ui.state_schema.id.push_str("-changed"),
        |ui| ui.state_schema.fields[0].writable = !ui.state_schema.fields[0].writable,
        |ui| ui.state_schema.fields[0].default = Value::Number(123.),
        |ui| graph(ui).classes[0].id = ClassId(999),
        |ui| graph(ui).classes[0].implementation = ClassKind::O2DSlider,
        |ui| graph(ui).classes[0].base = Some(ClassId(999)),
        |ui| graph(ui).classes[0].owned_child_classes.push(ClassId(999)),
        |ui| graph(ui).classes[0].visual_child_classes.push(ClassId(999)),
        |ui| {
            let property = graph(ui)
                .classes
                .iter_mut()
                .find_map(|class| class.properties.first_mut())
                .unwrap();
            property.id = PropertyId(999);
        },
        |ui| {
            let property = graph(ui)
                .classes
                .iter_mut()
                .find_map(|class| class.properties.first_mut())
                .unwrap();
            property.value_type = ValueType::Series;
        },
        |ui| {
            let property = graph(ui)
                .classes
                .iter_mut()
                .find_map(|class| class.properties.first_mut())
                .unwrap();
            property.number_range = Some([1., 999.]);
        },
        |ui| {
            let property = graph(ui)
                .classes
                .iter_mut()
                .find_map(|class| class.properties.first_mut())
                .unwrap();
            property.policy.live_literal = !property.policy.live_literal;
        },
        |ui| {
            let property = graph(ui)
                .classes
                .iter_mut()
                .find_map(|class| class.properties.first_mut())
                .unwrap();
            property.policy.app_binding = !property.policy.app_binding;
        },
        |ui| {
            graph(ui).grants.push(ActionGrant {
                field: 63,
                kind: GestureKind::Activate,
                operation: ActionOperation::Set,
            })
        },
        |ui| graph(ui).grants[0].kind = GestureKind::Pad,
        |ui| {
            let grant = &mut graph(ui).grants[0];
            grant.operation = match grant.operation {
                ActionOperation::Set => ActionOperation::Toggle,
                ActionOperation::Toggle => ActionOperation::Set,
            };
        },
    ];
    for (index, change) in changes.iter().enumerate() {
        let mut changed = original.clone();
        change(&mut changed);
        assert_ne!(digest(&changed), expected, "semantic mutation {index}");
    }
}

#[test]
fn source_and_materialized_ui_have_identical_semantic_admission() {
    let source = app::ui(&[]);
    assert_eq!(digest(&source), digest(&ui()));
    let mut missing = source;
    missing.presentation = None;
    assert!(oib::contract::admission(&missing).is_err());
}
