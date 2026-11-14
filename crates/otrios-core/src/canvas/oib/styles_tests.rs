// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::oib::designer::DocumentId;

fn system_ref() -> SystemRef {
    SystemRef {
        owner: Owner::Document(DocumentId(1)),
        system: SystemId(1),
    }
}
fn style_ref() -> StyleRef {
    StyleRef {
        system: system_ref(),
        style: StyleId(1),
    }
}
fn variable_ref() -> VariableRef {
    VariableRef {
        system: system_ref(),
        variable: VariableId(1),
    }
}
fn owners() -> BTreeSet<Owner> {
    BTreeSet::from([system_ref().owner])
}
fn literal(value: u8) -> FieldValue {
    FieldValue::Literal([value, value, value, 255])
}
fn entry(field: u64, value: FieldValue) -> Entry {
    Entry {
        field: FieldId(field),
        value,
    }
}
fn fixture() -> Catalog {
    Catalog {
        format: FORMAT.into(),
        systems: vec![System {
            reference: system_ref(),
            name: "otrios.base".into(),
            variables: vec![Variable {
                id: VariableId(1),
                name: "background".into(),
                availability: Availability::Active,
                values: ColorModes {
                    light: [240, 240, 240, 255],
                    dark: [8, 8, 8, 255],
                },
            }],
            contracts: vec![Contract {
                id: ContractId(1),
                name: "button".into(),
                classes: vec![ClassId(1)],
                fields: vec![
                    Field {
                        id: FieldId(1),
                        name: "background".into(),
                        domain: Domain::Body,
                    },
                    Field {
                        id: FieldId(2),
                        name: "text".into(),
                        domain: Domain::Body,
                    },
                    Field {
                        id: FieldId(3),
                        name: "focus".into(),
                        domain: Domain::Focus,
                    },
                ],
            }],
            styles: vec![Style {
                id: StyleId(1),
                name: "button.primary".into(),
                availability: Availability::Active,
                contract: ContractId(1),
                defaults: vec![
                    entry(1, FieldValue::Variable(variable_ref())),
                    entry(2, literal(200)),
                    entry(3, literal(100)),
                ],
                patches: vec![
                    Patch {
                        state: State::Selected,
                        fields: vec![entry(1, literal(10))],
                    },
                    Patch {
                        state: State::Hover,
                        fields: vec![entry(1, literal(20))],
                    },
                    Patch {
                        state: State::Pressed,
                        fields: vec![entry(1, literal(30))],
                    },
                    Patch {
                        state: State::Disabled,
                        fields: vec![entry(1, literal(40))],
                    },
                    Patch {
                        state: State::Focus,
                        fields: vec![entry(3, literal(50))],
                    },
                ],
            }],
        }],
    }
}
fn resolve(
    admitted: &Admitted,
    mode: Mode,
    mask: StateMask,
    doc: &[Override],
    instance: &[Override],
) -> Resolved {
    admitted
        .resolve(&style_ref(), ClassId(1), mode, mask, doc, instance)
        .unwrap()
}
fn override_entry(scope: Scope, color: u8) -> Override {
    Override {
        field: FieldId(1),
        scope,
        value: literal(color),
    }
}

#[test]
fn color_style_all_state_masks_both_modes_and_separate_focus() {
    let admitted = Admitted::new(fixture(), &owners()).unwrap();
    for mode in [Mode::Light, Mode::Dark] {
        for bits in 0..32 {
            let mask = StateMask {
                selected: bits & 1 != 0,
                hovered: bits & 2 != 0,
                pressed: bits & 4 != 0,
                disabled: bits & 8 != 0,
                focus_visible: bits & 16 != 0,
            };
            let output = resolve(&admitted, mode, mask, &[], &[]);
            let expected = if mask.disabled {
                40
            } else if mask.pressed {
                30
            } else if mask.hovered {
                20
            } else if mask.selected {
                10
            } else if mode == Mode::Light {
                240
            } else {
                8
            };
            assert_eq!(
                output.body[&FieldId(1)].color,
                [expected, expected, expected, 255]
            );
            assert_eq!(
                output.body[&FieldId(2)].color,
                [200, 200, 200, 255],
                "literal text never auto-inverts"
            );
            assert_eq!(output.focus.len(), usize::from(mask.focus_visible));
            if mask.focus_visible {
                assert_eq!(output.focus[&FieldId(3)].color, [50, 50, 50, 255]);
            }
        }
    }
}

