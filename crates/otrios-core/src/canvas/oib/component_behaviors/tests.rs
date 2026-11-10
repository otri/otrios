// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::oib::{
    designer::{DefinitionId, NodeId},
    graph::{ImportBinding, LockedExport},
    library::{BehaviorOutlet, BehaviorProperty, ComponentCode, Export, ExportId, LocalProperty},
};
use sha2::{Digest, Sha256};
#[allow(dead_code)]
#[path = "../../../../tests/support/oib_library_fixture.rs"]
mod fixture;

fn wat_data(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn static_guest(commands: Vec<Command>) -> Vec<u8> {
    let output = abi::encode_output(&abi::Output { commands }).unwrap();
    wat::parse_str(format!(
        r#"(module
        (memory (export "memory") 1 1)
        (data (i32.const 8192) "{}")
        (func (export "abi_version") (result i32) i32.const 1)
        (func (export "input_buffer") (result i32) i32.const 0)
        (func (export "dispatch") (param i32 i32) (result i64) i64.const {})
    )"#,
        wat_data(&output),
        ((output.len() as u64) << 32) | 8192
    ))
    .unwrap()
}
fn counting_guest(increment: u16) -> Vec<u8> {
    let output = abi::encode_output(&abi::Output {
        commands: vec![
            Command::SetLocal {
                property: 1,
                value: Value::Number(0.),
            },
            Command::Emit {
                outlet: 1,
                value: Value::Number(0.),
            },
        ],
    })
    .unwrap();
    assert_eq!(output.len(), 64);
    wat::parse_str(format!(r#"(module
        (memory (export "memory") 1 1)
        (data (i32.const 8192) "{}")
        (func (export "abi_version") (result i32) i32.const 1)
        (func (export "input_buffer") (result i32) i32.const 0)
        (func (export "dispatch") (param $input i32) (param i32) (result i64)
            (f32.store (i32.const 8228)
                (f32.add (f32.load (i32.add (local.get $input)
                    (i32.add (i32.const 76) (i32.mul (i32.load8_u offset=18 (local.get $input)) (i32.const 8))))) (f32.const {})))
            (f32.store (i32.const 8252) (f32.convert_i32_u (i32.load16_u offset=10 (local.get $input))))
            i64.const {}))"#,wat_data(&output),increment,(64u64<<32)|8192)).unwrap()
}
fn with_code(mut ui: Ui, bytes: Vec<u8>, revision: u8) -> Ui {
    let seed = fixture::seed_mut(&mut ui);
    let library = &mut seed.resolved_libraries[0];
    let definition = library
        .graph
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(12))
        .unwrap();
    let node = definition
        .nodes
        .iter()
        .find(|n| n.visual.as_ref().is_some_and(|v| v.action.is_some()))
        .unwrap();
    let source = node
        .visual
        .as_ref()
        .unwrap()
        .action
        .as_ref()
        .unwrap()
        .source
        .clone();
    library.graph.format = otrios_interfaces::oib::library::BEHAVIOR_FORMAT.into();
    library.graph.behaviors = vec![BehaviorDeclaration {
        id: 11,
        definition: DefinitionId(12),
        node: node.id,
        properties: vec![BehaviorProperty {
            id: 1,
            source: source.clone(),
        }],
        local: vec![LocalProperty {
            id: 1,
            default: Value::Number(0.),
        }],
        outlets: vec![BehaviorOutlet {
            id: 1,
            source,
            value_type: ValueType::Number,
        }],
        can_capture: true,
    }];
    library.component = Some(ComponentCode {
        abi: abi::ABI,
        sha256: Sha256::digest(&bytes).into(),
        bytes,
    });
    library.graph = crate::library::seal_graph(library.graph.clone()).unwrap();
    library.revision = [revision; 32];
    let lock = &mut seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0];
    lock.revision = library.revision;
    lock.exports = library
        .graph
        .exports
        .iter()
        .map(|e| LockedExport {
            export: e.id,
            contract: e.contract,
        })
        .collect();
    ui
}
fn mount(source: &Ui) -> (Ui, Layout, StateAdapter) {
    let ui = fixture::admit(source);
    let doc = StateAdapter::for_ui(&ui, "l2:occurrence-helper", &[], [91; 16]).unwrap();
    let (ui, layout) = doc.oib_compiled(None).unwrap();
    (ui, layout, doc)
}
fn event(a: u16) -> Event {
    Event {
        phase: Phase::Move,
        pointer: 47,
        a,
        b: 0,
    }
}
fn values(doc: &StateAdapter) -> Vec<Value> {
    (1..=4).map(|id| doc.typed_value(id).unwrap()).collect()
}
#[test]
fn imported_slider_guest_routes_only_its_admitted_field_and_preserves_occurrence_locals() {
    let source = with_code(fixture::imported(), counting_guest(1), 2);
    let (ui, layout, mut doc) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!(runtime.live_instances(), 2);
    assert_eq!(instances.code_hashes().len(), 1);
    let controls: Vec<_> = instances.controls.keys().copied().collect();
    let field = instances.controls[&controls[0]].spec.handler.fields[0];
    let before = values(&doc);
    assert!(
        instances
            .dispatch(&ui, &mut doc, controls[0], event(873), true)
            .unwrap()
            .changed
    );
    assert_eq!(doc.typed_value(field).unwrap(), Value::Number(873.));
    assert_eq!(
        instances.controls[&controls[0]].local[&1],
        Value::Number(1.)
    );
    assert_eq!(
        instances.controls[&controls[1]].local[&1],
        Value::Number(0.)
    );
    instances
        .dispatch(&ui, &mut doc, controls[0], event(874), true)
        .unwrap();
    instances
        .dispatch(&ui, &mut doc, controls[1], event(620), true)
        .unwrap();
    assert_eq!(
        instances.controls[&controls[0]].local[&1],
        Value::Number(2.)
    );
    assert_eq!(
        instances.controls[&controls[1]].local[&1],
        Value::Number(1.)
    );
    assert_eq!(values(&doc)[2..], before[2..]);
    assert!(instances.matches(&ui, &layout).unwrap());
    // Business-value changes do not need another guest lease or attachment set.
    let (next_ui, next_layout) = doc.oib_compiled(None).unwrap();
    assert!(instances.matches(&next_ui, &next_layout).unwrap());
    assert_eq!(runtime.live_instances(), 2);
}
#[test]
fn entire_mixed_result_rejects_unauthorized_outlet_without_app_or_local_effects() {
    let source = with_code(
        fixture::imported(),
        static_guest(vec![
            Command::SetLocal {
                property: 1,
                value: Value::Number(7.),
            },
            Command::Emit {
                outlet: 1,
                value: Value::Number(700.),
            },
            Command::Emit {
                outlet: 999,
                value: Value::Number(800.),
            },
        ]),
        2,
    );
    let (ui, layout, mut doc) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    let control = *instances.controls.keys().next().unwrap();
    let before = values(&doc);
    assert!(
        instances
            .dispatch(&ui, &mut doc, control, event(900), true)
            .is_err()
    );
    assert_eq!(values(&doc), before);
    assert_eq!(instances.controls[&control].local[&1], Value::Number(0.));
}
#[test]
fn local_types_ranges_and_capture_authority_are_validated_before_any_app_write() {
    for invalid in [
        Command::SetLocal {
            property: 999,
            value: Value::Number(1.),
        },
        Command::SetLocal {
            property: 1,
            value: Value::Boolean(true),
        },
        Command::Capture,
    ] {
        let source = with_code(
            fixture::imported(),
            static_guest(vec![
                Command::Emit {
                    outlet: 1,
                    value: Value::Number(700.),
                },
                invalid.clone(),
            ]),
            2,
        );
        let (ui, layout, mut doc) = mount(&source);
        let runtime = Arc::new(Runtime::default());
        let mut instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
        let control = *instances.controls.keys().next().unwrap();
        let before = values(&doc);
        assert!(
            instances
                .dispatch(&ui, &mut doc, control, event(900), false)
                .is_err()
        );
        assert_eq!(values(&doc), before);
    }
    let source = with_code(
        fixture::imported(),
        static_guest(vec![
            Command::SetLocal {
                property: 1,
                value: Value::Number(1.),
            },
            Command::Emit {
                outlet: 1,
                value: Value::Number(1001.),
            },
        ]),
        2,
    );
    let (ui, layout, mut doc) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    let control = *instances.controls.keys().next().unwrap();
    let before = values(&doc);
    assert!(
        instances
            .dispatch(&ui, &mut doc, control, event(900), true)
            .is_err()
    );
    assert_eq!(values(&doc), before);
    assert_eq!(instances.controls[&control].local[&1], Value::Number(0.));
}
#[test]
fn cancelled_input_can_cleanup_local_capture_but_cannot_emit_app_values() {
    for emit in [false, true] {
        let mut commands = vec![
            Command::SetLocal {
                property: 1,
                value: Value::Number(5.),
            },
            Command::ReleaseCapture,
        ];
        if emit {
            commands.push(Command::Emit {
                outlet: 1,
                value: Value::Number(850.),
            });
        }
        let source = with_code(fixture::imported(), static_guest(commands), 2);
        let (ui, layout, mut doc) = mount(&source);
        let runtime = Arc::new(Runtime::default());
        let mut instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
        let control = *instances.controls.keys().next().unwrap();
        let before = values(&doc);
        let result = instances.dispatch(
            &ui,
            &mut doc,
            control,
            Event {
                phase: Phase::Cancel,
                ..event(900)
            },
            true,
        );
        if emit {
            assert!(result.is_err());
            assert_eq!(instances.controls[&control].local[&1], Value::Number(0.));
        } else {
            let result = result.unwrap();
            assert!(result.changed);
            assert_eq!(result.capture, Some(false));
            assert_eq!(instances.controls[&control].local[&1], Value::Number(5.));
        }
        assert_eq!(values(&doc), before);
    }
}
#[test]
fn changed_guest_revision_stages_fresh_leases_and_resumes_compatible_host_locals() {
    let source = with_code(fixture::imported(), counting_guest(1), 2);
    let (ui, layout, mut doc) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut active = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    let control = *active.controls.keys().next().unwrap();
    active
        .dispatch(&ui, &mut doc, control, event(800), true)
        .unwrap();
    active
        .dispatch(&ui, &mut doc, control, event(801), true)
        .unwrap();
    let next = with_code(source, counting_guest(2), 3);
    let (next_ui, next_layout, _) = mount(&next);
    assert!(!active.matches(&next_ui, &next_layout).unwrap());
    let mut staged = Instances::prepare(&next_ui, &next_layout, &runtime, Some(&active)).unwrap();
    assert_eq!(runtime.live_instances(), 4);
    assert_eq!(staged.controls[&control].local[&1], Value::Number(2.));
    assert_ne!(active.code_hashes(), staged.code_hashes());
    staged
        .dispatch(&next_ui, &mut doc, control, event(802), true)
        .unwrap();
    assert_eq!(staged.controls[&control].local[&1], Value::Number(4.));
    assert_eq!(active.controls[&control].local[&1], Value::Number(2.));
    drop(active);
    assert_eq!(runtime.live_instances(), 2);
}
#[test]
fn shared_active_and_staged_budget_rejects_candidate_before_allocating_more_guests() {
    let source = with_code(fixture::imported(), counting_guest(1), 2);
    let (ui, layout, _) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut held = vec![];
    for _ in 0..4 {
        held.push(Instances::prepare(&ui, &layout, &runtime, None).unwrap());
    }
    assert_eq!(runtime.live_instances(), 8);
    assert!(Instances::prepare(&ui, &layout, &runtime, None).is_err());
    assert_eq!(runtime.live_instances(), 8);
    drop(held);
    assert_eq!(runtime.live_instances(), 0);
}
#[test]
fn shared_export_closure_does_not_duplicate_a_guest_attachment() {
    let mut source = with_code(fixture::imported(), counting_guest(1), 2);
    let seed = fixture::seed_mut(&mut source);
    let library = &mut seed.resolved_libraries[0];
    library.graph.exports.push(Export {
        id: ExportId(3),
        name: "Second Slider export".into(),
        definition: DefinitionId(12),
        contract: [0; 32],
    });
    library.graph = crate::library::seal_graph(library.graph.clone()).unwrap();
    let imports = seed.graph.as_mut().unwrap().imports.as_mut().unwrap();
    imports.locks[0].exports = library
        .graph
        .exports
        .iter()
        .map(|e| LockedExport {
            export: e.id,
            contract: e.contract,
        })
        .collect();
    let mut binding: ImportBinding = imports.bindings[0].clone();
    binding.id = 72;
    binding.export = ExportId(3);
    imports.bindings.push(binding);
    let (ui, layout, _) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!(runtime.live_instances(), 2);
}
#[test]
fn changed_handler_or_projection_is_not_silently_reused() {
    let source = with_code(fixture::imported(), counting_guest(1), 2);
    let (ui, layout, mut doc) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    let control = *instances.controls.keys().next().unwrap();
    let mut changed = ui.clone();
    changed
        .handlers
        .iter_mut()
        .find(|h| h.control == control)
        .unwrap()
        .fields = vec![4];
    assert!(instances.matches(&changed, &layout).is_err());
    let before = values(&doc);
    assert!(
        instances
            .dispatch(&changed, &mut doc, control, event(900), true)
            .is_err()
    );
    assert_eq!(values(&doc), before);
    let mut changed = ui;
    let selector = instances.controls[&control].spec.properties[0]
        .selector
        .as_ref()
        .unwrap();
    changed
        .selectors
        .iter_mut()
        .find(|s| s.local == selector.local)
        .unwrap()
        .projection = Projection::Percent;
    assert!(instances.matches(&changed, &layout).is_err());
}

