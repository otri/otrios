// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded dependencies compiled from an admitted UI, never a client grant.
use super::*;

#[derive(Clone, Debug)]
pub struct StateSelection {
    pub(crate) fields: u64,
    pub(crate) signals: u64,
}
impl StateSelection {
    pub(crate) fn all(schema: &StateSchema) -> Self {
        let fields = schema
            .fields
            .iter()
            .fold(0, |mask, f| mask | (1 << (f.id - 1)));
        Self {
            fields,
            signals: fields,
        }
    }
    pub fn from_ui(ui: &Ui) -> Result<Self> {
        validate_schema(&ui.state_schema)?;
        if ui.selectors.len() > 128
            || ui.handlers.len() > 128
            || ui.navigation.len() > 32
            || ui
                .behavior
                .as_ref()
                .is_some_and(|b| b.writes.len() > MAX_FIELDS)
        {
            return Err("UI dependency plan exceeds bounded limits".into());
        }
        let allowed = Self::all(&ui.state_schema).fields;
        let mut fields = 0;
        let mut signals = 0;
        let mut add = |id: u16| -> Result<()> {
            if !(1..=MAX_FIELDS as u16).contains(&id) || allowed & (1 << (id - 1)) == 0 {
                return Err("UI dependency is not a schema field".into());
            }
            fields |= 1 << (id - 1);
            Ok(())
        };
        for selector in &ui.selectors {
            if selector.field == 0
                && !matches!(
                    selector.projection,
                    Projection::KeyboardLabel { .. }
                        | Projection::KeyboardVisible
                        | Projection::SessionControls
                        | Projection::ReorderLabel
                )
            {
                return Err("Shared selector needs a schema field".into());
            }
            if selector.field != 0 {
                add(selector.field)?;
                if matches!(selector.projection, Projection::WindowSignal { .. }) {
                    signals |= 1 << (selector.field - 1);
                }
            }
        }
        for handler in &ui.handlers {
            for id in &handler.fields {
                add(*id)?;
            }
        }
        // ABI v1 passes the handler's current field; include permitted reducer
        // fields conservatively so future event changes cannot read defaults.
        if let Some(behavior) = &ui.behavior {
            for id in &behavior.writes {
                add(*id)?;
            }
        }
        match &ui.presentation {
            Some(Presentation::Motion {
                playing,
                speed,
                count,
                load,
                ..
            }) => {
                for id in [playing, speed, count, load] {
                    add(*id)?;
                }
            }
            Some(Presentation::Reorder { items }) => {
                if items.is_empty() || items.len() > 32 {
                    return Err("Invalid reorder dependency count".into());
                }
                for item in items {
                    add(item.field)?;
                }
            }
            Some(Presentation::Editor { fields }) => {
                for id in fields {
                    add(*id)?;
                }
            }
            _ => {}
        }
        Ok(Self { fields, signals })
    }
    pub(crate) fn coverage(&self, slot: Option<usize>) -> Result<crate::merkle::Coverage> {
        let mut registers = vec![vec![]];
        if let Some(slot) = slot {
            registers.push(state::path(2, slot));
        }
        for id in 0..MAX_FIELDS {
            if self.fields & (1 << id) != 0 {
                registers.push(state::path(1, id));
            }
            if self.signals & (1 << id) != 0 {
                registers.push(state::path(3, id));
            }
        }
        crate::merkle::Coverage::new(&registers, &[]).map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otrios_interfaces::layout::Value;
    fn ui() -> Ui {
        Ui {
            format: "otrios-canvas-v2".into(),
            ui_ref: "test:ui".into(),
            title: "dependencies".into(),
            state_schema: StateSchema {
                id: "dependencies".into(),
                fields: (1..=9)
                    .map(|id| Field {
                        id,
                        name: format!("field-{id}"),
                        default: Value::Number(0.),
                        writable: true,
                    })
                    .collect(),
            },
            layout: vec![],
            selectors: vec![
                Selector {
                    local: 1,
                    field: 1,
                    projection: Projection::Identity,
                },
                Selector {
                    local: 2,
                    field: 8,
                    projection: Projection::WindowSignal { seconds: 300 },
                },
                Selector {
                    local: 3,
                    field: 0,
                    projection: Projection::KeyboardVisible,
                },
            ],
            handlers: vec![Handler {
                control: 1,
                event: 1,
                kind: GestureKind::Activate,
                operation: Operation::Set,
                fields: vec![2],
            }],
            navigation: vec![],
            behavior: Some(BehaviorRef {
                sha256: vec![1; 32],
                abi: 1,
                writes: vec![3],
            }),
            presentation: Some(Presentation::Motion {
                three_d: false,
                playing: 4,
                speed: 5,
                count: 6,
                load: 7,
            }),
        }
    }
    #[test]
    fn coverage_includes_declared_reads_and_signals_but_excludes_unused_defaults() {
        let mut ui = ui();
        let selected = StateSelection::from_ui(&ui).unwrap();
        assert_eq!(selected.fields, 0xff);
        assert_eq!(selected.signals, 1 << 7);
        ui.presentation = Some(Presentation::Editor { fields: vec![4, 5] });
        let selected = StateSelection::from_ui(&ui).unwrap();
        assert_eq!(selected.fields, 0x9f);
        assert_eq!(selected.signals, 1 << 7);
        ui.selectors.push(Selector {
            local: 4,
            field: 9,
            projection: Projection::Identity,
        });
        assert_ne!(StateSelection::from_ui(&ui).unwrap().fields & (1 << 8), 0);
    }
    #[test]
    fn invalid_or_unbounded_dependency_plans_are_rejected() {
        let mut ui = ui();
        ui.handlers[0].fields.push(10);
        assert!(StateSelection::from_ui(&ui).is_err());
        let mut ui = self::ui();
        ui.selectors[0].field = 0;
        assert!(StateSelection::from_ui(&ui).is_err());
        let mut ui = self::ui();
        ui.selectors = vec![ui.selectors[0].clone(); 129];
        assert!(StateSelection::from_ui(&ui).is_err());
        let mut ui = self::ui();
        ui.state_schema.fields[0].id = 0;
        assert!(StateSelection::from_ui(&ui).is_err());
        assert!(StateAdapter::new(ui.state_schema, "bad", &[], [1; 16]).is_err());
    }
}