#[test]
fn color_style_overrides_prioritize_named_state_then_instance_and_reset() {
    let admitted = Admitted::new(fixture(), &owners()).unwrap();
    let doc = [
        override_entry(Scope::All, 60),
        override_entry(Scope::State(State::Selected), 61),
        override_entry(Scope::State(State::Hover), 62),
        override_entry(Scope::State(State::Pressed), 63),
        override_entry(Scope::State(State::Disabled), 64),
    ];
    for bits in 0..16 {
        let mask = StateMask {
            selected: bits & 1 != 0,
            hovered: bits & 2 != 0,
            pressed: bits & 4 != 0,
            disabled: bits & 8 != 0,
            focus_visible: true,
        };
        let expected = if mask.disabled {
            64
        } else if mask.pressed {
            63
        } else if mask.hovered {
            62
        } else if mask.selected {
            61
        } else {
            60
        };
        let output = resolve(&admitted, Mode::Dark, mask, &doc, &[]);
        assert_eq!(output.body[&FieldId(1)].color[0], expected);
        assert_eq!(output.body[&FieldId(1)].layer, Layer::Document);
        let instance = [override_entry(Scope::All, 70)];
        let output = resolve(&admitted, Mode::Dark, mask, &doc, &instance);
        assert_eq!(
            output.body[&FieldId(1)].color[0],
            70,
            "instance All outranks document state"
        );
        assert_eq!(output.body[&FieldId(1)].layer, Layer::Instance);
        assert_eq!(output.body[&FieldId(1)].scope, Scope::All);
        assert_eq!(
            resolve(&admitted, Mode::Dark, mask, &doc, &[]).body[&FieldId(1)].color[0],
            expected,
            "reset reveals today's document layer"
        );
    }
    let expression = FieldValue::Variable(variable_ref());
    let output = resolve(
        &admitted,
        Mode::Light,
        StateMask {
            pressed: true,
            ..Default::default()
        },
        &[],
        &[Override {
            field: FieldId(1),
            scope: Scope::All,
            value: expression.clone(),
        }],
    );
    let field = &output.body[&FieldId(1)];
    assert_eq!(field.color, [240, 240, 240, 255]);
    assert_eq!(field.expression, expression);
    assert_eq!(field.mode, Mode::Light);
}

#[test]
fn color_style_exact_owners_rename_retirement_and_reverse_dependencies() {
    let mut source = fixture();
    let other_ref = SystemRef {
        owner: Owner::Library {
            library: otrios_interfaces::oib::library::LibraryId(format!("{}:base", "a".repeat(64))),
        },
        system: SystemId(1),
    };
    let other_variable = VariableRef {
        system: other_ref.clone(),
        variable: VariableId(1),
    };
    let mut other = source.systems[0].clone();
    other.reference = other_ref.clone();
    other.styles.clear();
    other.variables[0].values.dark = [90, 90, 90, 255];
    source.systems.push(other);
    source.systems[0].styles[0].defaults[0].value = FieldValue::Variable(other_variable.clone());
    let allowed = BTreeSet::from([system_ref().owner, other_ref.owner]);
    assert!(
        Admitted::new(source.clone(), &owners()).is_err(),
        "catalog cannot self-admit a library owner"
    );
    let admitted = Admitted::new(source.clone(), &allowed).unwrap();
    assert_eq!(
        resolve(&admitted, Mode::Dark, StateMask::default(), &[], &[]).body[&FieldId(1)].color[0],
        90
    );
    assert_eq!(
        admitted.dependencies()[&other_variable],
        vec![VariableUse {
            style: style_ref(),
            field: FieldId(1),
            state: None
        }]
    );
    assert!(!admitted.dependencies().contains_key(&variable_ref()));
    source.systems[0].styles[0].name = "renamed".into();
    source.systems[0].styles[0].availability = Availability::Retired;
    source.systems[1].variables[0].availability = Availability::Retired;
    let retired = Admitted::new(source.clone(), &allowed).unwrap();
    assert!(!retired.selectable(&style_ref()));
    assert!(!retired.variable_selectable(&other_variable));
    assert_ne!(retired.revision(), admitted.revision());
    assert_eq!(
        resolve(&retired, Mode::Dark, StateMask::default(), &[], &[]),
        resolve(&admitted, Mode::Dark, StateMask::default(), &[], &[])
    );
    source.systems[1].reference.owner = Owner::Library {
        library: otrios_interfaces::oib::library::LibraryId(format!("{}:other", "a".repeat(64))),
    };
    let mut changed_owners = allowed;
    changed_owners.insert(source.systems[1].reference.owner.clone());
    assert!(
        Admitted::new(source, &changed_owners).is_err(),
        "variable references cannot change library namespace"
    );
}

