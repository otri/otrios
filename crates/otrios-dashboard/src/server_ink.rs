// SPDX-License-Identifier: Apache-2.0
//! Bounded five-second relay, multiplexed by the host session. No file access.
use otrios_interfaces::ink::{self, Point};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, broadcast};
#[derive(Clone, Copy)]
pub(super) struct Event {
    at: Instant,
    peer: u32,
    point: Point,
}
pub(super) struct Room {
    recent: Mutex<VecDeque<Event>>,
    bus: broadcast::Sender<Vec<Event>>,
    slots: Arc<Semaphore>,
    limit: usize,
    next: AtomicU32,
}
pub(super) struct Subscription {
    id: u32,
    pub updates: broadcast::Receiver<Vec<Event>>,
    _permit: OwnedSemaphorePermit,
    window: Instant,
    count: usize,
}
impl Room {
    pub(super) fn new(peers: usize) -> Result<Self, String> {
        let mut seed = [0; 4];
        getrandom::fill(&mut seed).map_err(|_| "Ink session seed unavailable")?;
        Ok(Self {
            recent: Mutex::new(VecDeque::new()),
            bus: broadcast::channel(128).0,
            // One ink subscription per admitted session; never impose a hidden
            // lower quota on clients viewing an ink-enabled app while idle.
            slots: Arc::new(Semaphore::new(peers)),
            limit: peers,
            next: AtomicU32::new(u32::from_le_bytes(seed).max(1)),
        })
    }
    pub(super) fn subscribers(&self) -> usize {
        self.limit - self.slots.available_permits()
    }
    pub(super) async fn expire(&self) {
        self.recent
            .lock()
            .await
            .retain(|e| e.at.elapsed() < Duration::from_millis(ink::LIFETIME_MS));
    }
    pub(super) async fn subscribe(&self) -> Result<(Subscription, Vec<u8>), String> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Ink room full")?;
        let updates = self.bus.subscribe();
        let id = self.next.fetch_add(1, Ordering::Relaxed).max(1);
        let mut recent = self.recent.lock().await;
        recent.retain(|e| e.at.elapsed() < Duration::from_millis(ink::LIFETIME_MS));
        let mut bytes = ink::welcome(id);
        bytes.extend(encode(&recent.iter().copied().collect::<Vec<_>>()));
        Ok((
            Subscription {
                id,
                updates,
                _permit: permit,
                window: Instant::now(),
                count: 0,
            },
            bytes,
        ))
    }
    pub(super) async fn receive(&self, sub: &mut Subscription, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty()
            || bytes.len() > 32 * ink::CLIENT_BYTES
            || !bytes.len().is_multiple_of(ink::CLIENT_BYTES)
        {
            return Err("Invalid ink batch".into());
        }
        let points = bytes
            .chunks_exact(ink::CLIENT_BYTES)
            .map(Point::decode)
            .collect::<Result<Vec<_>, _>>()?;
        if sub.window.elapsed() >= Duration::from_secs(1) {
            sub.window = Instant::now();
            sub.count = 0;
        }
        sub.count += points.len();
        if sub.count > 64 {
            return Err("Ink rate limit".into());
        }
        let at = Instant::now();
        let events = points
            .into_iter()
            .map(|point| Event {
                at,
                peer: sub.id,
                point,
            })
            .collect::<Vec<_>>();
        let mut recent = self.recent.lock().await;
        recent.retain(|e| e.at.elapsed() < Duration::from_millis(ink::LIFETIME_MS));
        for event in &events {
            if recent.len() >= 1024 {
                recent.pop_front();
            }
            recent.push_back(*event);
        }
        drop(recent);
        let _ = self.bus.send(events);
        Ok(())
    }
}
pub(super) fn encode(events: &[Event]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| {
            let age = e.at.elapsed().as_millis();
            (age < u128::from(ink::LIFETIME_MS)).then(|| ink::envelope(e.peer, age as u16, e.point))
        })
        .flatten()
        .collect()
}
