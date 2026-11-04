// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::{canvas, dashboard};
#[allow(dead_code)]
#[path = "../src/canvas/client/oib_property_editor.rs"]
mod editor;
use canvas::oib::document::PropertyInfo;
use editor::*;
use otrios_interfaces::keyboard::Edit;
use otrios_interfaces::layout::{Value, ValueType};
use otrios_interfaces::oib::designer::*;

fn info(id: u64, value: Value) -> PropertyInfo {
    let (value_type, editor) = match &value {
        Value::Text(_) => (ValueType::Text, PropertyEditor::TextSuggestions),
        Value::Number(_) => (ValueType::Number, PropertyEditor::NumberSuggestions),
        Value::Color(_) => (ValueType::Color, PropertyEditor::Palette),
        Value::Boolean(_) => (ValueType::Boolean, PropertyEditor::Toggle),
        _ => unreachable!(),
    };
    PropertyInfo {
        address: Some(PropertyAddress::Occurrence {
            instance: InstanceId(id),
            path: PropertyPath {
                references: vec![NodeId(7)],
                node: NodeId(1),
                property: PropertyId(1),
            },
        }),
        cell: Some(id),
        descriptor: PropertyDescriptor {
            id: PropertyId(1),
            name: "same-name".into(),
            label: "Value".into(),
            value_type,
            default: value.clone(),
            number_range: None,
            number_step: Some(10.),
            units: None,
            group: InspectorGroup::Component,
            editor,
            policy: PropertyPolicy {
                live_literal: true,
                app_binding: false,
            },
        },
        value: Some(value),
        inherited: None,
        origin: "authored".into(),
        source: "literal".into(),
        overridden: false,
    }
}
fn address(info: &PropertyInfo) -> &PropertyAddress {
    info.address.as_ref().unwrap()
}
fn field(engine: &PropertyDrafts, info: &PropertyInfo, part: Part) -> u32 {
    engine.draft(address(info)).unwrap().field(part).unwrap().id
}
fn replace(engine: &mut PropertyDrafts, id: u32, text: &str) -> Result<EditResult, Error> {
    engine.edit(
        id,
        &Edit::Replace {
            text: text.into(),
            cursor: text.encode_utf16().count() as u32,
            anchor: text.encode_utf16().count() as u32,
            selection_only: false,
        },
    )
}

#[test]
fn nonpreset_number_and_equal_value_explicit_commit() {
    let mut e = PropertyDrafts::new(100, 1000);
    let mut p = info(20, Value::Number(400.));
    p.descriptor.number_range = Some([10., 900.]);
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    e.focus(id).unwrap();
    assert_eq!(replace(&mut e, id, "377.5"), Ok(EditResult::Local(true)));
    assert_eq!(
        e.draft(address(&p)).unwrap().preview(),
        Ok(Value::Number(377.5))
    );
    assert!(e.draft(address(&p)).unwrap().dirty);
    assert_eq!(e.edit(id, &Edit::Enter), Ok(EditResult::CommitRequested));
    let c = e.prepare(address(&p), &p, 2).unwrap();
    assert_eq!(c.value, Value::Number(377.5));
    assert_eq!(c.base_revision, 1);
    e.acknowledge(&c, false).unwrap();
    assert!(e.draft(address(&p)).unwrap().dirty);
    let c = e.prepare(address(&p), &p, 2).unwrap();
    e.acknowledge(&c, true).unwrap();
    assert!(!e.draft(address(&p)).unwrap().dirty);
    p.value = Some(Value::Number(377.5));
    let equal = e.prepare(address(&p), &p, 3).unwrap();
    assert_eq!(equal.value, Value::Number(377.5));
}