#[test]
fn color_style_modes_inherit_and_stop_at_explicit_boundary() {
    assert_eq!(root_mode(RootMode::System, None), Mode::Dark);
    for host in [Mode::Light, Mode::Dark] {
        assert_eq!(root_mode(RootMode::System, Some(host)), host);
        assert_eq!(root_mode(RootMode::Light, Some(host)), Mode::Light);
        assert_eq!(root_mode(RootMode::Dark, Some(host)), Mode::Dark);
        assert_eq!(child_mode(ModeSelection::Inherit, host), host);
        let child = child_mode(ModeSelection::Light, host);
        assert_eq!(child_mode(ModeSelection::Inherit, child), Mode::Light);
        assert_eq!(child_mode(ModeSelection::Dark, child), Mode::Dark);
    }
}

#[test]
fn color_style_rejects_invalid_graphs_and_inactive_override_errors() {
    type InvalidCandidate = Box<dyn Fn(&mut Catalog)>;
    let invalid: Vec<InvalidCandidate> = vec![
        Box::new(|c| c.format = "future".into()),
        Box::new(|c| c.systems[0].reference.system = SystemId(0)),
        Box::new(|c| c.systems[0].variables[0].name = "has space".into()),
        Box::new(|c| {
            let duplicate = c.systems[0].variables[0].clone();
            c.systems[0].variables.push(duplicate);
        }),
        Box::new(|c| {
            c.systems[0].styles[0].defaults.pop();
        }),
        Box::new(|c| c.systems[0].styles[0].contract = ContractId(99)),
        Box::new(|c| c.systems[0].styles[0].patches[0].fields[0].field = FieldId(3)),
        Box::new(|c| c.systems[0].styles[0].patches[4].fields[0].field = FieldId(1)),
        Box::new(|c| c.systems[0].styles[0].patches.swap(0, 1)),
        Box::new(|c| {
            c.systems[0].styles[0].defaults[0].value = FieldValue::Variable(VariableRef {
                variable: VariableId(99),
                ..variable_ref()
            })
        }),
    ];
    for mutate in invalid {
        let mut source = fixture();
        mutate(&mut source);
        assert!(Admitted::new(source, &owners()).is_err());
    }
    let admitted = Admitted::new(fixture(), &owners()).unwrap();
    assert!(
        admitted
            .resolve(
                &style_ref(),
                ClassId(99),
                Mode::Dark,
                StateMask::default(),
                &[],
                &[]
            )
            .is_err()
    );
    let bad = Override {
        field: FieldId(1),
        scope: Scope::State(State::Pressed),
        value: FieldValue::Variable(VariableRef {
            variable: VariableId(99),
            ..variable_ref()
        }),
    };
    assert!(
        admitted
            .resolve(
                &style_ref(),
                ClassId(1),
                Mode::Dark,
                StateMask::default(),
                &[bad],
                &[]
            )
            .is_err(),
        "dormant invalid override rejects"
    );
    for bad in [
        Override {
            field: FieldId(3),
            scope: Scope::All,
            value: literal(1),
        },
        Override {
            field: FieldId(1),
            scope: Scope::State(State::Focus),
            value: literal(1),
        },
    ] {
        assert!(
            admitted
                .resolve(
                    &style_ref(),
                    ClassId(1),
                    Mode::Dark,
                    StateMask::default(),
                    &[bad],
                    &[]
                )
                .is_err()
        );
    }
    let duplicate = [override_entry(Scope::All, 1), override_entry(Scope::All, 2)];
    assert!(
        admitted
            .resolve(
                &style_ref(),
                ClassId(1),
                Mode::Dark,
                StateMask::default(),
                &duplicate,
                &[]
            )
            .is_err()
    );
}

