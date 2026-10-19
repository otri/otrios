// SPDX-License-Identifier: LGPL-3.0-only
//! Advisory event-time projection. Timestamps never participate in CRDT ordering
//! or authorization. The immutable index is rebuilt only when sample operations
//! change; a scene queries only its five-minute window once per clock second.
use super::{Result, err};
use automerge::{AutoCommit, ChangeHash, ROOT, ReadDoc};
use std::{collections::BTreeMap, sync::Arc};

pub const WINDOW_SECONDS: i64 = 5 * 60;
#[derive(Clone, Debug, Default)]
pub struct ToggleSignal(Arc<[(i64, f32)]>);
impl ToggleSignal {
    pub(super) fn read(doc: &mut AutoCommit) -> Result<Self> {
        let (_, list) = doc
            .get(ROOT, "samples")
            .map_err(err)?
            .ok_or("Missing samples")?;
        let rows: Vec<_> = doc
            .list_range(&list, ..)
            .map(|row| (row.id(), row.value.into_value().as_i64()))
            .collect();
        let mut samples = Vec::new();
        for (id, value) in rows {
            let value = value.ok_or("Invalid signal sample")?;
            if !(0..=100).contains(&value) {
                return Err("Signal sample outside 0..100".into());
            }
            let time = doc.hash_for_opid(&id).and_then(|hash| {
                doc.get_change_meta_by_hash(&hash)
                    .map(|meta| meta.timestamp)
            });
            // Seed and old commits carry zero time. Do not paint fabricated
            // history at load time or restart the retention window on receipt.
            if let Some(time) = time.filter(|time| *time > 0) {
                samples.push((time, value as f32));
            }
        }
        samples.sort_by_key(|s| s.0);
        Ok(Self(samples.into()))
    }
    pub(super) fn changed(doc: &mut AutoCommit, known: &[ChangeHash]) -> Result<bool> {
        use automerge::legacy::{Key, ObjectId, OpId};
        let (_, list) = doc
            .get(ROOT, "samples")
            .map_err(err)?
            .ok_or("Missing samples")?;
        let list = match list {
            automerge::ObjId::Id(counter, actor, _) => ObjectId::Id(OpId::new(counter, &actor)),
            _ => return Err("Invalid signal samples object".into()),
        };
        // Decode only newly received changes, never the full saved history.
        Ok(doc.get_changes(known).iter().any(|change| change.decode().operations.iter().any(|op| {
            op.obj == list || matches!((&op.obj, &op.key), (ObjectId::Root, Key::Map(key)) if key == "samples")
        })))
    }
    /// At most 151 explicit time-positioned points: the latest transition in
    /// each two-second visual bucket. No event is made younger by reconnection.
    /// Future timestamps are hidden until their advisory wall-clock time arrives.
    pub fn project(&self, now: i64) -> Vec<[f32; 2]> {
        let start = now.saturating_sub(WINDOW_SECONDS);
        let first = self.0.partition_point(|s| s.0 <= start);
        let last = self.0.partition_point(|s| s.0 <= now);
        let mut buckets = BTreeMap::new();
        for &(time, value) in &self.0[first..last] {
            let offset = time.saturating_sub(start);
            buckets.insert(offset / 2, [offset as f32 / WINDOW_SECONDS as f32, value]);
        }
        buckets.into_values().collect()
    }
}

/// Hosts supply wall time explicitly on the web. The native default is useful
/// for headless commands; animation and touch-repeat timers stay monotonic.
pub fn wall_time_seconds() -> i64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |v| v.as_secs().min(i64::MAX as u64) as i64)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{Replica, SyncSession};
    fn sync(a: &mut Replica, b: &mut Replica) {
        let (mut sa, mut sb) = (SyncSession::default(), SyncSession::default());
        for _ in 0..100 {
            let to_b = a.generate_sync(&mut sa).unwrap();
            let to_a = b.generate_sync(&mut sb).unwrap();
            if to_b.is_none() && to_a.is_none() {
                return;
            }
            if let Some(bytes) = to_b {
                b.receive_demo_sync(&mut sb, &bytes).unwrap();
            }
            if let Some(bytes) = to_a {
                a.receive_sync(&mut sa, &bytes).unwrap();
            }
        }
        panic!("Signal sync did not quiesce");
    }
    #[test]
    fn elapsed_time_positions_samples_and_expires_without_new_events_or_writes() {
        let mut replica = Replica::new().unwrap();
        assert!(replica.toggle_signal().unwrap().project(1000).is_empty());
        replica.set_enabled_at(false, 1000).unwrap();
        replica.set_enabled_at(true, 1120).unwrap();
        let signal = replica.toggle_signal().unwrap();
        let initial = signal.project(1120);
        assert_eq!(initial, vec![[0.6, 24.], [1., 78.]]);
        let before = replica.save();
        let shifted = signal.project(1180);
        assert_eq!(shifted, vec![[0.4, 24.], [0.8, 78.]]);
        assert_eq!(signal.project(1300), vec![[0.4, 78.]]);
        assert!(signal.project(1420).is_empty());
        assert_eq!(replica.save(), before); // clock-only projection never persists
        // Future/skewed event clocks do not leak points beyond the graph bounds.
        assert!(signal.project(999).is_empty());
    }
    #[test]
    fn old_timestampless_edits_stay_absent_and_many_toggles_use_time_not_last_32() {
        let mut replica = Replica::new().unwrap();
        replica.set_enabled_at(false, 0).unwrap();
        assert!(replica.toggle_signal().unwrap().project(2000).is_empty());
        for n in 0..40 {
            replica.set_enabled_at(n % 2 == 0, 2000 + n * 3).unwrap();
        }
        assert_eq!(replica.toggle_signal().unwrap().project(2120).len(), 40);
        assert!(replica.toggle_signal().unwrap().project(2420).is_empty());
        // Unrelated state edits preserve the same immutable sample index.
        let index = replica.toggle_signal().unwrap();
        replica.set_control(0, [731, 0]).unwrap();
        assert!(Arc::ptr_eq(&index.0, &replica.toggle_signal().unwrap().0));
    }
    #[test]
    fn concurrent_sync_reconnect_and_reload_keep_original_event_times() {
        let mut a = Replica::new().unwrap();
        let mut b = Replica::new().unwrap();
        a.set_enabled_at(false, 2000).unwrap();
        b.set_enabled_at(true, 2040).unwrap(); // concurrent equal-valued write is still an event
        sync(&mut a, &mut b);
        let expected = vec![[0.8, 24.], [280. / 300., 78.]];
        assert_eq!(a.toggle_signal().unwrap().project(2060), expected);
        assert_eq!(b.toggle_signal().unwrap().project(2060), expected);
        let mut restart = Replica::load(&a.save()).unwrap();
        let mut fresh = Replica::new().unwrap();
        fresh.set_wall_time(9999);
        sync(&mut restart, &mut fresh);
        assert_eq!(fresh.toggle_signal().unwrap().project(2060), expected);
        assert!(fresh.toggle_signal().unwrap().project(2340).is_empty());
    }
    #[test]
    fn dense_history_has_a_bounded_time_projection() {
        let signal = ToggleSignal((1..=1000).map(|t| (t, 24.)).collect::<Vec<_>>().into());
        let points = signal.project(1000);
        assert!(points.len() <= 151);
        assert!(points[0][0] > 0.);
        assert_eq!(points.last().unwrap()[0], 1.);
    }
}