#[test]
fn unbound_slider_source_is_display_only_and_needs_no_guest_until_reconnected() {
    let original = with_code(fixture::imported(), counting_guest(1), 2);
    let mut literal = original.clone();
    let source = fixture::seed_mut(&mut literal).graph.as_mut().unwrap();
    // The Value descriptor is read-only/app-bound, so removing its explicit
    // binding reveals the admitted literal default rather than assigning an
    // unauthorized new literal through that descriptor.
    source
        .presets
        .iter_mut()
        .find(|p| p.id == 20)
        .unwrap()
        .assignments
        .retain(|a| {
            !matches!(
                a.source,
                otrios_interfaces::oib::graph::Source::AppBinding { field: 1, .. }
            )
        });
    let (ui, layout, doc) = mount(&literal);
    let runtime = Arc::new(Runtime::default());
    validate(&ui, &layout).unwrap();
    assert_eq!(runtime.live_instances(), 0);
    let instances = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert!(
        instances
            .controls
            .values()
            .all(|i| i.spec.address.instance.0 != 20)
    );
    assert_eq!(doc.value(1).unwrap(), 400);
    let metadata = fixture::seed(&ui).designer.as_ref().unwrap();
    let mapping = metadata
        .mappings
        .objects
        .iter()
        .find(|m| {
            m.address.instance.0 == 20
                && m.address.references == vec![NodeId(7)]
                && m.address.node == NodeId(1)
        })
        .unwrap();
    let node = layout_node(
        &layout.blueprint().templates[0].root,
        mapping.layout_node.unwrap(),
    )
    .unwrap();
    assert!(node.responder.is_none());
    assert!(node.semantics.is_none());
    let (connected, connected_layout, _) = mount(&original);
    assert!(!instances.matches(&connected, &connected_layout).unwrap());
    let connected =
        Instances::prepare(&connected, &connected_layout, &runtime, Some(&instances)).unwrap();
    assert_eq!(connected.len(), 2);
    assert_eq!(runtime.live_instances(), 3);
}