#[test]
fn color_style_codec_roundtrip_rejects_mutation_and_has_fixed_digest() {
    let admitted = Admitted::new(fixture(), &owners()).unwrap();
    let decoded = Admitted::decode(admitted.bytes(), &owners()).unwrap();
    assert_eq!(decoded.revision(), admitted.revision());
    for bytes in [&admitted.bytes()[..admitted.bytes().len() - 1], b"invalid"] {
        assert!(Admitted::decode(bytes, &owners()).is_err());
    }
    let mut trailing = admitted.bytes().to_vec();
    trailing.push(0);
    assert!(Admitted::decode(&trailing, &owners()).is_err());
    assert!(Admitted::decode(&vec![0; MAX_BYTES + 1], &owners()).is_err());
    let empty = Admitted::new(
        Catalog {
            format: FORMAT.into(),
            systems: vec![],
        },
        &owners(),
    )
    .unwrap();
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        hex(empty.bytes()),
        concat!(
            "4f5472694f53006f69622d636f6c6f722d7374796c65732d763100",
            "666f726d6174001a6f7472696f732d6f69622d636f6c6f722d7374796c65732d763100",
            "73797374656d730000022d0b0201022a071428042401"
        )
    );
    assert_eq!(
        hex(&empty.revision()),
        "dca8b59d00a034f071efd689e897adcd75c154f7d71f9b431ce25524e7538c36"
    );
}

#[test]
fn color_style_codec_rejects_missing_modes_and_unknown_types() {
    let source = serde_json::to_value(fixture()).unwrap();
    let mut missing = source.clone();
    missing["systems"][0]["variables"][0]["values"]
        .as_object_mut()
        .unwrap()
        .remove("dark");
    let mut unknown = source;
    unknown["systems"][0]["contracts"][0]["fields"][0]["value_type"] = "Geometry".into();
    for malformed in [missing, unknown] {
        let mut bytes = HEADER.to_vec();
        bytes.extend(flexbuffers::to_vec(&malformed).unwrap());
        assert!(Admitted::decode(&bytes, &owners()).is_err());
    }
}

#[test]
fn color_style_resource_limits_reject_before_resolution() {
    let mut source = fixture();
    source.systems[0].variables = (1..=MAX_VARIABLES + 1)
        .map(|i| Variable {
            id: VariableId(i as u64),
            name: format!("v{i}"),
            availability: Availability::Active,
            values: ColorModes {
                light: [0; 4],
                dark: [0; 4],
            },
        })
        .collect();
    assert!(Admitted::new(source.clone(), &owners()).is_err());
    source.systems[0].variables.pop();
    assert!(
        Admitted::new(source, &owners()).is_ok(),
        "exact variable ceiling is admitted"
    );
    let mut source = fixture();
    source.systems[0].contracts[0].fields = (1..=MAX_FIELDS + 1)
        .map(|i| Field {
            id: FieldId(i as u64),
            name: format!("f{i}"),
            domain: Domain::Body,
        })
        .collect();
    assert!(Admitted::new(source, &owners()).is_err());
    let mut source = fixture();
    let fields: Vec<_> = (1..=32).map(|i| entry(i, literal(1))).collect();
    source.systems[0].contracts[0].fields = (1..=32)
        .map(|i| Field {
            id: FieldId(i),
            name: format!("f{i}"),
            domain: Domain::Body,
        })
        .collect();
    source.systems[0].styles = (1..=64)
        .map(|i| Style {
            id: StyleId(i),
            name: format!("s{i}"),
            availability: Availability::Active,
            contract: ContractId(1),
            defaults: fields.clone(),
            patches: [
                State::Selected,
                State::Hover,
                State::Pressed,
                State::Disabled,
            ]
            .into_iter()
            .map(|state| Patch {
                state,
                fields: fields.clone(),
            })
            .collect(),
        })
        .collect();
    assert!(matches!(Admitted::new(source, &owners()), Err(message) if message.contains("limit")));
    let mut source = fixture();
    let system = source.systems[0].clone();
    source.systems = (1..=MAX_SYSTEMS + 1)
        .map(|i| {
            let mut system = system.clone();
            system.reference.system = SystemId(i as u64);
            system
        })
        .collect();
    assert!(Admitted::new(source.clone(), &owners()).is_err());
    source.systems.pop();
    assert!(
        Admitted::new(source, &owners()).is_ok(),
        "exact system ceiling is admitted"
    );
    let mut source = fixture();
    let style = source.systems[0].styles[0].clone();
    source.systems[0].styles = (1..=MAX_STYLES + 1)
        .map(|i| {
            let mut style = style.clone();
            style.id = StyleId(i as u64);
            style.name = format!("s{i}");
            style
        })
        .collect();
    assert!(Admitted::new(source.clone(), &owners()).is_err());
    source.systems[0].styles.pop();
    assert!(
        Admitted::new(source, &owners()).is_ok(),
        "exact style ceiling is admitted"
    );
}
