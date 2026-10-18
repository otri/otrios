// SPDX-License-Identifier: Apache-2.0
//! Safe state/network owner shared by the SDL and direct Linux presentation hosts.
use otrios_core::dashboard::{
    Replica, SyncSession,
    display::{Calibration, Surface, cached_regions},
    motion::{Intent, Stress},
    presentation::{Command, Model, Presentation},
    render_scene,
    shell::Shell,
};
use otrios_interfaces::{keyboard::Edit, scene::PointerEvent};
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
#[derive(Clone)]
pub(super) struct Frame {
    pub(super) vertices: Arc<Vec<f32>>,
    #[allow(
        dead_code,
        reason = "SDL texture cache; direct host uses retained VBOs"
    )]
    pub(super) copy_regions: Arc<Vec<otrios_interfaces::scene::Rect>>,
    pub(super) snapshot: String,
    pub(super) layout: String,
    pub(super) screen: String,
    pub(super) keyboard: bool,
    #[allow(
        dead_code,
        reason = "SDL desktop text input; frame uses the shared virtual keyboard"
    )]
    pub(super) editing: bool,
    pub(super) online: bool,
    pub(super) intent: Intent,
    pub(super) controls: otrios_core::dashboard::controls::State,
    pub(super) updates: u64,
    pub(super) received_updates: u64,
    pub(super) serial: u64,
}
#[derive(Clone, Copy)]
pub(super) struct DisplayInfo {
    pub(super) video: &'static str,
    pub(super) rotation: u16,
    pub(super) output: [u32; 2],
    pub(super) touch_calibrated: bool,
    pub(super) touch_devices: i32,
    pub(super) format: &'static str,
    pub(super) color_bits: [i32; 4],
    pub(super) stride: u32,
}
pub(super) enum Input {
    Pointer(PointerEvent),
    #[allow(dead_code, reason = "SDL desktop keyboard input")]
    Back,
    #[allow(dead_code, reason = "SDL desktop keyboard input")]
    Edit(Edit),
    #[allow(dead_code, reason = "SDL desktop keyboard input")]
    First,
    Cancel,
    Presented(Box<Frame>, String, DisplayInfo),
    Stop,
}
#[derive(Clone, Default)]
pub(super) struct Options {
    pub(super) url: Option<String>,
    pub(super) state: Option<String>,
    pub(super) frames: Option<usize>,
    pub(super) fullscreen: bool,
    pub(super) capture: Option<String>,
    pub(super) capture_after: usize,
    pub(super) seconds: Option<u64>,
    pub(super) exit_file: Option<String>,
    pub(super) ready_file: Option<String>,
    pub(super) release_file: Option<String>,
    pub(super) screen: Option<String>,
    pub(super) report: Option<String>,
    pub(super) load: u8,
    pub(super) objects: u16,
    pub(super) stall_ms: u64,
    pub(super) rotation: u16,
    pub(super) touch_matrix: Option<Calibration>,
    pub(super) completion_probe: bool,
    pub(super) texture_cache: bool,
    #[cfg(test)]
    pub(super) state_gate: Option<Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>>,
}
pub(super) fn options() -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--fullscreen" {
            o.fullscreen = true;
            continue;
        }
        if arg == "--completion-probe" {
            o.completion_probe = true;
            continue;
        }
        let value = args.next().ok_or("Missing argument value")?;
        match arg.as_str() {
            "--sync" => o.url = Some(value),
            "--state" => o.state = Some(value),
            "--frames" => o.frames = Some(value.parse::<usize>().map_err(|e| e.to_string())?),
            "--rotation" => {
                o.rotation = value.parse::<u16>().map_err(|e| e.to_string())?;
                Surface::new(o.rotation)?;
            }
            "--touch-matrix" => {
                let v: Vec<f32> = value
                    .split(',')
                    .map(|v| v.parse::<f32>().map_err(|e| e.to_string()))
                    .collect::<Result<_, _>>()?;
                o.touch_matrix = Some(Calibration::new(
                    v.try_into()
                        .map_err(|_| "Expected six touch coefficients")?,
                )?);
            }
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
    if o.completion_probe && (o.url.is_some() || o.state.is_some()) {
        return Err("GPU completion probing requires an isolated benchmark".into());
    }
    Ok(o)
}
struct Work {
    commands: Vec<Command>,
    stress: u8,
    stall_ms: u64,
    stop: bool,
}
struct Published {
    replica: Arc<Replica>,
    acknowledged: u64,
    online: bool,
    updates: u64,
    received_updates: u64,
}
fn application_worker(
    o: Options,
    mut replica: Replica,
    work: mpsc::Receiver<Work>,
    publish: mpsc::SyncSender<Published>,
) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let (reply_tx, reply_rx) = mpsc::sync_channel::<HostUpdate>(1);
    let stream_ready = Arc::new(AtomicBool::new(false));
    let worker_ready = stream_ready.clone();
    let network_thread = o.url.clone().map(|url| {
        let release_file = o.release_file.clone();
        thread::spawn(move || network(url, rx, reply_tx, release_file, worker_ready))
    });
    let started = Instant::now();
    let mut acknowledged = 0;
    let mut last_sync = Instant::now() - Duration::from_secs(1);
    let mut sent_id = None;
    let mut saved_id = None;
    let mut saved_presentation: Option<Vec<u8>> = None;
    let mut published_key = None;
    let mut pending = None;
    let mut stress = Stress::new()?;
    let mut hz = 0;
    let mut stall_ms = 0;
    let mut stalled = Instant::now();
    let mut received_updates = 0;
    loop {
        if network_thread.as_ref().is_some_and(|h| h.is_finished()) {
            return Err("Network worker stopped unexpectedly".into());
        }
        let mut stop = false;
        match work.try_recv() {
            Ok(next) => {
                #[cfg(test)]
                if let Some(gate) = &o.state_gate {
                    let (lock, wake) = &**gate;
                    let mut paused = lock.lock().map_err(|_| "State gate poisoned")?;
                    while *paused {
                        paused = wake.wait(paused).map_err(|_| "State gate poisoned")?;
                    }
                }
                for command in next.commands {
                    if command.sequence > acknowledged {
                        command.apply(&mut replica)?;
                        acknowledged = command.sequence;
                    }
                }
                hz = next.stress;
                stall_ms = next.stall_ms;
                stop = next.stop;
            }
            Err(mpsc::TryRecvError::Disconnected) => stop = true,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        while let Ok(next) = reply_rx.try_recv() {
            received_updates += u64::from(replica.merge(&next.document)?);
            replica.set_presentation(next.presentation.as_deref())?;
        }
        stress.tick(started.elapsed().as_secs_f64() * 1000., hz)?;
        if stall_ms > 0 && stalled.elapsed() >= Duration::from_secs(1) {
            thread::sleep(Duration::from_millis(stall_ms));
            stalled = Instant::now();
        }
        if network_thread.is_some() && last_sync.elapsed() >= Duration::from_millis(100) {
            if sent_id != Some(replica.snapshot().id()) && tx.try_send(replica.save()).is_ok() {
                sent_id = Some(replica.snapshot().id());
            }
            last_sync = Instant::now();
        }
        if saved_id != Some(replica.snapshot().id()) {
            if let Some(path) = &o.state {
                save_state(path, &mut replica)?;
                let bytes = replica
                    .presentation()
                    .map(|p| p.bytes())
                    .unwrap_or_default();
                if saved_presentation.as_deref() != Some(bytes) {
                    let path = format!("{path}.presentation.flex");
                    let tmp = format!("{path}.tmp");
                    fs::write(&tmp, bytes)
                        .and_then(|_| fs::rename(tmp, path))
                        .map_err(|e| e.to_string())?;
                    saved_presentation = Some(bytes.to_vec());
                }
            }
            saved_id = Some(replica.snapshot().id());
        }
        if stop {
            return Ok(());
        }
        let online = stream_ready.load(Ordering::Acquire);
        let key = (
            replica.snapshot().id(),
            acknowledged,
            online,
            stress.updates,
            received_updates,
        );
        if published_key != Some(key) {
            pending = Some(Published {
                replica: Arc::new(replica.clone()),
                acknowledged,
                online,
                updates: stress.updates,
                received_updates,
            });
            published_key = Some(key);
        }
        if let Some(next) = pending.take() {
            match publish.try_send(next) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(next)) => pending = Some(next),
                Err(_) => return Ok(()),
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}
pub(super) fn state_worker(
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
    let mut replica = Presentation::new(Arc::new(replica.clone()));
    let (work_tx, work_rx) = mpsc::sync_channel(1);
    let (published_tx, published_rx) = mpsc::sync_channel(1);
    let app_options = o.clone();
    let initial = replica.published().as_ref().clone();
    let app_worker =
        thread::spawn(move || application_worker(app_options, initial, work_rx, published_tx));
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
    let mut sent_work = None;
    let mut saved_screen = String::new();
    let mut online = false;
    let mut dirty = true;
    let mut pending = None;
    let mut serial = 0;
    let mut received_updates = 0;
    let mut scene_cache = None;
    let mut mesh_cache = Arc::new(Vec::new());
    let mut copy_cache = Arc::new(Vec::new());
    let mut updates = 0;
    loop {
        if app_worker.is_finished() {
            return app_worker
                .join()
                .map_err(|_| "Application worker panicked")?
                .and(Err("Application worker stopped unexpectedly".into()));
        }
        let now = started.elapsed().as_millis() as u64;
        for event in input.try_iter() {
            match event {
                Input::Stop => {
                    work_tx
                        .send(Work {
                            commands: replica.pending().to_vec(),
                            stress: 0,
                            stall_ms: 0,
                            stop: true,
                        })
                        .map_err(|_| "Application worker disconnected")?;
                    return app_worker
                        .join()
                        .map_err(|_| "Application worker panicked")?;
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
                Input::Presented(frame, renderer, display) => {
                    if let Some(path) = &o.ready_file {
                        let info = serde_json::json!({"scanout_format":display.format,"color_bits":display.color_bits,"stride":display.stride,"video":display.video,"rotation":display.rotation,"output":display.output,"touch_calibrated":display.touch_calibrated,"touch_devices":display.touch_devices,"motion":{"playing":frame.intent.playing,"speed":frame.intent.speed,"count":frame.intent.count},"controls":{"level":frame.controls.level,"dial":frame.controls.dial,"pad":frame.controls.pad},"pid":std::process::id(),"snapshot":frame.snapshot,"renderer":renderer,"stream_ready":frame.online,"screen":frame.screen,"keyboard_visible":frame.keyboard,"layout":frame.layout,"release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default()});
                        let tmp = format!("{path}.tmp");
                        fs::write(&tmp, info.to_string())
                            .and_then(|_| fs::rename(tmp, path))
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        while let Ok(next) = published_rx.try_recv() {
            let next: Published = next;
            dirty |= replica.snapshot().id() != next.replica.snapshot().id()
                || online != next.online
                || updates != next.updates;
            replica.acknowledge(next.replica, next.acknowledged)?;
            online = next.online;
            updates = next.updates;
            received_updates = next.received_updates;
        }
        dirty |= shell.tick(now);
        dirty |= shell.flush_controls(&mut replica, now)?;
        let wanted = (
            replica.sequence(),
            shell.stress_hz(),
            o.stall_ms.max(shell.stress_stall_ms()),
        );
        if sent_work != Some(wanted)
            && work_tx
                .try_send(Work {
                    commands: replica.pending().to_vec(),
                    stress: wanted.1,
                    stall_ms: wanted.2,
                    stop: false,
                })
                .is_ok()
        {
            sent_work = Some(wanted);
        }
        if dirty {
            let scene = shell.scene(&replica)?;
            if scene_cache
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &scene))
            {
                let mesh = render_scene(&scene);
                copy_cache = Arc::new(if o.texture_cache {
                    cached_regions(
                        &mesh.vertices,
                        otrios_interfaces::scene::Rect::new(0., 0., 1280., 800.),
                        [16, 28, 41, 255],
                    )
                } else {
                    Vec::new()
                });
                mesh_cache = Arc::new(mesh.vertices);
                scene_cache = Some(scene);
            }
            serial += 1;
            pending = Some(Frame {
                vertices: mesh_cache.clone(),
                copy_regions: copy_cache.clone(),
                snapshot: replica.snapshot().hex_id(),
                layout: replica.snapshot().layout().hex_id(),
                screen: shell.screen_name().into(),
                keyboard: shell.keyboard_visible(),
                editing: shell.editing(),
                online,
                intent: replica.motion()?,
                controls: replica.controls()?,
                updates,
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
            dirty = false;
        }
        if let Some(frame) = pending.take() {
            match frames.try_send(frame) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(frame)) => pending = Some(frame),
                Err(_) => {
                    let _ = work_tx.send(Work {
                        commands: replica.pending().to_vec(),
                        stress: 0,
                        stall_ms: 0,
                        stop: true,
                    });
                    return app_worker
                        .join()
                        .map_err(|_| "Application worker panicked")?;
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
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
    fn blocked_application_and_disk_worker_cannot_block_drag_or_exact_release() {
        use otrios_interfaces::scene::{Action, PointerPhase, TouchResponder};
        let root = std::env::temp_dir().join(format!(
            "otrios-local-feedback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("state.automerge");
        let mut original = Replica::new().unwrap();
        let saved = original.save();
        fs::write(&path, &saved).unwrap();
        let mut shell = Shell::default();
        shell.open_name("controls");
        let scene = shell.scene(&original).unwrap();
        let slider = scene
            .controls()
            .into_iter()
            .find(|c| c.node.responder == Some(TouchResponder::Drag(Action::Control(0))))
            .unwrap();
        let at = |x| {
            [
                slider.origin[0] + x * slider.scale,
                slider.origin[1] + 40. * slider.scale,
            ]
        };
        let gate = Arc::new((std::sync::Mutex::new(true), std::sync::Condvar::new()));
        let options = Options {
            state: Some(path.to_string_lossy().into()),
            screen: Some("controls".into()),
            state_gate: Some(gate.clone()),
            ..Options::default()
        };
        let (tx, rx) = mpsc::sync_channel(128);
        let (ft, fr) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || state_worker(options, rx, ft));
        fr.recv_timeout(Duration::from_secs(3)).unwrap();
        for (phase, position) in [
            (PointerPhase::Down, at(20.)),
            (PointerPhase::Move, at(160.)),
            (PointerPhase::Up, at(999.)),
        ] {
            tx.send(Input::Pointer(PointerEvent {
                id: 9,
                phase,
                position,
            }))
            .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut presented = false;
        while Instant::now() < deadline {
            if fr
                .recv_timeout(Duration::from_millis(100))
                .is_ok_and(|f| f.controls.level == 1000)
            {
                presented = true;
                break;
            }
        }
        let before_ack = fs::read(&path).unwrap();
        *gate.0.lock().unwrap() = false;
        gate.1.notify_all();
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        assert!(
            presented,
            "Local release feedback waited for the application worker"
        );
        assert_eq!(
            before_ack, saved,
            "The application worker was not actually held"
        );
        assert_eq!(
            Replica::load(&fs::read(&path).unwrap())
                .unwrap()
                .controls()
                .unwrap()
                .level,
            1000
        );
        fs::remove_dir_all(root).unwrap();
    }
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
