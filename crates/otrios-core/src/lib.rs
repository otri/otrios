// SPDX-License-Identifier: LGPL-3.0-only
//! Snapshot validation scaffold. Storage, atomic publication, and IPC are pending.

use std::collections::BTreeMap;

use otrios_interfaces::{ContentId, Generation, ObjectId, RevisionRef};

/// An immutable aggregate manifest. References are validated for uniqueness here;
/// the future object store must validate content and availability before publish.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    generation: Generation,
    members: BTreeMap<ObjectId, ContentId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    DuplicateObject(ObjectId),
}

impl Snapshot {
    pub fn new(
        generation: Generation,
        revisions: impl IntoIterator<Item = RevisionRef>,
    ) -> Result<Self, SnapshotError> {
        let mut members = BTreeMap::new();
        for revision in revisions {
            if members.insert(revision.object, revision.content).is_some() {
                return Err(SnapshotError::DuplicateObject(revision.object));
            }
        }
        Ok(Self {
            generation,
            members,
        })
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn get(&self, object: ObjectId) -> Option<ContentId> {
        self.members.get(&object).copied()
    }

    pub fn members(&self) -> impl Iterator<Item = RevisionRef> + '_ {
        self.members
            .iter()
            .map(|(&object, &content)| RevisionRef { object, content })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(object: u8, content: u8) -> RevisionRef {
        RevisionRef {
            object: ObjectId([object; 16]),
            content: ContentId([content; 32]),
        }
    }

    #[test]
    fn duplicate_logical_objects_are_rejected_even_if_content_matches() {
        let item = revision(1, 2);
        assert_eq!(
            Snapshot::new(Generation(0), [item, item]),
            Err(SnapshotError::DuplicateObject(item.object))
        );
    }

    #[test]
    fn constructing_a_later_snapshot_preserves_the_old_snapshot() {
        let old = Snapshot::new(Generation(1), [revision(1, 2)]).unwrap();
        let new = Snapshot::new(Generation(2), [revision(1, 3)]).unwrap();
        assert_eq!(old.get(ObjectId([1; 16])), Some(ContentId([2; 32])));
        assert_eq!(new.get(ObjectId([1; 16])), Some(ContentId([3; 32])));
        assert_eq!(old.generation(), Generation(1));
    }

    #[test]
    fn manifest_order_is_independent_of_input_order() {
        let a = Snapshot::new(Generation(1), [revision(2, 4), revision(1, 3)]).unwrap();
        let b = Snapshot::new(Generation(1), [revision(1, 3), revision(2, 4)]).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a.members().collect::<Vec<_>>(),
            vec![revision(1, 3), revision(2, 4)]
        );
        assert_eq!(a.get(ObjectId([9; 16])), None);
    }
}

#[cfg(feature = "dashboard")]
pub mod dashboard;
