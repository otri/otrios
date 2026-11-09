// SPDX-License-Identifier: LGPL-3.0-only
//! Opt-in bounded coarse phase tracing. Durations are elapsed time, not CPU time.
//!
//! Enable with `OTRIOS_CORE_PROFILE=1` or [`set_enabled`]. Disabled spans use one
//! atomic check and do not read clocks or allocate. Enabled spans retain at most
//! 8192 completed records in RAM; [`take`] drains every native thread. Phase
//! names must be static technical labels, never document values or credentials.
//! Same-thread nesting permits exclusive elapsed time without double-counting
//! child phases. No per-element instrumentation belongs in this collector.
use serde::Serialize;

pub const MAX_EVENTS: usize = 8192;

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub id: u64,
    pub parent: Option<u64>,
    pub thread: u64,
    pub depth: u16,
    pub phase: &'static str,
    pub start_ns: u64,
    pub duration_ns: u64,
    pub self_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub format: &'static str,
    /// Approximate wall-time anchor for comparing separate processes. Within a
    /// process use `start_ns`; hosts can map [`origin`] into their own clock.
    pub origin_unix_us: u64,
    pub events: Vec<Event>,
    pub dropped: u64,
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        marker::PhantomData,
        rc::Rc,
        sync::{
            Mutex, OnceLock,
            atomic::{AtomicU8, AtomicU64, Ordering},
        },
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    // 0: environment not read, 1: disabled, 2: enabled.
    static ENABLED: AtomicU8 = AtomicU8::new(0);
    static NEXT_EVENT: AtomicU64 = AtomicU64::new(1);
    static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);
    static ORIGIN: OnceLock<(Instant, u64)> = OnceLock::new();
    static BUFFER: OnceLock<Mutex<Buffer>> = OnceLock::new();
    #[derive(Default)]
    struct Buffer {
        events: Vec<Event>,
        dropped: u64,
    }
    struct Active {
        id: u64,
        parent: Option<u64>,
        phase: &'static str,
        start: Instant,
        child_ns: u64,
        depth: u16,
    }
    thread_local! {
        static STACK: RefCell<Vec<Active>> = const { RefCell::new(Vec::new()) };
        static THREAD: Cell<u64> = Cell::new(NEXT_THREAD.fetch_add(1, Ordering::Relaxed));
    }
    fn anchor() -> &'static (Instant, u64) {
        ORIGIN.get_or_init(|| {
            let instant = Instant::now();
            let unix_us = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_micros().min(u128::from(u64::MAX)) as u64);
            (instant, unix_us)
        })
    }
    pub fn origin() -> Instant {
        anchor().0
    }
    pub fn enabled() -> bool {
        let mut value = ENABLED.load(Ordering::Relaxed);
        if value == 0 {
            let wanted = if std::env::var("OTRIOS_CORE_PROFILE").is_ok_and(|v| v == "1") {
                2
            } else {
                1
            };
            value = match ENABLED.compare_exchange(0, wanted, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => wanted,
                Err(current) => current,
            };
        }
        value == 2
    }
    pub fn set_enabled(value: bool) {
        ENABLED.store(if value { 2 } else { 1 }, Ordering::Relaxed);
    }
    fn buffer() -> &'static Mutex<Buffer> {
        BUFFER.get_or_init(|| Mutex::new(Buffer::default()))
    }
    /// A native span must be dropped on the thread that created it.
    pub struct Span {
        id: Option<u64>,
        _same_thread: PhantomData<Rc<()>>,
    }
    pub fn span(phase: &'static str) -> Span {
        let id = if enabled() {
            anchor();
            STACK.with(|stack| {
                let mut stack = stack.borrow_mut();
                // Defensive bound independent of the completed-record budget.
                if stack.len() >= 64 {
                    let mut buffer = buffer().lock().unwrap_or_else(|e| e.into_inner());
                    buffer.dropped = buffer.dropped.saturating_add(1);
                    return None;
                }
                let id = NEXT_EVENT.fetch_add(1, Ordering::Relaxed);
                let parent = stack.last().map(|frame| frame.id);
                let depth = stack.len() as u16;
                stack.push(Active {
                    id,
                    parent,
                    phase,
                    start: Instant::now(),
                    child_ns: 0,
                    depth,
                });
                Some(id)
            })
        } else {
            None
        };
        Span {
            id,
            _same_thread: PhantomData,
        }
    }
    impl Drop for Span {
        fn drop(&mut self) {
            let Some(id) = self.id else { return };
            let _ = STACK.try_with(|stack| {
                let Ok(mut stack) = stack.try_borrow_mut() else {
                    return;
                };
                let Some(index) = stack.iter().rposition(|frame| frame.id == id) else {
                    return;
                };
                let frame = stack.remove(index);
                let duration_ns = frame.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                if let Some(parent) = stack.iter_mut().rev().find(|p| Some(p.id) == frame.parent) {
                    parent.child_ns = parent.child_ns.saturating_add(duration_ns);
                }
                let event = Event {
                    id,
                    parent: frame.parent,
                    thread: THREAD.with(Cell::get),
                    depth: frame.depth,
                    phase: frame.phase,
                    start_ns: frame
                        .start
                        .duration_since(origin())
                        .as_nanos()
                        .min(u128::from(u64::MAX)) as u64,
                    duration_ns,
                    self_ns: duration_ns.saturating_sub(frame.child_ns),
                };
                let mut buffer = buffer().lock().unwrap_or_else(|e| e.into_inner());
                if buffer.events.len() < MAX_EVENTS {
                    buffer.events.push(event);
                } else {
                    buffer.dropped = buffer.dropped.saturating_add(1);
                }
            });
        }
    }
    pub fn take() -> Report {
        let mut buffer = buffer().lock().unwrap_or_else(|e| e.into_inner());
        Report {
            format: "otrios-core-phase-profile-v1",
            origin_unix_us: anchor().1,
            events: std::mem::take(&mut buffer.events),
            dropped: std::mem::take(&mut buffer.dropped),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{Span, enabled, origin, set_enabled, span, take};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    pub struct Span;
    pub fn enabled() -> bool {
        false
    }
    pub fn set_enabled(_: bool) {}
    pub fn span(_: &'static str) -> Span {
        Span
    }
    pub fn take() -> Report {
        Report {
            format: "otrios-core-phase-profile-v1",
            origin_unix_us: 0,
            events: vec![],
            dropped: 0,
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use wasm::{Span, enabled, set_enabled, span, take};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "process-global profiler; run alone with --test-threads=1"]
    fn coarse_profile_is_opt_in_nested_cross_thread_and_bounded() {
        set_enabled(false);
        take();
        drop(span("profile_test.disabled"));
        assert!(take().events.is_empty());
        set_enabled(true);
        {
            let _outer = span("profile_test.outer");
            drop(span("profile_test.child"));
            std::thread::spawn(|| drop(span("profile_test.worker")))
                .join()
                .unwrap();
        }
        set_enabled(false);
        let report = take();
        let outer = report
            .events
            .iter()
            .find(|e| e.phase == "profile_test.outer")
            .unwrap();
        let child = report
            .events
            .iter()
            .find(|e| e.phase == "profile_test.child")
            .unwrap();
        let worker = report
            .events
            .iter()
            .find(|e| e.phase == "profile_test.worker")
            .unwrap();
        assert_eq!(child.parent, Some(outer.id));
        assert_eq!(child.thread, outer.thread);
        assert_ne!(worker.thread, outer.thread);
        assert_eq!(worker.parent, None);
        assert!(outer.duration_ns >= child.duration_ns);
        assert!(outer.self_ns <= outer.duration_ns.saturating_sub(child.duration_ns));
        set_enabled(true);
        for _ in 0..MAX_EVENTS + 3 {
            drop(span("profile_test.bound"));
        }
        set_enabled(false);
        let report = take();
        assert_eq!(report.events.len(), MAX_EVENTS);
        assert!(report.dropped >= 3);
    }
}