#[test]
fn pure_validation_rejects_unbound_outlet_before_authority_configuration_changes_state() {
    let mut source = with_code(fixture::imported(), counting_guest(1), 2);
    let seed = fixture::seed_mut(&mut source);
    let library = &mut seed.resolved_libraries[0];
    library.graph.behaviors[0].outlets[0].source.property =
        otrios_interfaces::oib::designer::PropertyId(2);
    library.graph = crate::library::seal_graph(library.graph.clone()).unwrap();
    crate::library::validate_graph(&library.graph).unwrap();
    seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0].exports = library
        .graph
        .exports
        .iter()
        .map(|e| LockedExport {
            export: e.id,
            contract: e.contract,
        })
        .collect();
    // The signed library's typed path exists, but Width has no app binding and
    // is not the admitted handler's app effect. Pure UI admission must reject it.
    let mut authority =
        crate::canvas::Document::new(source.state_schema.clone(), "l2:static-admission").unwrap();
    let before = authority.save().unwrap();
    let error = authority.configure_oib(&source).unwrap_err();
    assert!(error.contains("outlet"), "{error}");
    assert_eq!(authority.save().unwrap(), before);
}

#[test]
fn responderless_bound_endpoint_remains_an_invalid_candidate() {
    let source = with_code(fixture::imported(), counting_guest(1), 2);
    let (ui, layout, _) = mount(&source);
    let spec = specs(&ui, &layout).unwrap().remove(0);
    let metadata = fixture::seed(&ui).designer.as_ref().unwrap();
    let id = metadata
        .mappings
        .objects
        .iter()
        .find(|m| m.address == spec.address)
        .unwrap()
        .layout_node
        .unwrap();
    fn clear(node: &mut TemplateNode, id: u16) -> bool {
        if node.id == id {
            node.responder = None;
            true
        } else {
            node.children.iter_mut().any(|n| clear(n, id))
        }
    }
    let mut blueprint = layout.blueprint().clone();
    assert!(clear(&mut blueprint.templates[0].root, id));
    let invalid = Layout::from_blueprint(blueprint).unwrap();
    assert!(validate(&ui, &invalid).is_err());
}

