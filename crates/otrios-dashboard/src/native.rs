// SPDX-License-Identifier: Apache-2.0
//! SDL and the presentation clock stay on the main thread. CRDT, Shell,
//! network reconciliation, static geometry and persistence belong to workers.
use otrios_core::dashboard::{
    Replica, SyncSession,
    motion::{Intent, Presenter, Stress},
    render_scene,
    shell::Shell,
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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
struct HostUpdate {
    document: Vec<u8>,
    presentation: Option<Vec<u8>>,
}
fn update(replica: &mut Replica) -> HostUpdate {
    HostUpdate {
        document: replica.save(),
        presentation: replica.presentation().map(|p| p.bytes().to_vec()),
    }
}
fn gpu_vertices(mesh: &[f32]) -> Vec<Vertex> {
    mesh.chunks_exact(6)
        .map(|v| Vertex {
            position: FPoint::new(v[0], v[1]),
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
#[derive(Clone)]
struct Frame {
    vertices: Arc<Vec<Vertex>>,
    snapshot: String,
    layout: String,
    screen: String,
    keyboard: bool,
    editing: bool,
    online: bool,
    intent: Intent,
    updates: u64,
    received_updates: u64,
    serial: u64,
}
enum Input {
    Pointer(PointerEvent),
    Back,
    Edit(Edit),
    First,
    Cancel,
    Presented(Frame, String),
    Stop,
}
#[derive(Default)]
struct Options {
    url: Option<String>,
    state: Option<String>,
    frames: Option<usize>,
    fullscreen: bool,
    capture: Option<String>,
    capture_after: usize,
    seconds: Option<u64>,
    exit_file: Option<String>,
    ready_file: Option<String>,
    release_file: Option<String>,
    screen: Option<String>,
    report: Option<String>,
    load: u8,
    objects: u16,
    stall_ms: u64,
}
fn options() -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--fullscreen" {
            o.fullscreen = true;
            continue;
        }
        let value = args.next().ok_or("Missing argument value")?;
        match arg.as_str() {
            "--sync" => o.url = Some(value),
            "--state" => o.state = Some(value),
            "--frames" => o.frames = Some(value.parse::<usize>().map_err(|e| e.to_string())?),
            "--capture-after" => {
                o.capture_after = value.parse::<usize>().map_err(|e| e.to_string())?
            }
            "--capture" => o.capture = Some(value),
            "--seconds" => o.seconds = Some(value.parse::<u64>().map_err(|e| e.to_string())?),
            "--exit-file" => o.exit_file = Some(value),
            "--ready-file" => o.ready_file = Some(value),
            "--release-file" => o.release_file = Some(value),
            "--screen" => o.screen = Some(value),
            "--report" => o.report = Some(value),
            "--objects" => o.objects = value.parse::<u16>().map_err(|e| e.to_string())?,
            "--load-hz" => o.load = value.parse::<u8>().map_err(|e| e.to_string())?,
            "--worker-stall-ms" => o.stall_ms = value.parse::<u64>().map_err(|e| e.to_string())?,
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    if o.objects > 64 || (o.objects > 0 && (o.url.is_some() || o.state.is_some())) {
        return Err(
            "--objects is an isolated benchmark override (1..64, no --sync or --state)".into(),
        );
    }
    if o.load > 60 || o.stall_ms > 1000 {
        return Err("Load must be <=60 Hz; stall <=1000 ms".into());
    }
    Ok(o)
}
fn state_worker(
    o: Options,
    input: mpsc::Receiver<Input>,
    frames: mpsc::SyncSender<Frame>,
) -> Result<(), String> {
    let mut replica = match &o.state {
        Some(p) if std::path::Path::new(p).exists() => {
            Replica::load(&fs::read(p).map_err(|e| e.to_string())?)?
        }
        _ => Replica::new()?,
    };
    if let Some(path) = &o.state
        && let Ok(bytes) = fs::read(format!("{path}.presentation.flex"))
        && !bytes.is_empty()
    {
        replica.set_presentation(Some(&bytes))?;
    }
    if o.objects > 0 {
        replica.set_motion(Intent {
            count: o.objects,
            ..replica.motion()?
        })?;
    }
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let (reply_tx, reply_rx) = mpsc::sync_channel::<HostUpdate>(1);
    let stream_ready = Arc::new(AtomicBool::new(false));
    let worker_ready = stream_ready.clone();
    let network_thread = o
        .url
        .map(|url| thread::spawn(move || network(url, rx, reply_tx, o.release_file, worker_ready)));
    let mut shell = Shell::default();
    if let Some(path) = &o.state
        && let Ok(route) = fs::read_to_string(format!("{path}.screen"))
    {
        shell.open_name(route.trim());
    }
    if let Some(screen) = &o.screen {
        shell.open_name(screen);
    }
    shell.set_stress(o.load);
    let started = Instant::now();
    let mut last_sync = Instant::now() - Duration::from_secs(1);
    let mut sent_id = None;
    let mut saved_id = None;
    let mut saved_screen = String::new();
    let mut observed_stream = false;
    let mut dirty = true;
    let mut pending = None;
    let mut serial = 0;
    let mut received_updates = 0;
    let mut scene_cache = None;
    let mut mesh_cache = Arc::new(Vec::new());
    let mut stress = Stress::new()?;
    let mut stall = Instant::now();
    loop {
        if network_thread.as_ref().is_some_and(|h| h.is_finished()) {
            return Err("Live state worker stopped unexpectedly".into());
        }
        let now = started.elapsed().as_millis() as u64;
        for event in input.try_iter() {
            match event {
                Input::Stop => {
                    if let Some(path) = &o.state {
                        save_state(path, &mut replica)?;
                    }
                    return Ok(());
                }
                Input::Pointer(e) => dirty |= shell.pointer(&mut replica, e, now)?,
                Input::Back => dirty |= shell.back(),
                Input::Edit(e) => dirty |= shell.hardware_edit(&e),
                Input::Cancel => {
                    shell.cancel();
                    dirty = true;
                }
                Input::First => {
                    if let Some(id) = shell.scene(&replica)?.controls().first().map(|c| c.node.id) {
                        dirty |= shell.activate(&mut replica, id)?;
                    }
                }
                Input::Presented(frame, renderer) => {
                    if let Some(path) = &o.ready_file {
                        let info = serde_json::json!({"pid":std::process::id(),"snapshot":frame.snapshot,"renderer":renderer,"stream_ready":frame.online,"screen":frame.screen,"keyboard_visible":frame.keyboard,"layout":frame.layout,"release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default()});
                        let tmp = format!("{path}.tmp");
                        fs::write(&tmp, info.to_string())
                            .and_then(|_| fs::rename(tmp, path))
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        let online = stream_ready.load(Ordering::Acquire);
        if online != observed_stream {
            observed_stream = online;
            dirty = true;
        }
        while let Ok(next) = reply_rx.try_recv() {
            let changed = replica.merge(&next.document)?;
            received_updates += u64::from(changed);
            dirty |= changed;
            dirty |= replica.set_presentation(next.presentation.as_deref())?;
        }
        dirty |= shell.tick(now);
        let stressed = stress.tick(now as f64, shell.stress_hz())?;
        dirty |= stressed;
        let stall_ms = o.stall_ms.max(shell.stress_stall_ms());
        if stall_ms > 0 && stall.elapsed() >= Duration::from_secs(1) {
            thread::sleep(Duration::from_millis(stall_ms));
            stall = Instant::now();
        }
        if network_thread.is_some() && last_sync.elapsed() >= Duration::from_millis(100) {
            if sent_id != Some(replica.snapshot().id()) && tx.try_send(replica.save()).is_ok() {
                sent_id = Some(replica.snapshot().id());
            }
            last_sync = Instant::now();
        }
        if dirty {
            let scene = shell.scene(&replica)?;
            if scene_cache
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &scene))
            {
                mesh_cache = Arc::new(gpu_vertices(&render_scene(&scene).vertices));
                scene_cache = Some(scene);
            }
            serial += 1;
            pending = Some(Frame {
                vertices: mesh_cache.clone(),
                snapshot: replica.snapshot().hex_id(),
                layout: replica.snapshot().layout().hex_id(),
                screen: shell.screen_name().into(),
                keyboard: shell.keyboard_visible(),
                editing: shell.editing(),
                online,
                intent: replica.motion()?,
                updates: stress.updates,
                received_updates,
                serial,
            });
            if shell.screen_name() != saved_screen {
                if let Some(path) = &o.state {
                    fs::write(format!("{path}.screen"), shell.screen_name())
                        .map_err(|e| e.to_string())?;
                }
                saved_screen = shell.screen_name().into();
            }
            if saved_id != Some(replica.snapshot().id()) {
                if let Some(p) = &o.state {
                    save_state(p, &mut replica)?;
                    let path = format!("{p}.presentation.flex");
                    let tmp = format!("{path}.tmp");
                    fs::write(
                        &tmp,
                        replica
                            .presentation()
                            .map(|p| p.bytes())
                            .unwrap_or_default(),
                    )
                    .and_then(|_| fs::rename(tmp, path))
                    .map_err(|e| e.to_string())?;
                }
                saved_id = Some(replica.snapshot().id());
            }
            dirty = false;
        }
        if let Some(frame) = pending.take() {
            match frames.try_send(frame) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(frame)) => pending = Some(frame),
                Err(_) => return Ok(()),
            }
        }
        thread::sleep(Duration::from_millis(4));
    }
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
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    sdl2::hint::set("SDL_TOUCH_MOUSE_EVENTS", "1");
    let mut builder = video.window("OTriOS dashboard", 1280, 800);
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
        .set_logical_size(1280, 800)
        .map_err(|e| e.to_string())?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    video.text_input().stop();
    println!(
        "Native video={} renderer={} vsync={}",
        video.current_video_driver(),
        canvas.info().name,
        canvas.info().flags & sdl2::sys::SDL_RendererFlags::SDL_RENDERER_PRESENTVSYNC as u32 != 0
    );
    let texture_creator = canvas.texture_creator();
    let mut static_texture = texture_creator
        .create_texture_target(sdl2::pixels::PixelFormatEnum::RGBA8888, 1280, 800)
        .map_err(|e| e.to_string())?;
    static_texture.set_blend_mode(sdl2::render::BlendMode::None);
    let mut texture_mesh: Option<Arc<Vec<Vertex>>> = None;
    let mut hud_revision = u64::MAX;
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
    let mut acked = 0;
    let mut last_present = Instant::now();
    let mut compose_us = Vec::new();
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
                            position: [x as f32, y as f32],
                        }))?;
                        sdl.mouse().capture(true);
                    }
                    Event::MouseMotion { x, y, .. } if held => {
                        send(Input::Pointer(PointerEvent {
                            id: 0,
                            phase: PointerPhase::Move,
                            position: [x as f32, y as f32],
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
                            position: [x as f32, y as f32],
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
                        )
                    } else {
                        Vec::new()
                    };
                    if texture_mesh
                        .as_ref()
                        .is_none_or(|old| !Arc::ptr_eq(old, &f.vertices))
                        || (animated && hud_revision != presenter.hud_revision())
                    {
                        let mut rendered = Ok(());
                        let hud = gpu_vertices(presenter.hud());
                        canvas
                            .with_texture_canvas(&mut static_texture, |target| {
                                target.set_draw_color(Color::RGB(16, 28, 41));
                                target.clear();
                                rendered = target
                                    .render_geometry(
                                        f.vertices.as_slice(),
                                        None,
                                        sdl2::render::VertexIndices::Sequential,
                                    )
                                    .and_then(|_| {
                                        if animated {
                                            target.render_geometry(
                                                &hud,
                                                None,
                                                sdl2::render::VertexIndices::Sequential,
                                            )
                                        } else {
                                            Ok(())
                                        }
                                    });
                            })
                            .map_err(|e| e.to_string())?;
                        rendered?;
                        hud_revision = presenter.hud_revision();
                        texture_mesh = Some(f.vertices.clone());
                    }
                    if animated {
                        // These demo stages are a solid Canvas between header and HUD.
                        // A tile clear avoids sampling a full-screen backdrop every frame.
                        canvas.set_draw_color(Color::RGB(16, 28, 41));
                        canvas.clear();
                        for region in [
                            sdl2::rect::Rect::new(0, 0, 1280, 256),
                            sdl2::rect::Rect::new(0, 736, 1280, 64),
                        ] {
                            canvas.copy(&static_texture, region, region)?;
                        }
                    } else {
                        canvas.copy(&static_texture, None, None)?;
                    }
                    if !dynamic.is_empty() {
                        canvas.render_geometry(
                            &dynamic,
                            None,
                            sdl2::render::VertexIndices::Sequential,
                        )?;
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
                        send(Input::Presented(f.clone(), canvas.info().name.into()))?;
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
                    if now < deadline {
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
        let info = serde_json::json!({"present_p50_us":pp(0.5),"present_p95_us":pp(0.95),"cadence":stats,"compose_submit_p50_us":p(0.5),"compose_submit_p95_us":p(0.95),"seconds":started.elapsed().as_secs_f64(),"load_requested_hz":load,"worker_stall_ms":stall_ms,"load_updates":model.as_ref().map(|f|f.updates),"received_app_updates":model.as_ref().map(|f|f.received_updates),"screen":model.as_ref().map(|f|&f.screen),"objects":model.as_ref().map(|f|f.intent.count),"renderer":canvas.info().name,"video":video.current_video_driver(),"drawn":drawn,"release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default(),"measurement":"host frame-start cadence; not display scanout latency"});
        fs::write(path, info.to_string()).map_err(|e| e.to_string())?;
        println!("{info}");
    }
    Ok(())
}
fn save_state(path: &str, replica: &mut Replica) -> Result<(), String> {
    use std::io::Write;
    let temp = format!("{path}.tmp");
    let mut f = fs::File::create(&temp).map_err(|e| e.to_string())?;
    f.write_all(&replica.save())
        .and_then(|_| f.sync_all())
        .and_then(|_| fs::rename(temp, path))
        .map_err(|e| e.to_string())
}
fn network(
    url: String,
    rx: mpsc::Receiver<Vec<u8>>,
    reply: mpsc::SyncSender<HostUpdate>,
    release_file: Option<String>,
    stream_ready: Arc<AtomicBool>,
) {
    use tungstenite::{Error, Message, stream::MaybeTlsStream};
    let _ = rustls::crypto::ring::default_provider().install_default();
    let url = url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let url = if let Some(prefix) = url.strip_suffix("/sync") {
        format!("{prefix}/stream")
    } else {
        url
    };
    let mut replica = match Replica::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Sync startup: {e}");
            return;
        }
    };
    let mut pending = None;
    let mut announced_release = String::new();
    loop {
        stream_ready.store(false, Ordering::Release);
        while let Ok(doc) = rx.try_recv() {
            if let Err(e) = replica.merge(&doc) {
                eprintln!("Rejected local sync: {e}");
            }
        }
        let mut socket = match tungstenite::connect(url.as_str()) {
            Ok((s, _)) => s,
            Err(e) => {
                eprintln!("Sync disconnected: {e}");
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        let tcp = match socket.get_mut() {
            MaybeTlsStream::Plain(s) => Some(s),
            MaybeTlsStream::Rustls(s) => Some(&mut s.sock),
            _ => None,
        };
        if let Some(tcp) = tcp {
            let _ = tcp.set_read_timeout(Some(Duration::from_millis(50)));
            let _ = tcp.set_write_timeout(Some(Duration::from_secs(5)));
        }
        let mut session = SyncSession::default();
        eprintln!("Live state stream connected");
        loop {
            loop {
                match rx.try_recv() {
                    Ok(doc) => {
                        if let Err(e) = replica.merge(&doc) {
                            eprintln!("Local sync: {e}");
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if let Some(bytes) = pending.take() {
                match reply.try_send(bytes) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(bytes)) => pending = Some(bytes),
                    Err(_) => return,
                }
            }
            match replica.generate_sync(&mut session) {
                Ok(Some(bytes)) => {
                    if socket.send(Message::Binary(bytes.into())).is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Sync generate: {e}");
                    break;
                }
            }
            match socket.read() {
                Ok(Message::Binary(bytes)) if bytes.starts_with(b"OTML1") => {
                    match replica.set_presentation(if bytes.len() == 5 {
                        None
                    } else {
                        Some(&bytes[5..])
                    }) {
                        Ok(_) => pending = Some(update(&mut replica)),
                        Err(e) => {
                            eprintln!("Rejected monitoring view: {e}");
                            break;
                        }
                    }
                }
                Ok(Message::Binary(bytes)) => match replica.receive_sync(&mut session, &bytes) {
                    Ok(true) => {
                        pending = Some(update(&mut replica));
                        stream_ready.store(true, Ordering::Release);
                    }
                    Ok(false) => {
                        stream_ready.store(true, Ordering::Release);
                    }
                    Err(e) => {
                        eprintln!("Rejected stream: {e}");
                        break;
                    }
                },
                Ok(Message::Text(text)) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                        && value["type"] == "release"
                    {
                        eprintln!("Published code release={}", value["release"]["id"]);
                        let id = value["release"]["id"].as_str().unwrap_or_default();
                        if announced_release != id {
                            if let Some(path) = &release_file {
                                let tmp = format!("{path}.tmp");
                                if let Err(e) = fs::write(&tmp, value["release"].to_string())
                                    .and_then(|_| fs::rename(tmp, path))
                                {
                                    eprintln!("Release notice: {e}");
                                }
                            }
                            announced_release = id.to_string();
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {
                    let _ = socket.flush();
                }
                Err(Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) => {
                    eprintln!("Sync connection: {e}");
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_presentation_consumer_does_not_block_state_or_unbound_the_mailbox() {
        let path = std::env::temp_dir().join(format!(
            "otrios-state-owner-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let options = Options {
            state: Some(path.to_string_lossy().into()),
            screen: Some("3d".into()),
            ..Options::default()
        };
        let (tx, rx) = mpsc::sync_channel(128);
        let (ft, fr) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || state_worker(options, rx, ft));
        // Leave the first view occupying the single frame slot. Navigation and
        // persistence must still complete without a presentation consumer.
        let deadline = Instant::now() + Duration::from_secs(5);
        let route = path.with_file_name(format!(
            "{}.screen",
            path.file_name().unwrap().to_string_lossy()
        ));
        while fs::read_to_string(&route).ok().as_deref() != Some("3d") {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        tx.send(Input::Back).unwrap();
        while fs::read_to_string(&route).ok().as_deref() != Some("home") {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        let first = fr.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first.screen, "3d");
        let latest = fr.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(latest.screen, "home");
        assert!(latest.serial > first.serial);
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        for suffix in ["", ".screen", ".presentation.flex"] {
            let _ = fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
