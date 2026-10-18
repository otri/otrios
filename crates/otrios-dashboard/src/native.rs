// SPDX-License-Identifier: Apache-2.0
//! Native GPU host: SDL owns the window/input, GLES2 draws the shared triangle list.
use otrios_core::dashboard::{Replica, SyncSession, render_scene, shell::Shell};
use otrios_interfaces::keyboard::Edit;
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
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
fn main() -> Result<(), String> {
    let mut url = None;
    let mut state = None;
    let mut frames = None;
    let mut fullscreen = false;
    let mut capture = None;
    let mut seconds = None;
    let mut exit_file: Option<String> = None;
    let mut ready_file: Option<String> = None;
    let mut release_file: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str(){
        "--sync"=>url=Some(args.next().ok_or("Missing sync URL")?),
        "--state"=>state=Some(args.next().ok_or("Missing state file")?),
        "--frames"=>frames=Some(args.next().ok_or("Missing frames")?.parse::<usize>().map_err(|e|e.to_string())?),
        "--fullscreen"=>fullscreen=true,
        "--capture"=>capture=Some(args.next().ok_or("Missing capture path")?),
        "--exit-file"=>exit_file=Some(args.next().ok_or("Missing exit file")?),
        "--ready-file"=>ready_file=Some(args.next().ok_or("Missing ready file")?),
        "--release-file"=>release_file=Some(args.next().ok_or("Missing release notice file")?),
        "--seconds"=>seconds=Some(args.next().ok_or("Missing duration")?.parse::<u64>().map_err(|e|e.to_string())?),
        _=>return Err("Usage: dashboard-native [--sync https://HOST/stream] [--state FILE] [--fullscreen] [--frames N] [--seconds N] [--capture FILE.ppm] [--exit-file FILE] [--ready-file FILE] [--release-file FILE]".into())
    }
    }
    let mut replica = match &state {
        Some(p) if std::path::Path::new(p).exists() => {
            Replica::load(&fs::read(p).map_err(|e| e.to_string())?)?
        }
        _ => Replica::new()?,
    };
    if let Some(path) = &state
        && let Ok(bytes) = fs::read(format!("{path}.presentation.flex"))
        && !bytes.is_empty()
    {
        replica.set_presentation(Some(&bytes))?;
    }
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let (reply_tx, reply_rx) = mpsc::sync_channel::<HostUpdate>(1);
    let stream_ready = Arc::new(AtomicBool::new(false));
    let worker_ready = stream_ready.clone();
    let network_thread = url
        .map(|url| thread::spawn(move || network(url, rx, reply_tx, release_file, worker_ready)));
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    sdl2::hint::set("SDL_TOUCH_MOUSE_EVENTS", "1");
    let mut builder = video.window("OTriOS dashboard", 1280, 800);
    builder.position_centered().resizable();
    if fullscreen {
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
        .ok_or_else(|| {
            format!("Required GPU renderer {preferred} is unavailable; refusing software fallback")
        })?;
    let mut canvas = window
        .into_canvas()
        .index(driver as u32)
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;
    println!(
        "Native video={} renderer={} snapshot={}",
        video.current_video_driver(),
        canvas.info().name,
        replica.snapshot().hex_id()
    );
    canvas
        .set_logical_size(1280, 800)
        .map_err(|e| e.to_string())?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    // The core OSK owns this surface; do not activate a second compositor input panel.
    video.text_input().stop();
    let mut events = sdl.event_pump()?;
    let mut dirty = true;
    let mut shell = Shell::default();
    if let Some(path) = &state
        && let Ok(route) = fs::read_to_string(format!("{path}.screen"))
    {
        shell.open_name(route.trim());
    }
    let mut saved_screen = String::new();
    let mut drawn = 0;
    let mut last_sync = Instant::now() - Duration::from_secs(1);
    let started = Instant::now();
    let mut saved_id = None;
    let mut observed_stream = false;
    'running: loop {
        if network_thread.as_ref().is_some_and(|h| h.is_finished()) {
            return Err("Live state worker stopped unexpectedly".into());
        }
        let online = stream_ready.load(Ordering::Acquire);
        if online != observed_stream {
            observed_stream = online;
            dirty = true;
        }
        if exit_file
            .as_ref()
            .is_some_and(|p| std::path::Path::new(p).exists())
        {
            if let Some(p) = &state {
                save_state(p, &mut replica)?;
            }
            break;
        }
        if seconds.is_some_and(|n| started.elapsed() >= Duration::from_secs(n)) {
            break;
        }
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => break 'running,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => dirty |= shell.back(),
                Event::TextInput { text, .. } if shell.editing() => {
                    dirty |= shell.hardware_edit(&Edit::Commit(text))
                }
                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } if shell.editing() => {
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
                        dirty |= shell.hardware_edit(&edit);
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
                            dirty |= shell.hardware_edit(&Edit::Commit(text));
                        }
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    dirty |= shell.pointer(
                        &mut replica,
                        PointerEvent {
                            id: 0,
                            phase: PointerPhase::Down,
                            position: [x as f32, y as f32],
                        },
                        started.elapsed().as_millis() as u64,
                    )?;
                    sdl.mouse().capture(true);
                }
                Event::MouseMotion { x, y, .. } => {
                    dirty |= shell.pointer(
                        &mut replica,
                        PointerEvent {
                            id: 0,
                            phase: PointerPhase::Move,
                            position: [x as f32, y as f32],
                        },
                        started.elapsed().as_millis() as u64,
                    )?;
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    dirty |= shell.pointer(
                        &mut replica,
                        PointerEvent {
                            id: 0,
                            phase: PointerPhase::Up,
                            position: [x as f32, y as f32],
                        },
                        started.elapsed().as_millis() as u64,
                    )?;
                    sdl.mouse().capture(false);
                    last_sync = Instant::now() - Duration::from_secs(1);
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    repeat: false,
                    ..
                } => {
                    if let Some(id) = shell.scene(&replica)?.controls().first().map(|c| c.node.id) {
                        dirty |= shell.activate(&mut replica, id)?;
                        last_sync = Instant::now() - Duration::from_secs(1);
                    }
                }
                Event::Window {
                    win_event: sdl2::event::WindowEvent::FocusLost,
                    ..
                } => {
                    sdl.mouse().capture(false);
                    shell.cancel();
                    dirty = true;
                }
                Event::Window { .. } => dirty = true,
                _ => {}
            }
        }
        while let Ok(next) = reply_rx.try_recv() {
            match replica.merge(&next.document).and_then(|changed| {
                replica
                    .set_presentation(next.presentation.as_deref())
                    .map(|view| changed || view)
            }) {
                Ok(changed) => dirty |= changed,
                Err(e) => eprintln!("Rejected sync: {e}"),
            }
        }
        if last_sync.elapsed() >= Duration::from_secs(1) {
            let _ = tx.try_send(replica.save());
            last_sync = Instant::now();
        }
        dirty |= shell.tick(started.elapsed().as_millis() as u64);
        if video.current_video_driver() != "wayland" {
            if shell.editing() {
                if !video.text_input().is_active() {
                    video.text_input().start();
                }
            } else if video.text_input().is_active() {
                video.text_input().stop();
            }
        }
        if shell.screen_name() != saved_screen {
            if let Some(path) = &state {
                let path = format!("{path}.screen");
                let tmp = format!("{path}.tmp");
                fs::write(&tmp, shell.screen_name())
                    .and_then(|_| fs::rename(tmp, path))
                    .map_err(|e| e.to_string())?;
            }
            saved_screen = shell.screen_name().into();
        }
        if dirty {
            let start = Instant::now();
            let mesh = render_scene(shell.scene(&replica)?.as_ref());
            let vertices: Vec<_> = mesh
                .vertices
                .chunks_exact(6)
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
                .collect();
            canvas.set_draw_color(Color::RGB(16, 28, 41));
            canvas.clear();
            canvas.render_geometry(&vertices, None, sdl2::render::VertexIndices::Sequential)?;
            if let Some(path) = capture.take() {
                let (w, h) = canvas.output_size()?;
                let rgb = canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::RGB24)?;
                let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
                ppm.extend(rgb);
                fs::write(path, ppm).map_err(|e| e.to_string())?;
            }
            canvas.present();
            if let Some(path) = &ready_file {
                let info = serde_json::json!({"pid":std::process::id(),"snapshot":replica.snapshot().hex_id(),"renderer":canvas.info().name,"stream_ready":observed_stream,"screen":shell.screen_name(),"keyboard_visible":shell.keyboard_visible(),"layout":replica.snapshot().layout().hex_id(),"release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default()});
                let tmp = format!("{path}.tmp");
                fs::write(&tmp, info.to_string())
                    .and_then(|_| fs::rename(tmp, path))
                    .map_err(|e| e.to_string())?;
            }

            println!(
                "Presented snapshot={} vertices={} submit_us={}",
                replica.snapshot().hex_id(),
                vertices.len(),
                start.elapsed().as_micros()
            );
            if saved_id != Some(replica.snapshot().id()) {
                if let Some(p) = &state {
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
                    .and_then(|_| fs::rename(&tmp, &path))
                    .map_err(|e| e.to_string())?;
                }
                saved_id = Some(replica.snapshot().id());
            }
            dirty = false;
            drawn += 1;
            if frames.is_some_and(|n| drawn >= n) {
                break;
            }
        }
        thread::sleep(Duration::from_millis(8));
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