#[test]
fn static_occurrence_limit_rejects_ninth_attachment_without_an_execution_runtime() {
    let source = with_code(fixture::imported(), counting_guest(1), 2);
    let (mut ui, layout, _) = mount(&source);
    let first = specs(&ui, &layout).unwrap().remove(0);
    let mut blueprint = layout.blueprint().clone();
    let metadata = fixture::seed(&ui).designer.as_ref().unwrap();
    let mapping = metadata
        .mappings
        .objects
        .iter()
        .find(|m| m.address == first.address)
        .unwrap()
        .clone();
    let old_root = metadata
        .instances
        .iter()
        .find(|i| i.id == first.address.instance)
        .unwrap()
        .clone();
    let old_object = fixture::seed(&ui)
        .instances
        .iter()
        .find(|i| i.id == first.address.instance.0)
        .unwrap()
        .clone();
    let old_node = layout_node(&blueprint.templates[0].root, mapping.layout_node.unwrap())
        .unwrap()
        .clone();
    let mut used_nodes = BTreeSet::new();
    fn ids(node: &TemplateNode, result: &mut BTreeSet<u16>) {
        result.insert(node.id);
        for child in &node.children {
            ids(child, result)
        }
    }
    ids(&blueprint.templates[0].root, &mut used_nodes);
    let mut used_controls: BTreeSet<_> = ui.handlers.iter().map(|h| h.control).collect();
    for n in 0..7 {
        let instance = otrios_interfaces::oib::designer::InstanceId(200 + n);
        let node_id = (1..1000).find(|id| !used_nodes.contains(id)).unwrap();
        used_nodes.insert(node_id);
        let control = (1..=128)
            .find(|control| !used_controls.contains(control))
            .unwrap();
        used_controls.insert(control);
        let mut root = old_root.clone();
        root.id = instance;
        let mut object = old_object.clone();
        object.id = instance.0;
        let mut mapping = mapping.clone();
        mapping.address.instance = instance;
        mapping.layout_node = Some(node_id);
        let mut node = old_node.clone();
        node.id = node_id;
        node.children.clear();
        node.responder = Some(Responder::AppEvent {
            control,
            drag: true,
        });
        blueprint.templates[0].root.children.push(node);
        let mut handler = first.handler.clone();
        handler.control = control;
        handler.event = u16::from(control);
        ui.handlers.push(handler);
        let seed = fixture::seed_mut(&mut ui);
        seed.instances.push(object);
        let metadata = seed.designer.as_mut().unwrap();
        metadata.instances.push(root);
        metadata.mappings.objects.push(mapping);
    }
    let layout = Layout::from_blueprint(blueprint).unwrap();
    let error = validate(&ui, &layout).unwrap_err();
    assert!(error.contains("static instance budget"), "{error}");
}

