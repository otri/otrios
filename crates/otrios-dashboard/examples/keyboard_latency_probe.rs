// SPDX-License-Identifier: Apache-2.0
//! Timed synthetic taps through the production editor/composer/GLES2 path.
//! No authority connection or shared edits. Run under an exclusive-display
//! controller that restores the native service and console on every exit.
#[cfg(target_os = "linux")]
#[path = "../src/document_gpu.rs"]
mod document_gpu;
#[cfg(target_os = "linux")]
mod linux {
    use super::document_gpu;
    use otrios_core::{
        canvas::{
            Client, decode_catalog,
            presentation::{Composer, Demand, Frame, Layer},
        },
        dashboard::{display::Surface, font::Font, font_reference},
    };
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    use otrios_linux_display::Display;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, Instant},
    };

    fn percentile(values: &mut [u64], q: f64) -> u64 {
        values.sort_unstable();
        values
            .get(((values.len().saturating_sub(1)) as f64 * q).round() as usize)
            .copied()
            .unwrap_or(0)
    }
    fn value(client: &Client) -> Result<String, String> {
        client
            .text_inputs()?
            .into_iter()
            .find(|i| i.focused)
            .map(|i| i.value)
            .ok_or("No focused field".into())
    }
    struct Paint {
        serial: u64,
        frame: Frame,
        text: String,
        processed: usize,
    }
    pub fn run() -> Result<(), String> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let apps = std::path::Path::new(
            args.first()
                .ok_or("Supply packaged apps directory, rotation, optional baseline")?,
        );
        let rotation = args
            .get(1)
            .map_or(Ok(90), |s| s.parse::<u16>().map_err(|e| e.to_string()))?;
        let baseline = args.get(2).is_some_and(|s| s == "baseline");
        let catalog =
            decode_catalog(&std::fs::read(apps.join("catalog.flex")).map_err(|e| e.to_string())?)?;
        let entry = catalog
            .apps
            .iter()
            .find(|a| a.app.id == "brass-relay")
            .ok_or("Brass Relay absent")?;
        let ui = std::fs::read(apps.join("brass-relay/ui.flex")).map_err(|e| e.to_string())?;
        let font = Arc::new(Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            font_reference::RESOURCE_SHA256,
        )?);
        let mut client = Client::default();
        client.set_chat_native(true);
        client
            .admit_catalog(&std::fs::read(apps.join("catalog.flex")).map_err(|e| e.to_string())?)?;
        client.mount_local(&ui, &entry.app.state_ref, &[], &[])?;
        client.set_reduced_motion(true);
        client.set_font(font.clone());
        if rotation == 0 {
            client.set_viewport([800., 1280.])?;
        }
        client.show_keyboard()?;
        let scene = client.scene()?;
        let expected = "qwertyuiopasdfghjklzxcvbnmqwertyuiopasdfghjklzxcvbnm".to_string();
        let points: Vec<_> = expected
            .chars()
            .map(|ch| {
                let key = scene
                    .controls()
                    .into_iter()
                    .find(|c| {
                        c.node
                            .semantics
                            .as_ref()
                            .is_some_and(|s| s.label == ch.to_string())
                    })
                    .ok_or("Missing key")?;
                Ok([
                    key.origin[0] + key.node.frame.width * key.scale / 2.,
                    key.origin[1] + key.node.frame.height * key.scale / 2.,
                ])
            })
            .collect::<Result<_, String>>()?;
        let surface = Surface::with_size(
            rotation,
            if rotation == 0 {
                [800, 1280]
            } else {
                [1280, 800]
            },
        )?;
        let mut display = Display::open(
            "/dev/dri/card0",
            "/dev/input/by-path/platform-2005a000.i2c-event",
            surface.extent(),
            true,
        )?;
        let atlas = font.atlas();
        display.set_font(
            atlas.curve_width,
            atlas.curve_height,
            &atlas.curves,
            atlas.band_width,
            atlas.band_height,
            &atlas.bands,
        )?;
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(128);
        let (frames, paints) = mpsc::sync_channel(1);
        let (ack, acks) = mpsc::sync_channel(1);
        let started = Instant::now();
        let source_ready = ready.clone();
        let source_stop = stop.clone();
        let source = thread::spawn(move || -> Result<Vec<Instant>, String> {
            while !source_ready.load(Ordering::Acquire) {
                if source_stop.load(Ordering::Acquire) {
                    return Err("Stopped before warmup".into());
                }
                thread::sleep(Duration::from_millis(2));
            }
            let first = Instant::now() + Duration::from_millis(100);
            let mut times = vec![];
            for (i, position) in points.into_iter().enumerate() {
                for (edge, phase) in [PointerPhase::Down, PointerPhase::Up]
                    .into_iter()
                    .enumerate()
                {
                    let at = first + Duration::from_millis((i as u64 * 120) + edge as u64 * 60);
                    while Instant::now() < at {
                        if source_stop.load(Ordering::Acquire) {
                            return Err("Stopped during typing".into());
                        }
                        thread::sleep(
                            at.saturating_duration_since(Instant::now())
                                .min(Duration::from_millis(2)),
                        );
                    }
                    times.push(at);
                    tx.try_send((
                        PointerEvent {
                            id: 1,
                            phase,
                            position,
                        },
                        at,
                    ))
                    .map_err(|e| e.to_string())?;
                }
            }
            Ok(times)
        });
        let owner_stop = stop.clone();
        let owner = thread::spawn(move || -> Result<_, String> {
            let mut demand = Demand::default();
            demand.request();
            let mut composer = Composer::default();
            let mut dispatch = vec![];
            let mut compose = vec![];
            let mut processed = 0;
            while !owner_stop.load(Ordering::Acquire) {
                while let Ok(serial) = acks.try_recv() {
                    demand.presented(serial);
                }
                while let Ok((event, sampled)) = rx.try_recv() {
                    dispatch.push(sampled.elapsed().as_micros() as u64);
                    client.pointer_at(
                        client.epoch(),
                        event,
                        sampled.duration_since(started).as_millis() as u64,
                    )?;
                    processed += 1;
                    demand.request();
                }
                if client.tick(started.elapsed().as_millis() as u64, 0)? {
                    demand.request();
                }
                if let Some(serial) = demand.begin() {
                    let before = Instant::now();
                    let frame = composer.compose(&mut client, &font)?.clone();
                    compose.push(before.elapsed().as_micros() as u64);
                    frames
                        .send(Paint {
                            serial,
                            frame,
                            text: value(&client)?,
                            processed,
                        })
                        .map_err(|e| e.to_string())?;
                }
                thread::sleep(Duration::from_millis(2));
            }
            Ok((value(&client)?, processed, dispatch, compose))
        });
        let mut base = None;
        let mut overlay = None;
        let mut cache = document_gpu::Cache::default();
        let mut presented = vec![];
        let mut feedback = 0;
        let mut draw = vec![];
        let mut count = 0;
        let mut uploads = 0;
        let mut initial_pixels = None;
        let mut feedback_pixels = false;
        let mut kernel_flips = 0;
        let result = (|| -> Result<(), String> {
            while started.elapsed() < Duration::from_secs(30) {
                let paint = paints
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|e| e.to_string())?;
                let f = &paint.frame;
                let before = Instant::now();
                if base.as_ref().is_none_or(|b| !Arc::ptr_eq(b, &f.base.mesh)) {
                    display.upload_scene(
                        0,
                        &f.base.mesh.vertices,
                        &f.base.mesh.glyph_vertices,
                        &f.base.mesh.draws,
                    )?;
                    display.text_styles(0, &f.base.styles)?;
                    base = Some(f.base.mesh.clone());
                    uploads += 1;
                }
                display.text_views(0, &f.base.views)?;
                if let Some(o) = &f.overlay {
                    if overlay.as_ref().is_none_or(|b| !Arc::ptr_eq(b, &o.mesh)) {
                        display.upload_scene(
                            1,
                            &o.mesh.vertices,
                            &o.mesh.glyph_vertices,
                            &o.mesh.draws,
                        )?;
                        display.text_styles(1, &o.styles)?;
                        overlay = Some(o.mesh.clone());
                        uploads += 1;
                    }
                    display.text_views(1, &o.views)?;
                }
                display.upload(2, &f.effects)?;
                let (document, _) = cache.update(&mut display, f.document.as_ref())?;
                let mut slots = vec![];
                for layer in &f.order {
                    match layer {
                        Layer::Base => slots.push(0),
                        Layer::Overlay => slots.push(1),
                        Layer::Document => slots.extend(document.iter().copied()),
                        Layer::Effects => slots.push(2),
                    }
                }
                display.draw_slots(surface.projection_for(f.viewport), &slots)?;
                // The first down is read back too; reported maxima include this
                // instrumented sample and are not steady-state latency.
                if count == 0 {
                    initial_pixels = Some(display.pixels()?);
                }
                if paint.processed == 1 && !f.effects.is_empty() {
                    let pixels = display.pixels()?;
                    feedback_pixels = initial_pixels.as_ref().is_some_and(|p| *p != pixels);
                }
                if display.present()?.usec > 0 {
                    kernel_flips += 1;
                }
                if count > 0 {
                    draw.push(before.elapsed().as_micros() as u64);
                }
                count += 1;
                if !f.effects.is_empty() {
                    feedback += 1;
                }
                presented.push((paint.text.len(), Instant::now()));
                ack.send(paint.serial).map_err(|e| e.to_string())?;
                ready.store(true, Ordering::Release);
                if paint.text == expected && f.effects.is_empty() {
                    break;
                }
            }
            Ok(())
        })();
        stop.store(true, Ordering::Release);
        drop(paints);
        drop(ack);
        let times = source.join().map_err(|_| "Source panic")??;
        let (text, processed, mut dispatch, mut compose) =
            owner.join().map_err(|_| "Owner panic")??;
        result?;
        if text != expected || processed != expected.len() * 2 {
            return Err(format!(
                "Dropped typing: {} characters, {processed} edges",
                text.len()
            ));
        }
        if kernel_flips == 0 {
            return Err("No kernel page-flip completions".into());
        }
        if !baseline && (!feedback_pixels || feedback == 0) {
            return Err("Missing depressed-key pixels".into());
        }
        let mut latency: Vec<_> = (1..=expected.len())
            .map(|n| {
                presented
                    .iter()
                    .find(|(length, _)| *length >= n)
                    .map(|(_, at)| {
                        at.saturating_duration_since(times[n * 2 - 1]).as_micros() as u64
                    })
                    .ok_or_else(|| "Unpresented character".to_string())
            })
            .collect::<Result<_, String>>()?;
        println!(
            "{}",
            serde_json::json!({"synthetic":true,"kernel_flips":kernel_flips,"rotation":rotation,"characters":text.len(),"edges":processed,"tap_ms":60,"keys_per_second":8.333333,"frames":count,"feedback_frames":feedback,"feedback_pixels":feedback_pixels,"scene_uploads":uploads,"dispatch_p95_us":percentile(&mut dispatch,0.95),"compose_p95_us":percentile(&mut compose,0.95),"draw_present_p95_us":percentile(&mut draw,0.95),"release_to_present_p50_us":percentile(&mut latency,0.5),"release_to_present_p95_us":percentile(&mut latency,0.95),"release_to_present_max_us":latency.iter().max()})
        );
        Ok(())
    }
}
fn main() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Linux GLES2 required".into())
    }
}
