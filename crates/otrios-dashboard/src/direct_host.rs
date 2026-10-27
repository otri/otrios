// SPDX-License-Identifier: Apache-2.0
use crate::native_worker::{DisplayInfo, Frame, Input, options, state_worker};
use otrios_core::dashboard::{
    display::Surface,
    motion::{Presenter, render_hud_with_font},
};
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
use otrios_linux_display::Display;
#[path = "document_gpu.rs"]
mod document_gpu;
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
    let font = crate::font_resource::load()?;
    o.font = Some(font.clone());
    let seconds = o.seconds;
    let frames = o.frames;
    let exit_file = o.exit_file.clone();
    let report = o.report.take();
    let mut capture = o.capture.take();
    let capture_after = o.capture_after;
    let load = o.load;
    let stall = o.stall_ms;
    let surface = Surface::with_size(
        o.rotation,
        if o.portrait { [800, 1280] } else { [1280, 800] },
    )?;
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
    let atlas = font.atlas();
    display.set_font(
        atlas.curve_width,
        atlas.curve_height,
        &atlas.curves,
        atlas.band_width,
        atlas.band_height,
        &atlas.bands,
    )?;
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
    let (hud_tx, hud_requests) = mpsc::sync_channel::<(u32, u64, String, Option<[f32; 2]>)>(1);
    let (hud_results, hud_rx) = mpsc::sync_channel(1);
    let hud_font = font.clone();
    let hud_worker = thread::spawn(move || {
        while let Ok((epoch, bucket, label, extent)) = hud_requests.recv() {
            let mesh = if let Some(extent) = extent {
                match otrios_core::canvas::text_scroll::hud(&label, extent, &hud_font) {
                    Ok(mesh) => mesh,
                    Err(_) => continue,
                }
            } else {
                render_hud_with_font(&label, &hud_font)
            };
            let _ = hud_results.try_send((epoch, bucket, extent, mesh));
        }
    });
    let started = Instant::now();
    let mut model: Option<Frame> = None;
    let mut presenter = Presenter::default();
    let mut reflow = otrios_core::canvas::reflow::Presenter::default();
    let mut reflow_serial = None;
    let mut retained = None;
    let mut retained_overlay = None;
    let mut hud = u64::MAX;
    let mut scroll_meter = otrios_core::canvas::text_scroll::FrameMeter::default();
    let mut scroll_active = false;
    let mut document_gpu = document_gpu::Cache::default();
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
                    send(Input::CanvasCancel(
                        model.as_ref().map(|f| f.epoch).unwrap_or(0),
                    ))?;
                    continue;
                }
                let phase = match e.phase {
                    0 => PointerPhase::Down,
                    1 => PointerPhase::Move,
                    2 => PointerPhase::Up,
                    _ => return Err("Invalid input phase".into()),
                };
                if let Some(position) = surface.touch_for(
                    calibration,
                    [e.x, e.y],
                    model.as_ref().map(|f| f.viewport).unwrap_or([1280., 800.]),
                ) {
                    send(Input::CanvasPointer(
                        model.as_ref().map(|f| f.epoch).unwrap_or(0),
                        PointerEvent {
                            id: 1,
                            phase,
                            position,
                        },
                    ))?;
                } else {
                    send(Input::CanvasCancel(
                        model.as_ref().map(|f| f.epoch).unwrap_or(0),
                    ))?;
                }
            }
            while let Ok(next) = frame_rx.try_recv() {
                if model
                    .as_ref()
                    .is_some_and(|old| old.viewport != next.viewport)
                {
                    hud = u64::MAX;
                }
                if model.as_ref().is_some_and(|old| {
                    old.epoch != next.epoch
                        || old.ui_ref != next.ui_ref
                        || old.state_ref != next.state_ref
                }) {
                    display.retire_slot(1)?;
                    retained_overlay = None;
                    display.upload_scene(2, &[], &[], &[])?;
                    presenter = Presenter::default();
                    scroll_meter = Default::default();
                    scroll_active = false;
                    hud = u64::MAX;
                    document_gpu.clear(&mut display)?;
                }
                model = Some(next);
                dirty = true;
            }
            if let Some(f) = &model {
                let animated = f.motion_kind.is_some();
                let now = started.elapsed().as_secs_f64() * 1000.;
                if let Some(frame) = &f.reflow
                    && reflow_serial != Some(f.serial)
                {
                    reflow.update(frame.clone(), &font, now)?;
                    reflow_serial = Some(f.serial);
                }
                let flowing = f.reflow.is_some() && reflow.active(now);
                while let Ok((epoch, _bucket, extent, mesh)) = hud_rx.try_recv() {
                    if epoch == f.epoch && extent.is_none_or(|extent| extent == f.viewport) {
                        display.upload_scene(
                            2,
                            &mesh.vertices,
                            &mesh.glyph_vertices,
                            &mesh.draws,
                        )?;
                        dirty = true;
                    }
                }
                if dirty || animated || flowing {
                    let before = Instant::now();
                    if f.reflow.is_some() {
                        let mesh = reflow.mesh(now);
                        display.upload_scene(
                            0,
                            &mesh.vertices,
                            &mesh.glyph_vertices,
                            &mesh.draws,
                        )?;
                        retained = None;
                    } else if retained
                        .as_ref()
                        .is_none_or(|old| !Arc::ptr_eq(old, &f.vertices))
                    {
                        display.upload_scene(0, &f.vertices, &f.glyph_vertices, &f.draws)?;
                        display.text_styles(0, &f.text_styles)?;
                        retained = Some(f.vertices.clone());
                    }
                    if f.reflow.is_none() {
                        display.text_views(0, &f.text_views)?;
                    }
                    if animated {
                        let now = started.elapsed().as_secs_f64() * 1000.;
                        let mesh =
                            presenter.geometry(f.motion_kind.unwrap_or(false), f.intent, now);
                        display.upload(1, &mesh.vertices)?;
                        let bucket = (now / 1000.) as u64;
                        if hud != bucket
                            && hud_tx
                                .try_send((f.epoch, bucket, presenter.hud_label(f.updates), None))
                                .is_ok()
                        {
                            hud = bucket;
                        }
                    }
                    let scrolling = f
                        .resources
                        .text_scroll
                        .as_ref()
                        .map(|s| s.active)
                        .or_else(|| f.resources.text_document.as_ref().map(|s| s.active));
                    let (document_slots, uploading) =
                        document_gpu.update(&mut display, f.document.as_ref())?;
                    if let Some(overlay) = &f.overlay {
                        if retained_overlay
                            .as_ref()
                            .is_none_or(|old| !Arc::ptr_eq(old, &overlay.mesh))
                        {
                            display.upload_scene(
                                1,
                                &overlay.mesh.vertices,
                                &overlay.mesh.glyph_vertices,
                                &overlay.mesh.draws,
                            )?;
                            display.text_styles(1, &overlay.styles)?;
                            retained_overlay = Some(overlay.mesh.clone());
                        }
                        display.text_views(1, &overlay.views)?;
                    }
                    if let Some(active) = scrolling {
                        let bucket = (now / 500.) as u64;
                        if (hud != bucket || scroll_active != active)
                            && hud_tx
                                .try_send((
                                    f.epoch,
                                    bucket,
                                    if uploading || f.document.as_ref().is_some_and(|d| d.loading) {
                                        "Preparing document...".into()
                                    } else {
                                        scroll_meter.label(active)
                                    },
                                    Some(f.viewport),
                                ))
                                .is_ok()
                        {
                            hud = bucket;
                            scroll_active = active;
                        }
                    }
                    let mut slots = vec![0];
                    slots.extend(document_slots);
                    if animated || scrolling.is_some() {
                        slots.extend([1, 2]);
                    }
                    display.draw_slots(surface.projection_for(f.viewport), &slots)?;
                    if drawn >= capture_after
                        && !f.ui_ref.is_empty()
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
                    if scrolling == Some(true) {
                        scroll_meter.record(if flip.usec > 0 {
                            flip.usec as f64 / 1000.
                        } else {
                            started.elapsed().as_secs_f64() * 1000.
                        });
                    }
                    if present.len() < 20000 {
                        present.push(before.elapsed().as_micros() as u64);
                    }
                    if flip.usec > 0 && flips.len() < 20000 {
                        flips.push((started.elapsed().as_secs_f64(), flip));
                    }
                    if acked != f.serial {
                        let info = display.info();
                        send(Input::Presented(
                            Box::new(f.clone()),
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
                    dirty = uploading;
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
        let report = serde_json::json!({"renderer":"gles2-direct","font_renderer":"slug-coverage-cache","alpha":"premultiplied","font_resource":otrios_core::dashboard::font_reference::RESOURCE_SHA256,"font_cache":display.glyph_cache_stats(),"gamma":if display.linear_scene_enabled() {"linear-light-srgb"} else {"srgb-framebuffer-blend"},"video":"DRM","drawn":drawn,"seconds":started.elapsed().as_secs_f64(),"scanout_format":if rgb565{"RGB565"}else{"XRGB8888"},"stride":info.stride,"color_bits":[info.red,info.green,info.blue,info.alpha],"output":[info.width,info.height],"rotation":surface.degrees(),"cadence":cadence,"compose_submit_p50_us":percentile(&compose,0.5),"compose_submit_p95_us":percentile(&compose,0.95),"compose_submit_p99_us":percentile(&compose,0.99),"compose_submit_max_us":compose.last(),"present_p50_us":percentile(&present,0.5),"present_p95_us":percentile(&present,0.95),"load_requested_hz":load,"worker_stall_ms":stall,"load_updates":model.as_ref().map(|f|f.updates),"received_app_updates":model.as_ref().map(|f|f.received_updates),"screen":model.as_ref().map(|f|&f.screen),"objects":model.as_ref().map(|f|f.intent.count),"kernel_flips":{"events":sample.len(),"seconds":duration,"fps":actual_fps,"missed_vblanks":missed,"p50_us":percentile(&intervals,0.5),"p95_us":percentile(&intervals,0.95),"p99_us":percentile(&intervals,0.99),"max_us":intervals.last(),"crtc":sample.first().map(|f|f.crtc)},"measurement":"kernel page-flip completions after two-second warmup; no tracing"});
        fs::write(path, report.to_string()).map_err(|e| e.to_string())?;
        println!("{report}");
    }
    Ok(())
}
