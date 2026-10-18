// SPDX-License-Identifier: Apache-2.0
//! SDL presentation host; shared state and network work live in native_worker.
mod native_worker;
use native_worker::{DisplayInfo, Frame, Input, options, state_worker};
use otrios_core::dashboard::{
    display::{Surface, cached_regions},
    motion::Presenter,
};
use otrios_interfaces::{
    keyboard::Edit,
    scene::{PointerEvent, PointerPhase},
};
use sdl2::{
    event::Event, keyboard::Keycode, mouse::MouseButton, pixels::Color, rect::FPoint,
    render::Vertex,
};
use std::{
    fs,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
fn gpu_vertices(mesh: &[f32], surface: Surface) -> Vec<Vertex> {
    mesh.chunks_exact(6)
        .map(|v| Vertex {
            position: {
                let p = surface.project([v[0], v[1]]);
                FPoint::new(p[0], p[1])
            },
            color: Color::RGBA(
                (v[2] * 255.).round() as u8,
                (v[3] * 255.).round() as u8,
                (v[4] * 255.).round() as u8,
                (v[5] * 255.).round() as u8,
            ),
            tex_coord: FPoint::new(0., 0.),
        })
        .collect()
}
fn display_rect(surface: Surface, region: otrios_interfaces::scene::Rect) -> sdl2::rect::Rect {
    let r = surface.rect(region);
    sdl2::rect::Rect::new(r.x as i32, r.y as i32, r.width as u32, r.height as u32)
}
fn main() -> Result<(), String> {
    let mut o = options()?;
    let seconds = o.seconds;
    let frames = o.frames;
    let exit_file = o.exit_file.clone();
    let report = o.report.take();
    let load = o.load;
    let stall_ms = o.stall_ms;
    let mut capture = o.capture.take();
    let capture_after = o.capture_after;
    let completion_probe = o.completion_probe;
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let direct = video.current_video_driver().eq_ignore_ascii_case("kmsdrm");
    if direct {
        sdl.mouse().show_cursor(false);
    }
    sdl2::hint::set("SDL_TOUCH_MOUSE_EVENTS", if direct { "0" } else { "1" });
    sdl2::hint::set("SDL_RENDER_BATCHING", "1");
    let surface = Surface::new(o.rotation)?;
    let [width, height] = surface.extent();
    let calibration = o.touch_matrix;
    let mut builder = video.window("OTriOS dashboard", width, height);
    builder.position_centered().resizable();
    if o.fullscreen {
        builder.fullscreen_desktop();
    }
    let window = builder.build().map_err(|e| e.to_string())?;
    let preferred = std::env::var("SDL_RENDER_DRIVER").unwrap_or_else(|_| {
        if cfg!(target_os = "linux") {
            "opengles2".into()
        } else {
            "opengl".into()
        }
    });
    let driver = sdl2::render::drivers()
        .position(|d| d.name == preferred)
        .ok_or_else(|| format!("Required GPU renderer {preferred} unavailable"))?;
    let mut canvas = window
        .into_canvas()
        .index(driver as u32)
        .accelerated()
        .target_texture()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;
    canvas
        .set_logical_size(width, height)
        .map_err(|e| e.to_string())?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    video.text_input().stop();
    let output = canvas.output_size()?;
    let touch_devices = sdl2::touch::num_touch_devices();
    if direct && touch_devices < 1 {
        return Err(
            "Direct host has no accessible touch device; check udev/netlink and input permissions"
                .into(),
        );
    }
    if direct && output != (width, height) {
        return Err(format!(
            "Direct output {output:?} does not match rotated canvas {width}x{height}"
        ));
    }
    let display_info = DisplayInfo {
        video: video.current_video_driver(),
        rotation: o.rotation,
        output: [output.0, output.1],
        touch_calibrated: calibration.is_some(),
        touch_devices,
        format: "SDL-managed",
        color_bits: [0; 4],
        stride: 0,
    };
    println!(
        "Native video={} renderer={} vsync={}",
        video.current_video_driver(),
        canvas.info().name,
        canvas.info().flags & sdl2::sys::SDL_RendererFlags::SDL_RENDERER_PRESENTVSYNC as u32 != 0
    );
    let texture_creator = canvas.texture_creator();
    let mut static_texture = texture_creator
        .create_texture_target(sdl2::pixels::PixelFormatEnum::RGBA8888, width, height)
        .map_err(|e| e.to_string())?;
    static_texture.set_blend_mode(sdl2::render::BlendMode::None);
    let hud_bounds = display_rect(
        surface,
        otrios_interfaces::scene::Rect::new(0., 736., 1280., 64.),
    );
    let mut hud_texture = texture_creator
        .create_texture_target(
            sdl2::pixels::PixelFormatEnum::RGBA8888,
            hud_bounds.width(),
            hud_bounds.height(),
        )
        .map_err(|e| e.to_string())?;
    hud_texture.set_blend_mode(sdl2::render::BlendMode::None);
    let mut texture_mesh: Option<Arc<Vec<f32>>> = None;
    let mut hud_revision = u64::MAX;
    let mut hud_regions = Vec::new();
    let (input_tx, input_rx) = mpsc::sync_channel(128);
    let (frame_tx, frame_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || state_worker(o, input_rx, frame_tx));
    let send = |event| {
        input_tx
            .try_send(event)
            .map_err(|e| format!("State input queue unavailable: {e}"))
    };
    let mut events = sdl.event_pump()?;
    let started = Instant::now();
    let mut model: Option<Frame> = None;
    let mut presenter = Presenter::default();
    let mut dirty = true;
    let mut drawn = 0;
    let mut held = false;
    let mut finger: Option<(i64, i64)> = None;
    let mut acked = 0;
    let mut last_present = Instant::now();
    let mut compose_us = Vec::new();
    let mut completion_us = Vec::new();
    let mut copy_pixels = 0u64;
    let mut present_us = Vec::new();
    let result = (|| -> Result<(), String> {
        'running: loop {
            if worker.is_finished() {
                return Err("State worker stopped unexpectedly".into());
            }
            if exit_file
                .as_ref()
                .is_some_and(|p| std::path::Path::new(p).exists())
                || seconds.is_some_and(|n| started.elapsed() >= Duration::from_secs(n))
            {
                break;
            }
            for event in events.poll_iter() {
                if direct {
                    let touch = match &event {
                        Event::FingerDown {
                            touch_id,
                            finger_id,
                            x,
                            y,
                            ..
                        } if finger.is_none() => {
                            finger = Some((*touch_id, *finger_id));
                            Some((PointerPhase::Down, [*x, *y]))
                        }
                        Event::FingerMotion {
                            touch_id,
                            finger_id,
                            x,
                            y,
                            ..
                        } if finger == Some((*touch_id, *finger_id)) => {
                            Some((PointerPhase::Move, [*x, *y]))
                        }
                        Event::FingerUp {
                            touch_id,
                            finger_id,
                            x,
                            y,
                            ..
                        } if finger == Some((*touch_id, *finger_id)) => {
                            finger = None;
                            Some((PointerPhase::Up, [*x, *y]))
                        }
                        _ => None,
                    };
                    if let Some((phase, p)) = touch {
                        if let Some(position) = calibration.and_then(|m| surface.touch(m, p)) {
                            send(Input::Pointer(PointerEvent {
                                id: 1,
                                phase,
                                position,
                            }))?;
                        } else {
                            finger = None;
                            send(Input::Cancel)?;
                        }
                        continue;
                    }
                }
                let editing = model.as_ref().is_some_and(|f| f.editing);
                match event {
                    Event::Quit { .. } => break 'running,
                    Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => send(Input::Back)?,
                    Event::TextInput { text, .. } if editing => {
                        send(Input::Edit(Edit::Commit(text)))?
                    }
                    Event::KeyDown {
                        keycode: Some(key),
                        keymod,
                        ..
                    } if editing => {
                        let shift = keymod.intersects(
                            sdl2::keyboard::Mod::LSHIFTMOD | sdl2::keyboard::Mod::RSHIFTMOD,
                        );
                        let shortcut = keymod.intersects(
                            sdl2::keyboard::Mod::LCTRLMOD
                                | sdl2::keyboard::Mod::RCTRLMOD
                                | sdl2::keyboard::Mod::LGUIMOD
                                | sdl2::keyboard::Mod::RGUIMOD,
                        );
                        let edit = match key {
                            Keycode::Backspace => Some(Edit::Backspace),
                            Keycode::Left => Some(Edit::Left { select: shift }),
                            Keycode::Right => Some(Edit::Right { select: shift }),
                            Keycode::Tab => Some(Edit::Tab { backward: shift }),
                            Keycode::Return => Some(Edit::Enter),
                            Keycode::A if shortcut => Some(Edit::SelectAll),
                            _ => None,
                        };
                        if let Some(edit) = edit {
                            send(Input::Edit(edit))?;
                        } else if video.current_video_driver() == "wayland" && !shortcut {
                            let name = if key == Keycode::Space {
                                " ".into()
                            } else {
                                key.name()
                            };
                            if let Some(text) = otrios_core::dashboard::keyboard::ascii_key(
                                &name,
                                shift,
                                keymod.contains(sdl2::keyboard::Mod::CAPSMOD),
                            ) {
                                send(Input::Edit(Edit::Commit(text)))?;
                            }
                        }
                    }
                    Event::MouseButtonDown {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        held = true;
                        send(Input::Pointer(PointerEvent {
                            id: 0,
                            phase: PointerPhase::Down,
                            position: surface.unproject([x as f32, y as f32]),
                        }))?;
                        sdl.mouse().capture(true);
                    }
                    Event::MouseMotion { x, y, .. } if held => {
                        send(Input::Pointer(PointerEvent {
                            id: 0,
                            phase: PointerPhase::Move,
                            position: surface.unproject([x as f32, y as f32]),
                        }))?;
                    }
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        held = false;
                        send(Input::Pointer(PointerEvent {
                            id: 0,
                            phase: PointerPhase::Up,
                            position: surface.unproject([x as f32, y as f32]),
                        }))?;
                        sdl.mouse().capture(false);
                    }
                    Event::KeyDown {
                        keycode: Some(Keycode::Space),
                        repeat: false,
                        ..
                    } => send(Input::First)?,
                    Event::Window {
                        win_event: sdl2::event::WindowEvent::FocusLost,
                        ..
                    } => {
                        held = false;
                        finger = None;
                        sdl.mouse().capture(false);
                        send(Input::Cancel)?;
                    }
                    Event::Window { .. } => dirty = true,
                    _ => {}
                }
            }
            while let Ok(next) = frame_rx.try_recv() {
                if model.as_ref().is_some_and(|old| old.screen != next.screen) {
                    presenter = Presenter::default();
                }
                model = Some(next);
                dirty = true;
            }
            let animated = model
                .as_ref()
                .is_some_and(|f| matches!(f.screen.as_str(), "animation" | "3d"));
            if let Some(f) = &model {
                if video.current_video_driver() != "wayland" {
                    if f.editing {
                        if !video.text_input().is_active() {
                            video.text_input().start();
                        }
                    } else if video.text_input().is_active() {
                        video.text_input().stop();
                    }
                }
                if dirty || animated {
                    let before = Instant::now();
                    let dynamic = if animated {
                        gpu_vertices(
                            &presenter
                                .mesh(
                                    f.screen == "3d",
                                    f.intent,
                                    started.elapsed().as_secs_f64() * 1000.,
                                    f.updates,
                                )
                                .vertices,
                            surface,
                        )
                    } else {
                        Vec::new()
                    };
                    if texture_mesh
                        .as_ref()
                        .is_none_or(|old| !Arc::ptr_eq(old, &f.vertices))
                    {
                        let static_vertices = gpu_vertices(&f.vertices, surface);
                        let mut rendered = Ok(());
                        canvas
                            .with_texture_canvas(&mut static_texture, |target| {
                                target.set_draw_color(Color::RGB(16, 28, 41));
                                target.clear();
                                rendered = target.render_geometry(
                                    &static_vertices,
                                    None,
                                    sdl2::render::VertexIndices::Sequential,
                                );
                            })
                            .map_err(|e| e.to_string())?;
                        rendered?;
                        texture_mesh = Some(f.vertices.clone());
                    }
                    if animated && hud_revision != presenter.hud_revision() {
                        let mut hud_mesh = gpu_vertices(presenter.hud(), surface);
                        for v in &mut hud_mesh {
                            v.position.x -= hud_bounds.x() as f32;
                            v.position.y -= hud_bounds.y() as f32;
                        }
                        hud_regions = cached_regions(
                            presenter.hud(),
                            otrios_interfaces::scene::Rect::new(0., 736., 1280., 64.),
                            [16, 28, 41, 255],
                        )
                        .into_iter()
                        .map(|r| display_rect(surface, r))
                        .collect();
                        let mut rendered = Ok(());
                        canvas
                            .with_texture_canvas(&mut hud_texture, |target| {
                                target.set_draw_color(Color::RGB(16, 28, 41));
                                target.clear();
                                rendered = target.render_geometry(
                                    &hud_mesh,
                                    None,
                                    sdl2::render::VertexIndices::Sequential,
                                );
                            })
                            .map_err(|e| e.to_string())?;
                        rendered?;
                        hud_revision = presenter.hud_revision();
                    }
                    if animated {
                        // These demo stages are a solid Canvas between header and HUD.
                        // A tile clear avoids sampling a full-screen backdrop every frame.
                        canvas.set_draw_color(Color::RGB(16, 28, 41));
                        canvas.clear();
                        copy_pixels = 0;
                        for logical in f.copy_regions.iter() {
                            let r = display_rect(surface, *logical);
                            canvas.copy(&static_texture, r, r)?;
                            copy_pixels += u64::from(r.width()) * u64::from(r.height());
                        }
                        for r in &hud_regions {
                            let source = sdl2::rect::Rect::new(
                                r.x() - hud_bounds.x(),
                                r.y() - hud_bounds.y(),
                                r.width(),
                                r.height(),
                            );
                            canvas.copy(&hud_texture, source, *r)?;
                            copy_pixels += u64::from(r.width()) * u64::from(r.height());
                        }
                    } else {
                        canvas.copy(&static_texture, None, None)?;
                    }
                    if !dynamic.is_empty() {
                        // Opaque primitives need no destination read/blend. Keep
                        // source-over behavior available for translucent meshes.
                        canvas.set_blend_mode(if dynamic.iter().all(|v| v.color.a == 255) {
                            sdl2::render::BlendMode::None
                        } else {
                            sdl2::render::BlendMode::Blend
                        });
                        canvas.render_geometry(
                            &dynamic,
                            None,
                            sdl2::render::VertexIndices::Sequential,
                        )?;
                        canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
                    }
                    if completion_probe {
                        // Diagnostic only: force GPU completion through SDL's
                        // safe readback API, separating GPU execution from the
                        // later GBM buffer/flip wait. Never used in a live room.
                        let before_completion = Instant::now();
                        canvas.read_pixels(
                            sdl2::rect::Rect::new(0, 0, 1, 1),
                            sdl2::pixels::PixelFormatEnum::RGBA8888,
                        )?;
                        if completion_us.len() < 3600 {
                            completion_us.push(before_completion.elapsed().as_micros() as u64);
                        }
                    }
                    if drawn >= capture_after
                        && let Some(path) = capture.take()
                    {
                        let (w, h) = canvas.output_size()?;
                        let rgb = canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::RGB24)?;
                        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
                        ppm.extend(rgb);
                        fs::write(path, ppm).map_err(|e| e.to_string())?;
                    }
                    if compose_us.len() < 3600 {
                        compose_us.push(before.elapsed().as_micros() as u64);
                    }
                    let before_present = Instant::now();
                    canvas.present();
                    if present_us.len() < 3600 {
                        present_us.push(before_present.elapsed().as_micros() as u64);
                    }
                    if f.serial != acked {
                        send(Input::Presented(
                            Box::new(f.clone()),
                            canvas.info().name.into(),
                            display_info,
                        ))?;
                        acked = f.serial;
                    }
                    dirty = false;
                    drawn += 1;
                    if frames.is_some_and(|n| drawn >= n) {
                        break;
                    }
                    // SDL/Wayland may ignore swap interval. Cap a fast return at 60 Hz;
                    // never add a fixed sleep after an already-blocking vsync present.
                    let deadline = last_present + Duration::from_nanos(16_666_667);
                    let now = Instant::now();
                    if !direct && now < deadline {
                        thread::sleep(deadline - now);
                    }
                    last_present = Instant::now();
                } else {
                    thread::sleep(Duration::from_millis(4));
                }
            } else {
                thread::sleep(Duration::from_millis(4));
            }
        }
        Ok(())
    })();
    let _ = input_tx.send(Input::Stop);
    let worker_result = worker.join().map_err(|_| "State worker panicked")?;
    result?;
    worker_result?;
    if let Some(path) = report {
        let stats: serde_json::Value =
            serde_json::from_str(&presenter.stats()).map_err(|e| e.to_string())?;
        compose_us.sort_unstable();
        let p = |fraction: f64| {
            compose_us
                .get(((compose_us.len().saturating_sub(1)) as f64 * fraction).round() as usize)
                .copied()
                .unwrap_or(0)
        };
        present_us.sort_unstable();
        let pp = |f: f64| {
            present_us
                .get(((present_us.len().saturating_sub(1)) as f64 * f).round() as usize)
                .copied()
                .unwrap_or(0)
        };
        completion_us.sort_unstable();
        let cp = |f: f64| {
            completion_us
                .get(((completion_us.len().saturating_sub(1)) as f64 * f).round() as usize)
                .copied()
                .unwrap_or(0)
        };
        let info = serde_json::json!({"copy_pixels_per_frame":copy_pixels,"completion_probe":completion_probe,"completion_p50_us":cp(0.5),"completion_p95_us":cp(0.95),"rotation":display_info.rotation,"output":display_info.output,"touch_calibrated":display_info.touch_calibrated,"touch_devices":display_info.touch_devices,"present_p50_us":pp(0.5),"present_p95_us":pp(0.95),"cadence":stats,"compose_submit_p50_us":p(0.5),"compose_submit_p95_us":p(0.95),"seconds":started.elapsed().as_secs_f64(),"load_requested_hz":load,"worker_stall_ms":stall_ms,"load_updates":model.as_ref().map(|f|f.updates),"received_app_updates":model.as_ref().map(|f|f.received_updates),"screen":model.as_ref().map(|f|&f.screen),"objects":model.as_ref().map(|f|f.intent.count),"renderer":canvas.info().name,"video":video.current_video_driver(),"drawn":drawn,"release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default(),"measurement":"host frame-start cadence; not display scanout latency"});
        fs::write(path, info.to_string()).map_err(|e| e.to_string())?;
        println!("{info}");
    }
    Ok(())
}