#[test]
fn utf8_committed_and_raw_byte_limits() {
    let mut e = PropertyDrafts::new(1, 100);
    let p = info(1, Value::Text("Volume".into()));
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    for (text, valid) in [
        ("é".repeat(48), true),
        ("é".repeat(49), false),
        (String::new(), false),
        ("a".repeat(256), false),
    ] {
        replace(&mut e, id, &text).unwrap();
        assert_eq!(e.prepare(address(&p), &p, 2).is_ok(), valid);
    }
    assert_eq!(replace(&mut e, id, &"é".repeat(129)), Err(Error::RawLimit));
    assert_eq!(
        e.draft(address(&p))
            .unwrap()
            .field(Part::Value)
            .unwrap()
            .buffer()
            .text()
            .len(),
        256
    );
    replace(&mut e, id, "Sound").unwrap();
    assert_eq!(
        e.prepare(address(&p), &p, 2).unwrap().value,
        Value::Text("Sound".into())
    );
    assert_eq!(
        replace(&mut e, id, "bad\0text"),
        Ok(EditResult::Local(false))
    );
}

#[test]
fn invalid_numbers_and_descriptor_range() {
    let mut e = PropertyDrafts::new(1, 100);
    let mut p = info(1, Value::Number(100.));
    p.descriptor.number_range = Some([10., 500.]);
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    for raw in [
        "NaN", "inf", "-0", "-0.0", "-1", "1001", "9.99", "500.1", "", "1e999",
    ] {
        replace(&mut e, id, raw).unwrap();
        assert_eq!(
            e.prepare(address(&p), &p, 2),
            Err(Error::InvalidNumber),
            "{raw}"
        );
        assert!(e.draft(address(&p)).unwrap().dirty);
    }
    for raw in ["10", "377.5", "500"] {
        replace(&mut e, id, raw).unwrap();
        assert!(e.prepare(address(&p), &p, 2).is_ok());
    }
}

#[test]
fn capacity_no_eviction_and_retired_ids() {
    let mut e = PropertyDrafts::new(10, 1000);
    let first = info(1, Value::Text("one".into()));
    for id in 1..=16 {
        e.open(&info(id, Value::Text("one".into())), 1).unwrap();
    }
    let old = field(&e, &first, Part::Value);
    replace(&mut e, old, "").unwrap();
    assert_eq!(
        e.open(&info(17, Value::Text("next".into())), 1).err(),
        Some(Error::Capacity)
    );
    assert_eq!(e.drafts().len(), 16);
    assert_eq!(field(&e, &first, Part::Value), old);
    assert!(e.cancel(address(&first)));
    e.open(&first, 2).unwrap();
    assert!(field(&e, &first, Part::Value) > old);
    assert_eq!(
        e.edit(old, &Edit::Commit("stale".into())),
        Err(Error::UnknownField)
    );
    assert!(e.draft(address(&first)).unwrap().error.is_none());
}

#[test]
fn full_address_isolation_focus_and_dirty_remote_refresh() {
    let mut e = PropertyDrafts::new(1, 100);
    let p = info(20, Value::Text("Volume".into()));
    let mut q = p.clone();
    q.address = Some(PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(8)],
            node: NodeId(1),
            property: PropertyId(1),
        },
    });
    e.open(&p, 1).unwrap();
    e.open(&q, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    e.focus(id).unwrap();
    replace(&mut e, id, "Sound").unwrap();
    e.focus(field(&e, &q, Part::Value)).unwrap();
    let mut remote = p.clone();
    remote.value = Some(Value::Text("Remote".into()));
    e.refresh(address(&p), Some(&remote), 9).unwrap();
    let d = e.draft(address(&p)).unwrap();
    assert_eq!(d.field(Part::Value).unwrap().buffer().text(), "Sound");
    assert_eq!((d.base_revision, d.source_revision), (1, 9));
    assert_eq!(e.focused().unwrap().address, *address(&q));
    assert_eq!(
        e.draft(address(&q)).unwrap().preview(),
        Ok(Value::Text("Volume".into()))
    );
    assert_eq!(e.prepare(address(&p), &q, 9), Err(Error::Unavailable));
}