fn update_context(mut source: Ui, revision: u8, edit: impl FnOnce(&mut BehaviorDeclaration)) -> Ui {
    let seed = fixture::seed_mut(&mut source);
    let library = &mut seed.resolved_libraries[0];
    edit(&mut library.graph.behaviors[0]);
    library.graph = crate::library::seal_graph(library.graph.clone()).unwrap();
    library.revision = [revision; 32];
    let lock = &mut seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0];
    lock.revision = library.revision;
    lock.exports = library
        .graph
        .exports
        .iter()
        .map(|e| LockedExport {
            export: e.id,
            contract: e.contract,
        })
        .collect();
    source
}
fn materialized_layout(source: &Ui) -> (Ui, Layout) {
    let ui = crate::canvas::oib::graph::materialize(source).unwrap();
    let layout = Layout::from_bytes(&ui.layout).unwrap();
    (ui, layout)
}
fn text_locals(first: usize, second: usize) -> Vec<LocalProperty> {
    vec![
        LocalProperty {
            id: 1,
            default: Value::Text("x".repeat(first)),
        },
        LocalProperty {
            id: 2,
            default: Value::Text("y".repeat(second)),
        },
    ]
}
#[test]
fn collective_local_defaults_reject_before_guest_reservation_or_publication() {
    let source = update_context(
        with_code(fixture::imported(), static_guest(vec![]), 2),
        2,
        |declaration| declaration.local = text_locals(4096, 4096),
    );
    let (ui, layout) = materialized_layout(&source);
    let runtime = Arc::new(Runtime::default());
    assert!(validate(&ui, &layout).is_err());
    assert!(Instances::prepare(&ui, &layout, &runtime, None).is_err());
    assert_eq!(runtime.live_instances(), 0);
    let mut authority =
        crate::canvas::Document::new(source.state_schema.clone(), "l2:aggregate-locals").unwrap();
    let before = authority.save().unwrap();
    assert!(authority.configure_oib(&source).is_err());
    assert_eq!(authority.save().unwrap(), before);
}
#[test]
fn property_and_local_contexts_share_one_aggregate_byte_limit() {
    let source = update_context(
        with_code(fixture::imported(), static_guest(vec![]), 2),
        2,
        |declaration| {
            declaration.local = text_locals(4096, 3996);
            let mut second = declaration.properties[0].clone();
            second.id = 2;
            declaration.properties.push(second);
        },
    );
    let (ui, layout) = materialized_layout(&source);
    let specs = specs(&ui, &layout).unwrap();
    // Each half fits separately; only the combined canonical input is rejected.
    for spec in &specs {
        let locals = spec
            .declaration
            .local
            .iter()
            .map(|p| Field {
                id: p.id,
                value: p.default.clone(),
            })
            .collect();
        abi::encode_input(&context(spec, validation_event(), vec![], locals)).unwrap();
        let properties = spec
            .properties
            .iter()
            .map(|read| Field {
                id: read.id,
                value: Value::Number(400.),
            })
            .collect();
        abi::encode_input(&context(spec, validation_event(), properties, vec![])).unwrap();
    }
    assert!(validate(&ui, &layout).is_err());
    let runtime = Arc::new(Runtime::default());
    assert!(Instances::prepare(&ui, &layout, &runtime, None).is_err());
    assert_eq!(runtime.live_instances(), 0);
}
#[test]
fn prepublication_context_check_uses_actual_causal_literals_and_app_values() {
    let source = update_context(
        with_code(fixture::imported(), static_guest(vec![]), 2),
        2,
        |declaration| {
            declaration.local = text_locals(4096, 3976);
            let mut label = declaration.properties[0].clone();
            label.id = 2;
            label.source.property = otrios_interfaces::oib::designer::PropertyId(3);
            declaration.properties.push(label);
        },
    );
    let (ui, layout) = materialized_layout(&source);
    validate(&ui, &layout).unwrap();
    let spec = specs(&ui, &layout).unwrap().remove(0);
    let literal = &spec.properties[1];
    let seed = fixture::seed(&ui);
    let cell =
        document::graph_property(seed, spec.address.instance, &literal.path, &BTreeMap::new())
            .unwrap()
            .0
            .unwrap();
    let values = BTreeMap::from([(cell, Some(Value::Text("z".repeat(96))))]);
    assert!(validate_context(&ui, &layout, &values, |id| default_field(&ui, id)).is_err());
    // The field reader is also authoritative for the current typed projection.
    assert!(
        validate_context(&ui, &layout, &BTreeMap::new(), |_| Ok(Value::Number(1001.))).is_err()
    );
}
#[test]
fn staged_context_checks_retained_locals_with_new_properties_before_activation() {
    let commands = vec![
        Command::SetLocal {
            property: 1,
            value: Value::Text("x".repeat(4096)),
        },
        Command::SetLocal {
            property: 2,
            value: Value::Text("y".repeat(3976)),
        },
    ];
    let source = update_context(
        with_code(fixture::imported(), static_guest(commands), 2),
        2,
        |declaration| declaration.local = text_locals(0, 0),
    );
    let (ui, layout, mut doc) = mount(&source);
    let runtime = Arc::new(Runtime::default());
    let mut active = Instances::prepare(&ui, &layout, &runtime, None).unwrap();
    let control = *active.controls.keys().next().unwrap();
    active
        .dispatch(&ui, &mut doc, control, event(700), true)
        .unwrap();
    active.validate_current(&ui, &doc).unwrap();
    let before = values(&doc);
    let next = update_context(source, 3, |declaration| {
        let mut label = declaration.properties[0].clone();
        label.id = 2;
        label.source.property = otrios_interfaces::oib::designer::PropertyId(3);
        declaration.properties.push(label);
        let mut number = declaration.properties[0].clone();
        number.id = 3;
        declaration.properties.push(number);
    });
    let (next_ui, next_layout, _) = mount(&next);
    let staged = Instances::prepare(&next_ui, &next_layout, &runtime, Some(&active)).unwrap();
    assert_eq!(runtime.live_instances(), 4);
    assert!(staged.validate_current(&next_ui, &doc).is_err());
    drop(staged);
    assert_eq!(runtime.live_instances(), 2);
    active.validate_current(&ui, &doc).unwrap();
    assert_eq!(values(&doc), before);
    assert_eq!(
        active.controls[&control].local[&1],
        Value::Text("x".repeat(4096))
    );
}
