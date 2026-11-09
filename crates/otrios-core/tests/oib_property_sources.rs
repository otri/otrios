// SPDX-License-Identifier: LGPL-3.0-only
//! In-process real D1 fixture checks; not authority-process or live acceptance.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::oib::document::{AuthoredOrigin, SourceRole, property_sources};
use otrios_core::canvas::{self, Document, Session, StateAdapter, StateSelection, oib};
use otrios_interfaces::{
    canvas::{Presentation, StateRequest, Ui},
    layout::Value,
    oib::{Seed, designer::*},
};

fn seed(ui: &Ui) -> &Seed {
    let Some(Presentation::Oib(seed)) = &ui.presentation else {
        panic!("fixture")
    };
    seed
}
fn address(references: &[u64], property: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: references.iter().copied().map(NodeId).collect(),
            node: NodeId(1),
            property: PropertyId(property),
        },
    }
}
fn shared(definition: u64, property: u64) -> PropertyAddress {
    PropertyAddress::Definition {
        definition: DefinitionId(definition),
        node: NodeId(1),
        property: PropertyId(property),
    }
}
fn setup() -> (Ui, Document) {
    let ui = canvas::decode_ui(&canvas::encode_ui(&app::ui(&[])).unwrap()).unwrap();
    let mut doc = Document::new(ui.state_schema.clone(), "sources").unwrap();
    doc.configure_oib(&ui).unwrap();
    (ui, doc)
}
fn sync(doc: &mut Document, session: &mut Session, client: &mut StateAdapter) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = client.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            client.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("sync did not settle");
}
fn adapter(ui: &Ui, doc: &mut Document, actor: u8) -> (StateAdapter, Session) {
    let (mut session, _) = doc.open_session(&[actor; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(ui, "sources", &[], [actor; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(doc, &mut session, &mut client);
    (client, session)
}
fn issue(client: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = client.generate().unwrap() {
        if matches!(
            canvas::decode_state_request(&bytes).unwrap(),
            StateRequest::OibWrite { .. }
        ) {
            return bytes;
        }
    }
    panic!("missing write")
}
fn trace(ui: &Ui, doc: &Document, at: &PropertyAddress) -> oib::document::PropertySources {
    property_sources(
        seed(ui),
        &doc.oib_allocations().unwrap(),
        at,
        &doc.oib_values(2).unwrap(),
    )
    .unwrap()
}

#[test]
fn real_forwarding_masks_slider_and_preserves_definition_selection() {
    let (ui, doc) = setup();
    let at = address(&[7], 1);
    let result = trace(&ui, &doc, &at);
    assert_eq!(result.immediate_source, Some(address(&[], 1)));
    assert_eq!(result.terminal_shared, Some(shared(10, 1)));
    assert!(!result.read_only);
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.address == shared(10, 1) && !h.masked)
    );
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.address == shared(12, 1) && h.masked)
    );
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.address == at && matches!(h.role, SourceRole::Forward { .. }))
    );
    let definition = trace(&ui, &doc, &shared(12, 1));
    assert_eq!(definition.terminal_shared, Some(shared(12, 1)));
    assert!(
        definition
            .hops
            .iter()
            .any(|h| h.address == shared(12, 1) && !h.masked)
    );
    assert!(result.hops.len() <= 320);
}

#[test]
fn authored_equal_override_reset_and_current_inherited_source() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    let label = address(&[7], 3);
    let volume = Value::Text("Volume".into());
    let original = trace(&ui, &doc, &label);
    assert_eq!(original.value, Some(volume.clone()));
    assert!(!original.overridden);
    assert!(original.hops.iter().any(|h| h.cell.is_none()
        && h.value == Some(volume.clone())
        && matches!(
            h.role,
            SourceRole::AuthoredLiteral(AuthoredOrigin::Instance(InstanceId(20)))
        )));
    a.edit_oib_property(&label, Some(volume.clone())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let equal = trace(&ui, &doc, &label);
    assert!(equal.overridden);
    assert_eq!(equal.inherited, Some(volume.clone()));
    assert!(
        equal
            .hops
            .iter()
            .any(|h| h.masked && matches!(h.role, SourceRole::AuthoredLiteral(_)))
    );
    a.edit_oib_property(&label, None).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(trace(&ui, &doc, &label).value, Some(volume));
    let accent = address(&[7], 1);
    let local = Value::Color([1, 2, 3, 255]);
    let remote = Value::Color([4, 5, 6, 255]);
    a.edit_oib_property(&accent, Some(local.clone())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2);
    b.edit_oib_property(&shared(10, 1), Some(remote.clone()))
        .unwrap();
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    let updated = trace(&ui, &doc, &accent);
    assert_eq!(updated.value, Some(local));
    assert_eq!(updated.inherited, Some(remote.clone()));
    a.edit_oib_property(&accent, None).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(trace(&ui, &doc, &accent).value, Some(remote));
}

