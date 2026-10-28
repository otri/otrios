// SPDX-License-Identifier: Apache-2.0
//! Bounded evdev-to-owner bridge. Graphics never owns this polling loop.
use crate::native_worker::Input;
use otrios_core::dashboard::display::{Calibration, Surface};
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub(super) struct Target {
    pub epoch: u32,
    pub viewport: [f32; 2],
    pub access: Arc<AtomicBool>,
}
pub(super) type SharedTarget = Arc<Mutex<Option<Target>>>;
pub(super) struct Sample {
    pub phase: i32,
    pub position: [f32; 2],
    pub sampled: Instant,
}
pub(super) fn run(
    mut read: impl FnMut() -> Result<Vec<Sample>, String>,
    target: SharedTarget,
    stop: Arc<AtomicBool>,
    tx: SyncSender<Input>,
    surface: Surface,
    calibration: Calibration,
) -> Result<(), String> {
    let mut contact: Option<Target> = None;
    let send = |event| {
        tx.try_send(event)
            .map_err(|e| format!("Touch dispatch queue unavailable: {e}"))
    };
    while !stop.load(Ordering::Acquire) {
        let events = read()?;
        if events.len() > 64 {
            return Err("Touch input batch limit".into());
        }
        let idle = events.is_empty();
        for sample in events {
            let current = target.lock().map_err(|_| "Touch target poisoned")?.clone();
            if sample.phase == 3 {
                if let Some(old) = contact.take().or(current) {
                    send(Input::CanvasCancel(old.epoch))?;
                }
                continue;
            }
            let phase = match sample.phase {
                0 => PointerPhase::Down,
                1 => PointerPhase::Move,
                2 => PointerPhase::Up,
                _ => return Err("Invalid input phase".into()),
            };
            if phase == PointerPhase::Down {
                if let Some(old) = contact.take() {
                    send(Input::CanvasCancel(old.epoch))?;
                }
                contact = current.clone().filter(|t| t.access.load(Ordering::Acquire));
            }
            let Some(owner) = contact.as_ref() else {
                continue;
            };
            if !owner.access.load(Ordering::Acquire)
                || current
                    .as_ref()
                    .is_none_or(|t| t.epoch != owner.epoch || t.viewport != owner.viewport)
            {
                send(Input::CanvasCancel(owner.epoch))?;
                contact = None;
                continue;
            }
            if let Some(position) = surface.touch_for(calibration, sample.position, owner.viewport)
            {
                send(Input::CanvasPointer(
                    owner.epoch,
                    PointerEvent {
                        id: 1,
                        phase,
                        position,
                    },
                    sample.sampled,
                ))?;
            } else {
                send(Input::CanvasCancel(owner.epoch))?;
                contact = None;
                continue;
            }
            if phase == PointerPhase::Up {
                contact = None;
            }
        }
        if idle {
            thread::sleep(Duration::from_millis(2));
        }
    }
    if let Some(owner) = contact {
        send(Input::CanvasCancel(owner.epoch))?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    fn target(epoch: u32) -> Target {
        Target {
            epoch,
            viewport: [1280., 800.],
            access: Arc::new(AtomicBool::new(true)),
        }
    }
    #[test]
    fn down_up_dispatch_during_a_blocked_renderer_and_keep_sample_times() {
        let shared = Arc::new(Mutex::new(Some(target(7))));
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let (tx, rx) = mpsc::sync_channel(128);
        let (source, samples) = mpsc::channel::<Vec<Sample>>();
        let worker = thread::spawn(move || {
            run(
                || Ok(samples.try_recv().unwrap_or_default()),
                shared,
                done,
                tx,
                Surface::new(0).unwrap(),
                Calibration::new([1., 0., 0., 0., 1., 0.]).unwrap(),
            )
        });
        // No renderer credit or graphics call exists on this route. A stalled
        // graphics owner therefore cannot postpone delivery of either edge.
        let at = Instant::now() - Duration::from_millis(35);
        source
            .send(vec![
                Sample {
                    phase: 0,
                    position: [0.2, 0.8],
                    sampled: at,
                },
                Sample {
                    phase: 2,
                    position: [0.2, 0.8],
                    sampled: at + Duration::from_millis(20),
                },
            ])
            .unwrap();
        for (phase, sampled) in [
            (PointerPhase::Down, at),
            (PointerPhase::Up, at + Duration::from_millis(20)),
        ] {
            let Input::CanvasPointer(epoch, e, time) =
                rx.recv_timeout(Duration::from_millis(200)).unwrap()
            else {
                panic!("pointer edge")
            };
            assert_eq!((epoch, e.phase, time), (7, phase, sampled));
        }
        stop.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
    }
    #[test]
    fn mount_replacement_cancels_capture_and_never_redirects_its_release() {
        let shared = Arc::new(Mutex::new(Some(target(1))));
        let updated = shared.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let (tx, rx) = mpsc::sync_channel(128);
        let (source, samples) = mpsc::channel::<Vec<Sample>>();
        let worker = thread::spawn(move || {
            run(
                || Ok(samples.try_recv().unwrap_or_default()),
                shared,
                done,
                tx,
                Surface::new(0).unwrap(),
                Calibration::new([1., 0., 0., 0., 1., 0.]).unwrap(),
            )
        });
        let sample = |phase| {
            vec![Sample {
                phase,
                position: [0.2, 0.8],
                sampled: Instant::now(),
            }]
        };
        source.send(sample(0)).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Input::CanvasPointer(1, ..)
        ));
        *updated.lock().unwrap() = Some(target(2));
        source.send(sample(1)).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Input::CanvasCancel(1)
        ));
        source.send(sample(2)).unwrap();
        source.send(sample(0)).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Input::CanvasPointer(2, ..)
        ));
        updated
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .access
            .store(false, Ordering::Release);
        source.send(sample(2)).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Input::CanvasCancel(2)
        ));
        stop.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
    }
    #[test]
    fn source_failure_and_queue_overload_are_explicit_errors() {
        let shared = Arc::new(Mutex::new(Some(target(1))));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = mpsc::sync_channel(1);
        let surface = Surface::new(0).unwrap();
        let calibration = Calibration::new([1., 0., 0., 0., 1., 0.]).unwrap();
        let error = run(
            || Err("device disconnected".into()),
            shared.clone(),
            stop.clone(),
            tx.clone(),
            surface,
            calibration,
        )
        .unwrap_err();
        assert_eq!(error, "device disconnected");
        let error = run(
            || {
                Ok([0, 2]
                    .map(|phase| Sample {
                        phase,
                        position: [0.2, 0.8],
                        sampled: Instant::now(),
                    })
                    .into())
            },
            shared,
            stop,
            tx,
            surface,
            calibration,
        )
        .unwrap_err();
        assert!(
            error.contains("Touch dispatch queue unavailable"),
            "a lost release must fail the host, not leave a silently stuck key"
        );
    }
}