#[test]
fn invalid_blur_cancel_and_disappearance() {
    let mut e = PropertyDrafts::new(1, 100);
    let p = info(1, Value::Number(100.));
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    e.focus(id).unwrap();
    replace(&mut e, id, "NaN").unwrap();
    assert_eq!(e.prepare(address(&p), &p, 2), Err(Error::InvalidNumber));
    e.clear_focus();
    assert_eq!(
        e.draft(address(&p)).unwrap().error,
        Some(Error::InvalidNumber)
    );
    assert_eq!(e.refresh(address(&p), None, 3), Err(Error::Unavailable));
    assert_eq!(e.edit(id, &Edit::Backspace), Err(Error::Unavailable));
    assert!(e.cancel(address(&p)));
    assert!(e.draft(address(&p)).is_none());
}

#[test]
fn rgba_hex_alpha_picker_and_release_candidate() {
    let mut e = PropertyDrafts::new(1, 100);
    let p = info(1, Value::Color([1, 2, 3, 255]));
    e.open(&p, 1).unwrap();
    let hex = field(&e, &p, Part::Hex);
    let opacity = field(&e, &p, Part::Opacity);
    replace(&mut e, hex, "#12aBeF").unwrap();
    replace(&mut e, opacity, "50").unwrap();
    assert_eq!(
        e.prepare(address(&p), &p, 2).unwrap().value,
        Value::Color([18, 171, 239, 128])
    );
    replace(&mut e, hex, "#11223340").unwrap();
    assert_eq!(
        e.prepare(address(&p), &p, 2).unwrap().value,
        Value::Color([17, 34, 51, 64])
    );
    replace(&mut e, opacity, "25").unwrap();
    assert_eq!(
        e.draft(address(&p)).unwrap().preview(),
        Ok(Value::Color([17, 34, 51, 64]))
    );
    for raw in ["NaN", "101", "-0", "-1"] {
        replace(&mut e, opacity, raw).unwrap();
        assert_eq!(e.prepare(address(&p), &p, 2), Err(Error::InvalidOpacity));
    }
    e.set_color(address(&p), [100, 150, 200, 17]).unwrap();
    e.set_opacity(address(&p), 37.5).unwrap();
    let c = e.prepare(address(&p), &p, 2).unwrap();
    assert_eq!(c.value, Value::Color([100, 150, 200, 96]));
    e.acknowledge(&c, true).unwrap();
}

#[test]
fn authority_refusal_and_boolean() {
    let p = info(1, Value::Boolean(false));
    let mut e = PropertyDrafts::new(1, 100);
    for variant in 0..4 {
        let mut denied = p.clone();
        match variant {
            0 => denied.source = "bound".into(),
            1 => denied.descriptor.editor = PropertyEditor::ReadOnly,
            2 => denied.descriptor.policy.live_literal = false,
            _ => denied.cell = None,
        }
        assert_eq!(e.open(&denied, 1).err(), Some(Error::ReadOnly));
    }
    e.open(&p, 1).unwrap();
    e.set_boolean(address(&p), true).unwrap();
    assert_eq!(
        e.prepare(address(&p), &p, 2).unwrap().value,
        Value::Boolean(true)
    );
}

#[test]
fn edits_after_submission_cannot_be_cleared_by_old_ack() {
    let p = info(1, Value::Text("Volume".into()));
    let mut e = PropertyDrafts::new(1, 100);
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    replace(&mut e, id, "Sound").unwrap();
    let c = e.prepare(address(&p), &p, 2).unwrap();
    replace(&mut e, id, "Newest").unwrap();
    assert_eq!(e.acknowledge(&c, true), Err(Error::StaleCandidate));
    assert!(e.draft(address(&p)).unwrap().dirty);
    assert_eq!(
        e.draft(address(&p)).unwrap().preview(),
        Ok(Value::Text("Newest".into()))
    );
}

#[test]
fn id_exhaustion_and_color_roundtrip() {
    let p = info(1, Value::Text("x".into()));
    let mut e = PropertyDrafts::new(u32::MAX, u32::MAX);
    assert_eq!(e.open(&p, 1).err(), Some(Error::IdExhausted));
    let mut e = PropertyDrafts::new(1, 100);
    for alpha in 0..=255 {
        let p = info(1, Value::Color([10, 20, 30, alpha]));
        if e.draft(address(&p)).is_none() {
            e.open(&p, 1).unwrap();
        }
        e.set_color(address(&p), [10, 20, 30, alpha]).unwrap();
        assert_eq!(
            e.draft(address(&p)).unwrap().preview(),
            Ok(Value::Color([10, 20, 30, alpha]))
        );
    }
}