#[test]
fn explicit_conflict_choices_include_clear_and_guard_stale_versions() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2);
    let at = address(&[7], 3);
    let sound = Value::Text("Sound".into());
    a.edit_oib_property(&at, Some(sound.clone())).unwrap();
    let aw = issue(&mut a);
    b.edit_oib_property(&at, None).unwrap();
    let bw = issue(&mut b);
    doc.receive(&mut sa, &aw).unwrap();
    let active = doc.oib_status().unwrap().active_root;
    let values = doc.oib_values(2).unwrap();
    doc.receive(&mut sb, &bw).unwrap();
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    assert_eq!(doc.oib_values(2).unwrap(), values);
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    let view = a.oib_property_register(&at).unwrap();
    assert_eq!(
        view.cell,
        oib::document::cell_for_address(seed(&ui), &doc.oib_allocations().unwrap(), &at).unwrap()
    );
    assert!(view.alternatives.contains(&None));
    assert!(view.alternatives.contains(&Some(sound.clone())));
    assert_eq!(view.alternatives.len(), 2);
    let root = doc.root_hash();
    assert!(
        a.choose_oib_property(&view, Some(Value::Text("Unseen".into())))
            .is_err()
    );
    assert!(a.confirmed());
    assert_eq!(doc.root_hash(), root);
    a.set_read_only(true);
    assert!(!a.choose_oib_property(&view, Some(sound.clone())).unwrap());
    a.set_read_only(false);
    assert!(a.choose_oib_property(&view, Some(sound.clone())).unwrap());
    assert!(!a.choose_oib_property(&view, None).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
    assert_eq!(trace(&ui, &doc, &at).value, Some(sound.clone()));
    assert!(a.choose_oib_property(&view, None).is_err());
    assert!(a.confirmed());
    let current = a.oib_property_register(&at).unwrap();
    assert_eq!(current.alternatives, vec![Some(sound)]);
    assert_ne!(view.version, current.version);
    assert!(a.choose_oib_property(&current, None).is_err());
    // Reset is a distinct action and does not require a clear alternative.
    assert!(a.edit_oib_property(&at, None).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(
        trace(&ui, &doc, &at).value,
        Some(Value::Text("Volume".into()))
    );
}

#[test]
fn multi_hop_binding_stays_read_only_and_cycles_fail_boundedly() {
    let (ui, doc) = setup();
    let mut metadata = seed(&ui).clone();
    let bound = trace(&ui, &doc, &address(&[7], 4)).binding.unwrap();
    let designer = metadata.designer.as_mut().unwrap();
    let slider = designer
        .classes
        .iter_mut()
        .find(|c| c.properties.iter().any(|p| p.id == PropertyId(4)))
        .unwrap();
    let mut extra = slider
        .properties
        .iter()
        .find(|p| p.id == PropertyId(2))
        .unwrap()
        .clone();
    extra.id = PropertyId(6);
    slider.properties.push(extra);
    let panel = designer
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let reference = panel
        .nodes
        .iter_mut()
        .find(|n| n.id == NodeId(7))
        .unwrap()
        .reference
        .as_mut()
        .unwrap();
    let path = |property| PropertyPath {
        references: vec![],
        node: NodeId(1),
        property: PropertyId(property),
    };
    let from = |property| PropertyPath {
        references: vec![NodeId(7)],
        node: NodeId(1),
        property: PropertyId(property),
    };
    reference.assignments.push(Assignment {
        target: path(2),
        source: AssignmentSource::Forward(from(6)),
    });
    reference.assignments.push(Assignment {
        target: path(6),
        source: AssignmentSource::Forward(from(4)),
    });
    // Metadata-only extension exercises the existing resolver and admission guard;
    // it is not a new signed fixture or a claim of whole-UI admission.
    let allocations = doc.oib_allocations().unwrap();
    let values = doc.oib_values(2).unwrap();
    let result = property_sources(&metadata, &allocations, &address(&[7], 2), &values).unwrap();
    assert!(result.read_only);
    assert_eq!(result.binding, Some(bound));
    assert_eq!(result.value, None);
    assert_eq!(result.terminal_shared, None);
    assert_eq!(
        result
            .hops
            .iter()
            .filter(|h| matches!(h.role, SourceRole::Forward { .. }))
            .count(),
        2
    );
    assert!(oib::document::cell_for_address(&metadata, &allocations, &address(&[7], 2)).is_err());
    let designer = metadata.designer.as_mut().unwrap();
    designer
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter_mut()
        .find(|n| n.id == NodeId(7))
        .unwrap()
        .reference
        .as_mut()
        .unwrap()
        .assignments[1]
        .source = AssignmentSource::Forward(from(2));
    assert!(property_sources(&metadata, &allocations, &address(&[7], 2), &values).is_err());
}

#[test]
fn explicit_clear_choice_restores_authored_literal() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2);
    let at = address(&[7], 3);
    a.edit_oib_property(&at, Some(Value::Text("Sound".into())))
        .unwrap();
    let aw = issue(&mut a);
    b.edit_oib_property(&at, None).unwrap();
    let bw = issue(&mut b);
    doc.receive(&mut sa, &aw).unwrap();
    doc.receive(&mut sb, &bw).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let view = a.oib_property_register(&at).unwrap();
    assert!(a.choose_oib_property(&view, None).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    let result = trace(&ui, &doc, &at);
    assert!(!result.overridden);
    assert_eq!(result.value, Some(Value::Text("Volume".into())));
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
}

