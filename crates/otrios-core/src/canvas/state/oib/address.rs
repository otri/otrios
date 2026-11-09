// SPDX-License-Identifier: LGPL-3.0-only
//! Semantic property keys, independent of lowering tables and renderer slots.
use super::*;
use otrios_interfaces::oib::designer::PropertyAddress;

pub(super) fn path(branch: u64, address: &PropertyAddress) -> Result<Vec<u64>> {
    let mut path = vec![4, branch];
    match address {
        PropertyAddress::Definition {
            definition,
            node,
            property,
        } => {
            path.extend([0, definition.0, node.0, property.0]);
        }
        PropertyAddress::Occurrence {
            instance,
            path: property,
        } => {
            if property.references.len() > 8 {
                return Err("OIB property reference depth".into());
            }
            path.extend([1, instance.0, property.references.len() as u64]);
            path.extend(property.references.iter().map(|node| node.0));
            path.extend([property.node.0, property.property.0]);
        }
        PropertyAddress::Placement { .. } | PropertyAddress::Structural { .. } => {
            return Err("Graph source fields are structural".into());
        }
    }
    Ok(path)
}

pub(super) fn cell_path(
    seed: &otrios_interfaces::oib::Seed,
    branch: u64,
    cell: u64,
) -> Result<Vec<u64>> {
    path(
        branch,
        &super::super::super::oib::document::address_for_cell(seed, cell)?,
    )
}

pub(super) fn schema(
    scope: &str,
    seed: &otrios_interfaces::oib::Seed,
    cell: u64,
) -> Result<merkle::Hash> {
    let address = super::super::super::oib::document::address_for_cell(seed, cell)?;
    Ok(digest(
        &[
            b"OTriOS\0oib-property-v2\0".as_slice(),
            scope.as_bytes(),
            &bytes(&address)?,
        ]
        .concat(),
    ))
}

/// Check the complete path set, including unknown internal branches. The Merkle
/// library also enforces its real fanout/depth/node/byte limits at construction.
pub(super) fn leaves(
    tree: &Snapshot,
    prefix: &mut Vec<u64>,
    found: &mut BTreeSet<Vec<u64>>,
) -> Result<()> {
    if tree.register().is_some() {
        if tree.child_keys().next().is_some() || !found.insert(prefix.clone()) {
            return Err("Invalid OIB property leaf".into());
        }
    } else if tree.child_keys().next().is_none() {
        return Err("Empty OIB property branch".into());
    }
    for key in tree.child_keys() {
        prefix.push(key);
        leaves(&tree.subtree(&[key]).map_err(err)?, prefix, found)?;
        prefix.pop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use otrios_interfaces::oib::designer::*;

    #[test]
    fn address_paths_are_direct_distinct_and_fit_merkle_depth() {
        let a = PropertyAddress::Occurrence {
            instance: InstanceId(8),
            path: PropertyPath {
                references: (1..=8).map(NodeId).collect(),
                node: NodeId(9),
                property: PropertyId(10),
            },
        };
        let mut b = a.clone();
        if let PropertyAddress::Occurrence { path, .. } = &mut b {
            path.references.reverse();
        }
        assert_ne!(path(1, &a).unwrap(), path(1, &b).unwrap());
        assert_eq!(
            path(1, &a).unwrap(),
            vec![4, 1, 1, 8, 8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
        assert!(path(1, &a).unwrap().len() <= merkle::MAX_DEPTH);
        assert_ne!(path(1, &a).unwrap(), path(2, &a).unwrap());
        assert_ne!(
            path(1, &a).unwrap(),
            path(
                1,
                &PropertyAddress::Definition {
                    definition: DefinitionId(8),
                    node: NodeId(9),
                    property: PropertyId(10),
                }
            )
            .unwrap()
        );
    }
}