#[test]
fn real_host_input_geometry_scale_and_local_caret() {
    let font = dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut e = PropertyDrafts::new(90, 100);
    let p = info(1, Value::Text("é😀".into()));
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    e.focus(id).unwrap();
    e.place_cursor(id, 6).unwrap();
    let input = e.focused_input([12., 34., 240., 52.], &font).unwrap();
    assert_eq!(input.id, id);
    assert_eq!(input.bounds, [12., 34., 240., 52.]);
    assert_eq!(input.value, "é😀");
    assert_eq!(input.cursor, 3);
    assert_eq!(input.max_bytes, 256);
    assert_eq!(input.font_size, font.metrics().cache_em(2.));
    assert_eq!(input.line_height, font.metrics().line_height(2.));
    assert_eq!(
        e.edit(id, &Edit::Tab { backward: true }),
        Ok(EditResult::Traverse { backward: true })
    );
    assert!(!e.draft(address(&p)).unwrap().dirty);
    e.refresh(address(&p), None, 2).unwrap_err();
    assert!(e.focused_input([0.; 4], &font).is_none());
}

#[test]
fn fresh_constraints_reject_dirty_input_and_clean_focus_survives_remote() {
    let mut e = PropertyDrafts::new(1, 100);
    let p = info(1, Value::Number(100.));
    e.open(&p, 1).unwrap();
    let id = field(&e, &p, Part::Value);
    e.focus(id).unwrap();
    let mut remote = p.clone();
    remote.value = Some(Value::Number(200.));
    e.refresh(address(&p), Some(&remote), 2).unwrap();
    assert_eq!(
        e.draft(address(&p)).unwrap().preview(),
        Ok(Value::Number(100.))
    );
    replace(&mut e, id, "377.5").unwrap();
    remote.descriptor.number_range = Some([0., 300.]);
    assert_eq!(
        e.prepare(address(&p), &remote, 3),
        Err(Error::InvalidNumber)
    );
    assert_eq!(field(&e, &p, Part::Value), id);
    let mut bound = remote.clone();
    bound.source = "bound".into();
    assert_eq!(e.prepare(address(&p), &bound, 4), Err(Error::ReadOnly));
    assert!(e.draft(address(&p)).unwrap().dirty);
    assert_eq!(
        e.draft(address(&p))
            .unwrap()
            .field(Part::Value)
            .unwrap()
            .buffer()
            .text(),
        "377.5"
    );
}

#[test]
fn raw_events_have_no_commit_and_cannot_replay_old_candidates() {
    let mut e = PropertyDrafts::new(1, 100);
    let p = info(1, Value::Color([1, 2, 3, 255]));
    e.open(&p, 1).unwrap();
    let hex = field(&e, &p, Part::Hex);
    let old = e.prepare(address(&p), &p, 1).unwrap();
    for raw in ["#", "#12", "#FFGG00", "#😀abcd", "#00000080"] {
        assert!(matches!(
            replace(&mut e, hex, raw),
            Ok(EditResult::Local(_))
        ));
    }
    assert_eq!(e.acknowledge(&old, true), Err(Error::StaleCandidate));
    assert_eq!(
        e.draft(address(&p))
            .unwrap()
            .field(Part::Opacity)
            .unwrap()
            .buffer()
            .text()
            .parse::<f32>()
            .unwrap()
            .round(),
        50.
    );
    let release = e.prepare(address(&p), &p, 2).unwrap();
    assert_eq!(release.value, Value::Color([0, 0, 0, 128]));
    e.acknowledge(&release, false).unwrap();
    assert!(e.draft(address(&p)).unwrap().dirty);
    assert!(e.cancel(address(&p)));
    assert_eq!(e.acknowledge(&release, true), Err(Error::StaleCandidate));
}
