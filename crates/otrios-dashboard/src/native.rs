// SPDX-License-Identifier: Apache-2.0
//! Native GPU host: SDL owns the window/input, GLES2 draws the shared triangle list.
use otrios_core::dashboard::{MAX_DOCUMENT_BYTES, Replica, render};
use sdl2::{
    event::Event, keyboard::Keycode, mouse::MouseButton, pixels::Color, rect::FPoint,
    render::Vertex,
};
use std::{
    fs,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
fn main() -> Result<(), String> {
    let mut url = None;
    let mut state = None;
    let mut frames = None;
    let mut fullscreen = false;
    let mut capture = None;
    let mut seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str(){
        "--sync"=>url=Some(args.next().ok_or("Missing sync URL")?),
        "--state"=>state=Some(args.next().ok_or("Missing state file")?),
        "--frames"=>frames=Some(args.next().ok_or("Missing frames")?.parse::<usize>().map_err(|e|e.to_string())?),
        "--fullscreen"=>fullscreen=true,
        "--capture"=>capture=Some(args.next().ok_or("Missing capture path")?),
        "--seconds"=>seconds=Some(args.next().ok_or("Missing duration")?.parse::<u64>().map_err(|e|e.to_string())?),
        _=>return Err("Usage: dashboard-native [--sync http://HOST:8876/sync] [--state FILE] [--fullscreen] [--frames N] [--seconds N] [--capture FILE.ppm]".into())
    }
    }
    let mut replica = match &state {
        Some(p) if std::path::Path::new(p).exists() => {
            Replica::load(&fs::read(p).map_err(|e| e.to_string())?)?
        }
        _ => Replica::new()?,
    };
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let (reply_tx, reply_rx) = mpsc::sync_channel::<Vec<u8>>(1);
    if let Some(url) = url {
        thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(4)))
                .build()
                .into();
            while let Ok(bytes) = rx.recv() {
                match agent
                    .post(&url)
                    .header("Content-Type", "application/octet-stream")
                    .send(&bytes)
                {
                    Ok(mut res) => match res
                        .body_mut()
                        .with_config()
                        .limit(MAX_DOCUMENT_BYTES as u64)
                        .read_to_vec()
                    {
                        Ok(bytes) => {
                            let _ = reply_tx.try_send(bytes);
                        }
                        Err(e) => eprintln!("Sync read: {e}"),
                    },
                    Err(e) => eprintln!("Sync unavailable: {e}"),
                }
            }
        });
    }
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
    let mut events = sdl.event_pump()?;
    let mut dirty = true;
    let mut drawn = 0;
    let mut last_sync = Instant::now() - Duration::from_secs(1);
    let started = Instant::now();
    'running: loop {
        if seconds.is_some_and(|n| started.elapsed() >= Duration::from_secs(n)) {
            break;
        }
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if x >= 0 && y >= 0 {
                        dirty |= replica.touch(x as u32, y as u32)?;
                        last_sync = Instant::now() - Duration::from_secs(1);
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    repeat: false,
                    ..
                } => {
                    dirty |= replica.touch(200, 300)?;
                    last_sync = Instant::now() - Duration::from_secs(1);
                }
                Event::Window { .. } => dirty = true,
                _ => {}
            }
        }
        while let Ok(bytes) = reply_rx.try_recv() {
            match replica.merge(&bytes) {
                Ok(changed) => dirty |= changed,
                Err(e) => eprintln!("Rejected sync: {e}"),
            }
        }
        if last_sync.elapsed() >= Duration::from_secs(1) {
            let _ = tx.try_send(replica.save());
            last_sync = Instant::now();
        }
        if dirty {
            let start = Instant::now();
            let mesh = render(replica.snapshot());
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
            if let Some(path) = &capture {
                let (w, h) = canvas.output_size()?;
                let rgb = canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::RGB24)?;
                let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
                ppm.extend(rgb);
                fs::write(path, ppm).map_err(|e| e.to_string())?;
            }
            canvas.present();
            println!(
                "Presented snapshot={} vertices={} submit_us={}",
                replica.snapshot().hex_id(),
                vertices.len(),
                start.elapsed().as_micros()
            );
            if let Some(p) = &state {
                let temp = format!("{p}.tmp");
                fs::write(&temp, replica.save())
                    .and_then(|_| fs::rename(temp, p))
                    .map_err(|e| e.to_string())?;
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
