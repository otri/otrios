// SPDX-License-Identifier: LGPL-3.0-only
//! Disposable dependency checks for confirmed, paint-only property activation.
use super::*;
use crate::merkle;
use otrios_interfaces::scene::Color;
use std::sync::Arc;

/// All structural compiler inputs, together with the property's current values.
/// Equal property registers alone cannot establish that target identities remain
/// valid after a library, connection, placement or live-instance change.
#[derive(Clone)]
pub(crate) struct Basis {
    pub signed: Arc<Ui>,
    pub binding: merkle::Hash,
    pub source: merkle::Hash,
    pub live: BTreeSet<u64>,
    pub values: BTreeMap<u64, Option<Value>>,
}
impl Basis {
    pub(crate) fn same_structure(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.signed, &other.signed)
            && self.binding == other.binding
            && self.source == other.source
            && self.live == other.live
    }
}

/// Classify effective dependencies using the admitted, expanded target table.
/// Return `None` for every non-color edit, including untargeted numeric/text
/// values which can feed graph lowering outside the ordinary terminal outputs.
/// Inheritance and terminal-output precedence are resolved before collecting
/// changes, so a shadowed definition color cannot repaint an override.
pub(crate) fn changes(
    seed: &Seed,
    previous: &BTreeMap<u64, Option<Value>>,
    next: &BTreeMap<u64, Option<Value>>,
) -> Result<Option<BTreeMap<u16, Color>>> {
    // Graph lowering records direct/affine terminal outputs. A composed color
    // expression (for example Select with mutable color branches) is lowered
    // into literals without a terminal cell target. Until that expression has
    // its own dependency instruction, use the ordinary compiler for that graph.
    let untracked_color = |definition: &otrios_interfaces::oib::designer::ComponentDefinition| {
        definition
            .nodes
            .iter()
            .filter_map(|node| node.visual.as_ref())
            .any(|visual| {
                let color = match &visual.shape {
                    Shape::Text { color, .. }
                    | Shape::Superellipse { color, .. }
                    | Shape::Circle { color } => color,
                    _ => return false,
                };
                !matches!(color, Expr::Literal(_) | Expr::Parameter(_))
            })
    };
    if seed
        .graph
        .as_ref()
        .is_some_and(|graph| graph.definitions.iter().any(untracked_color))
        || seed
            .resolved_libraries
            .iter()
            .any(|library| library.graph.definitions.iter().any(untracked_color))
    {
        return Ok(None);
    }
    for id in previous.keys().chain(next.keys()) {
        if previous.get(id) != next.get(id) && !seed.cells.iter().any(|cell| cell.id == *id) {
            return Ok(None);
        }
    }
    let outputs = document::output_cells(seed)?;
    let mut colors = BTreeMap::new();
    for cell in &seed.cells {
        let old = effective(seed, previous, cell.id)?;
        let new = effective(seed, next, cell.id)?;
        if old == new {
            continue;
        }
        let (Value::Color(_), Value::Color(color)) = (&old, &new) else {
            return Ok(None);
        };
        validate_value(cell, &new)?;
        for target in &cell.targets {
            if outputs.get(&document::output_key(target)) != Some(&cell.id) {
                continue;
            }
            if target.property != Property::Color {
                return Ok(None);
            }
            colors.insert(target.node, *color);
        }
    }
    Ok(Some(colors))
}