#[test]
fn equal_value_rewrite_changes_exact_version_and_tampered_views_do_not_write() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    let at = address(&[7], 3);
    let volume = Value::Text("Volume".into());
    a.edit_oib_property(&at, Some(volume.clone())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let old = a.oib_property_register(&at).unwrap();
    a.edit_oib_property(&at, Some(volume.clone())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let current = a.oib_property_register(&at).unwrap();
    assert_eq!(old.alternatives, current.alternatives);
    assert_ne!(old.version, current.version);
    let root = doc.root_hash();
    assert!(a.choose_oib_property(&old, Some(volume.clone())).is_err());
    let mut tampered = current.clone();
    tampered.cell = u64::MAX;
    assert!(
        a.choose_oib_property(&tampered, Some(volume.clone()))
            .is_err()
    );
    tampered = current;
    tampered.alternatives.push(None);
    assert!(a.choose_oib_property(&tampered, None).is_err());
    assert!(a.confirmed());
    assert_eq!(root, doc.root_hash());
    assert!(app::manifest().dependencies.iter().any(|d| d == FEATURE));
}

#[test]
fn allocated_occurrence_trace_uses_resident_identity_and_stable_cells() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    assert!(a.duplicate_oib(20, Some("Audio copy".into())).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    let allocations = doc.oib_allocations().unwrap();
    let resident = allocations.iter().find(|a| a.name == "Audio copy").unwrap();
    let at = PropertyAddress::Occurrence {
        instance: InstanceId(resident.id),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(1),
        },
    };
    let result = trace(&ui, &doc, &at);
    assert_eq!(result.address, at);
    assert_eq!(
        result.immediate_source,
        Some(PropertyAddress::Occurrence {
            instance: InstanceId(resident.id),
            path: PropertyPath {
                references: vec![],
                node: NodeId(1),
                property: PropertyId(1)
            },
        })
    );
    assert_eq!(result.terminal_shared, Some(shared(10, 1)));
    let cell = oib::document::cell_for_address(seed(&ui), &allocations, &at).unwrap();
    assert_ne!(at, address(&[7], 1));
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.address == at && h.cell == Some(cell))
    );
    assert_eq!(a.oib_property_register(&at).unwrap().cell, cell);
    assert_eq!(a.value(1).unwrap(), 400);
}
