// SPDX-License-Identifier: LGPL-3.0-only
//! Fixed, bounded coverage over stable keys. This is a coherent materialized
//! view, not a membership proof against the authority's unfiltered root.
use super::*;
use std::collections::BTreeMap;

pub const MAX_COVERAGE_PATHS: usize = 192;

#[derive(Clone, Debug, Default)]
pub(crate) struct Selection {
    pub register: bool,
    pub subtree: bool,
    pub children: BTreeMap<u64, Selection>,
}

/// Registers select exactly one node's causal register; subtrees select that
/// node and all descendants. Ancestors are structural containers and reveal no
/// register payload unless separately selected. Missing paths remain watched.
#[derive(Clone, Debug)]
pub struct Coverage {
    pub(crate) root: Selection,
}
impl Coverage {
    pub fn new(registers: &[Path], subtrees: &[Path]) -> Result<Self> {
        if registers.len().saturating_add(subtrees.len()) > MAX_COVERAGE_PATHS {
            return Err(Error::Limit);
        }
        let mut root = Selection::default();
        for (paths, subtree) in [(registers, false), (subtrees, true)] {
            for path in paths {
                path_check(path)?;
                let mut node = &mut root;
                for key in path {
                    node = node.children.entry(*key).or_default();
                }
                if subtree {
                    node.subtree = true;
                } else {
                    node.register = true;
                }
            }
        }
        Ok(Self { root })
    }
    /// All requested targets need a server grant. Ancestors introduced only to
    /// address those targets do not need grants and expose no ancestor values.
    pub(crate) fn authorize(&self, allowed_roots: &[Path]) -> Result<()> {
        if allowed_roots.len() > MAX_COVERAGE_PATHS {
            return Err(Error::Limit);
        }
        for root in allowed_roots {
            path_check(root)?;
        }
        fn visit(node: &Selection, path: &mut Path, allowed: &[Path]) -> Result<()> {
            if (node.register || node.subtree) && !allowed.iter().any(|r| path.starts_with(r)) {
                return Err(Error::Forbidden);
            }
            for (key, child) in &node.children {
                path.push(*key);
                visit(child, path, allowed)?;
                path.pop();
            }
            Ok(())
        }
        visit(&self.root, &mut Vec::new(), allowed_roots)
    }
}
