// SPDX-License-Identifier: Apache-2.0
use crate::native_worker::{DisplayInfo, Frame, Input, options, state_worker};
use otrios_core::dashboard::{
    display::Surface,
    motion::{Presenter, render_hud},
};
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
use otrios_linux_display::Display;
use std::{
    fs,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
fn percentile(v: &[u64], q: f64) -> u64 {
    v.get(((v.len().saturating_sub(1)) as f64 * q).round() as usize)
        .copied()
        .unwrap_or(0)
}
pub(super) fn run() -> Result<(), String> {
    let mut o = options()?;
    let seconds = o.seconds;
    let frames = o.frames;
    let exit_file = o.exit_file.clone();
    let report = o.report.take();
    let mut capture = o.capture.take();
    let capture_after = o.capture_after;
    let load = o.load;
    let stall = o.stall_ms;
    let surface = Surface::new(o.rotation)?;
    let calibration = o
        .touch_matrix
        .ok_or("Direct input requires the accepted touch calibration")?;
    let rgb565 = match std::env::var("OTRIOS_SCANOUT_FORMAT")
        .as_deref()
        .unwrap_or("xrgb8888")
    {
        "rgb565" => true,
        "xrgb8888" => false,
        _ => return Err("Choose rgb565 or xrgb8888 scanout".into()),
    };
    let touch = std::env::var("OTRIOS_TOUCH_DEVICE")
        .unwrap_or_else(|_| "/dev/input/by-path/platform-2005a000.i2c-event".into());
    let card = std::env::var("OTRIOS_DRM_DEVICE").unwrap_or_else(|_| "/dev/dri/card0".into());
    let mut display = Display::open(&card, &touch, surface.extent(), rgb565)?;
    let initial = display.info();
    println!(
        "Direct DRM/GBM/EGL/GLES2 {}x{} format={} bits={}/{}/{}/{}",
        initial.width,
        initial.height,
        if rgb565 { "RGB565" } else { "XRGB8888" },
        initial.red,
        initial.green,
        initial.blue,
        initial.alpha
    );
    let (input_tx, input_rx) = mpsc::sync_channel(128);
    let (frame_tx, frame_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || state_worker(o, input_rx, frame_tx));
    let send = |e| {
        input_tx
            .try_send(e)
            .map_err(|e| format!("State input queue unavailable: {e}"))
    };
    let (hud_tx, hud_requests) = mpsc::sync_channel::<(u64, String)>(1);
    let (hud_results, hud_rx) = mpsc::sync_channel(1);
    let hud_worker = thread::spawn(move || {
        while let Ok((bucket, label)) = hud_requests.recv() {
            let _ = hud_results.try_send((bucket, render_hud(&label).vertices));
        }
    });
    let started = Instant::now();
    let mut model: Option<Frame> = None;
    let mut presenter = Presenter::default();
    let mut retained = None;
    let mut hud = u64::MAX;
    let mut drawn = 0;
    let mut acked = 0;
    let mut dirty = true;
    let mut compose = Vec::new();
    let mut present = Vec::new();
    let mut flips = Vec::new();
    let result = (|| -> Result<(), String> {
        loop {
            if worker.is_finished() {
                return Err("State worker stopped unexpectedly".into());
            }
            if seconds.is_some_and(|s| started.elapsed() >= Duration::from_secs(s))
                || exit_file
                    .as_ref()
                    .is_some_and(|p| std::path::Path::new(p).exists())
            {
                break;
            }
            for e in display.input()? {
                if e.phase == 3 {
                    send(Input::Cancel)?;
                    continue;
                }
                let phase = match e.phase {
                    0 => PointerPhase::Down,
                    1 => PointerPhase::Move,
                    2 => PointerPhase::Up,
                    _ => return Err("Invalid input phase".into()),
                };
                if let Some(position) = surface.touch(calibration, [e.x, e.y]) {
                    send(Input::Pointer(PointerEvent {
                        id: 1,
                        phase,
                        position,
                    }))?;
                } else {
                    send(Input::Cancel)?;
                }
            }
            while let Ok(next) = frame_rx.try_recv() {
                if model.as_ref().is_some_and(|old| old.screen != next.screen) {
                    presenter = Presenter::default();
                    hud = u64::MAX;
                }
                model = Some(next);
                dirty = true;
            }
            if let Some(f) = &model {
                let animated = matches!(f.screen.as_str(), "animation" | "3d");
                if dirty || animated {
                    let before = Instant::now();
                    if retained
                        .as_ref()
                        .is_none_or(|old| !Arc::ptr_eq(old, &f.vertices))
                    {
                        display.upload(0, &f.vertices)?;
                        retained = Some(f.vertices.clone());
                    }
                    if animated {
                        let now = started.elapsed().as_secs_f64() * 1000.;
                        let mesh = presenter.geometry(f.screen == "3d", f.intent, now);
                        display.upload(1, &mesh.vertices)?;
                        let bucket = (now / 1000.) as u64;
                        if hud != bucket
                            && hud_tx
                                .try_send((bucket, presenter.hud_label(f.updates)))
                                .is_ok()
                        {
                            hud = bucket;
                        }
                        while let Ok((_bucket, vertices)) = hud_rx.try_recv() {
                            display.upload(2, &vertices)?;
                        }
                    }
                    display.draw(surface.projection(), animated)?;
                    if drawn >= capture_after
                        && let Some(path) = capture.take()
                    {
                        let info = display.info();
                        let rgba = display.pixels()?;
                        let mut ppm =
                            format!("P6\n{} {}\n255\n", info.width, info.height).into_bytes();
                        for row in rgba.chunks_exact(info.width as usize * 4).rev() {
                            for pixel in row.chunks_exact(4) {
                                ppm.extend_from_slice(&pixel[..3]);
                            }
                        }
                        fs::write(path, ppm).map_err(|e| e.to_string())?;
                    }
                    if compose.len() < 20000 {
                        compose.push(before.elapsed().as_micros() as u64);
                    }
                    let before = Instant::now();
                    let flip = display.present()?;
                    if present.len() < 20000 {
                        present.push(before.elapsed().as_micros() as u64);
                    }
                    if flip.usec > 0 && flips.len() < 20000 {
                        flips.push((started.elapsed().as_secs_f64(), flip));
                    }
                    if acked != f.serial {
                        let info = display.info();
                        send(Input::Presented(
                            f.clone(),
                            "gles2-direct".into(),
                            DisplayInfo {
                                video: "DRM",
                                rotation: surface.degrees(),
                                output: surface.extent(),
                                touch_calibrated: true,
                                touch_devices: 1,
                                format: if rgb565 { "RGB565" } else { "XRGB8888" },
                                color_bits: [info.red, info.green, info.blue, info.alpha],
                                stride: info.stride,
                            },
                        ))?;
                        acked = f.serial;
                    }
                    drawn += 1;
                    dirty = false;
                    if frames.is_some_and(|n| drawn >= n) {
                        break;
                    }
                } else {
                    thread::sleep(Duration::from_millis(4));
                }
            } else {
                thread::sleep(Duration::from_millis(4));
            }
        }
        Ok(())
    })();
    drop(hud_tx);
    hud_worker
        .join()
        .map_err(|_| "Presentation telemetry worker panicked")?;
    let _ = input_tx.send(Input::Stop);
    let worker_result = worker.join().map_err(|_| "State worker panicked")?;
    result?;
    worker_result?;
    if let Some(path) = report {
        compose.sort_unstable();
        present.sort_unstable();
        let sample: Vec<_> = flips
            .iter()
            .filter(|(s, _)| *s >= 2.)
            .map(|(_, f)| *f)
            .collect();
        let mut intervals: Vec<_> = sample
            .windows(2)
            .map(|w| w[1].usec.saturating_sub(w[0].usec))
            .collect();
        intervals.sort_unstable();
        let missed: u64 = sample
            .windows(2)
            .map(|w| u64::from(w[1].sequence.wrapping_sub(w[0].sequence).saturating_sub(1)))
            .sum();
        let duration = sample
            .first()
            .zip(sample.last())
            .map(|(a, b)| (b.usec - a.usec) as f64 / 1e6)
            .unwrap_or(0.);
        let actual_fps = if duration > 0. {
            sample.len().saturating_sub(1) as f64 / duration
        } else {
            0.
        };
        let info = display.info();
        let cadence: serde_json::Value =
            serde_json::from_str(&presenter.stats()).map_err(|e| e.to_string())?;
        let report = serde_json::json!({"renderer":"gles2-direct","video":"DRM","drawn":drawn,"seconds":started.elapsed().as_secs_f64(),"scanout_format":if rgb565{"RGB565"}else{"XRGB8888"},"stride":info.stride,"color_bits":[info.red,info.green,info.blue,info.alpha],"output":[info.width,info.height],"rotation":surface.degrees(),"cadence":cadence,"compose_submit_p50_us":percentile(&compose,0.5),"compose_submit_p95_us":percentile(&compose,0.95),"compose_submit_p99_us":percentile(&compose,0.99),"compose_submit_max_us":compose.last(),"present_p50_us":percentile(&present,0.5),"present_p95_us":percentile(&present,0.95),"load_requested_hz":load,"worker_stall_ms":stall,"load_updates":model.as_ref().map(|f|f.updates),"received_app_updates":model.as_ref().map(|f|f.received_updates),"screen":model.as_ref().map(|f|&f.screen),"objects":model.as_ref().map(|f|f.intent.count),"kernel_flips":{"events":sample.len(),"seconds":duration,"fps":actual_fps,"missed_vblanks":missed,"p50_us":percentile(&intervals,0.5),"p95_us":percentile(&intervals,0.95),"p99_us":percentile(&intervals,0.99),"max_us":intervals.last(),"crtc":sample.first().map(|f|f.crtc)},"measurement":"kernel page-flip completions after two-second warmup; no tracing"});
        fs::write(path, report.to_string()).map_err(|e| e.to_string())?;
        println!("{report}");
    }
    Ok(())
}
