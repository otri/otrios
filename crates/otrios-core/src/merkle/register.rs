// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use std::collections::BTreeMap;

/// Unique writer incarnation, supplied by the host. Never reuse an actor after
/// losing its causal state. Dots/counters below are scoped to one logical leaf.
pub(crate) type Dot = (Actor, u64);

/// Causal multi-value register: concurrent assignments survive; a later write
/// that has observed them replaces them. None is a causal deletion, not absence
/// of metadata. No last-writer timestamp or implicit winner is invented.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Register {
    pub(crate) schema: Hash,
    pub(crate) context: BTreeMap<Actor, u64>,
    pub(crate) entries: BTreeMap<Dot, Option<Vec<u8>>>,
}
impl Register {
    pub fn schema(&self) -> Hash {
        self.schema
    }
    /// Includes concurrent deletion markers. Ordering is canonical dot order,
    /// NOT chronological or a conflict resolution rule. No duplicate-value fold.
    pub fn values(&self) -> impl Iterator<Item = (Actor, u64, Option<&[u8]>)> {
        self.entries
            .iter()
            .map(|((actor, counter), value)| (*actor, *counter, value.as_deref()))
    }
    pub(crate) fn empty(schema: Hash) -> Self {
        Self {
            schema,
            context: BTreeMap::new(),
            entries: BTreeMap::new(),
        }
    }
    pub(crate) fn assign(&self, actor: Actor, value: Option<&[u8]>) -> Result<Self> {
        if value.is_some_and(|v| v.len() > MAX_VALUE_BYTES) {
            return Err(Error::Limit);
        }
        let counter = self
            .context
            .get(&actor)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::ClockExhausted)?;
        let mut next = self.clone();
        next.context.insert(actor, counter);
        next.entries.clear();
        next.entries
            .insert((actor, counter), value.map(<[u8]>::to_vec));
        next.validate()?;
        Ok(next)
    }
    pub(crate) fn merge(&self, other: &Self) -> Result<Self> {
        if self.schema != other.schema {
            return Err(Error::Schema);
        }
        let mut next = self.clone();
        next.entries.clear();
        for (here, there) in [(self, other), (other, self)] {
            for (&dot, value) in &here.entries {
                if let Some(other_value) = there.entries.get(&dot) {
                    if value != other_value {
                        return Err(Error::ActorCollision);
                    }
                    next.entries.insert(dot, value.clone());
                } else if there.context.get(&dot.0).copied().unwrap_or(0) < dot.1 {
                    next.entries.insert(dot, value.clone());
                }
            }
        }
        for (&actor, &counter) in &other.context {
            next.context
                .entry(actor)
                .and_modify(|n| *n = (*n).max(counter))
                .or_insert(counter);
        }
        next.validate()?;
        Ok(next)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.context.is_empty()
            || self.context.len() > MAX_ACTORS
            || self.entries.is_empty()
            || self.entries.len() > MAX_ACTORS
        {
            return Err(Error::Limit);
        }
        if self.context.values().any(|n| *n == 0) {
            return Err(Error::Invalid);
        }
        // One live dot per actor. Its counter is that actor's observed maximum;
        // overwritten dots remain in the version vector, not the live set.
        for (&(actor, counter), value) in &self.entries {
            if self.context.get(&actor) != Some(&counter) {
                return Err(Error::Invalid);
            }
            if value.as_ref().is_some_and(|v| v.len() > MAX_VALUE_BYTES) {
                return Err(Error::Limit);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_actor_clocks_reject_without_wrapping_or_losing_causality() {
        let actor = [1; 16];
        let reg = Register {
            schema: [7; 32],
            context: BTreeMap::from([(actor, u64::MAX)]),
            entries: BTreeMap::from([((actor, u64::MAX), None)]),
        };
        reg.validate().unwrap();
        assert_eq!(reg.assign(actor, Some(b"x")), Err(Error::ClockExhausted));
        let mut invalid = reg.clone();
        invalid.entries = BTreeMap::from([((actor, 0), None)]);
        assert_eq!(invalid.validate(), Err(Error::Invalid));
    }
}
