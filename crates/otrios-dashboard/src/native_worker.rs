// SPDX-License-Identifier: Apache-2.0
//! Safe state/network owner shared by the SDL and direct Linux presentation hosts.
use otrios_core::dashboard::{
    Replica, SyncSession,
    display::{Calibration, Surface, cached_regions},
    motion::{Intent, Stress},
    presentation::{Command, Model, Presentation},
    render_scene_with_font,
    shell::Shell,
};
use otrios_interfaces::{keyboard::Edit, scene::PointerEvent};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
enum NetworkEvent {
    Connected(u64),
    Packet(u64, Instant, Vec<u8>),
}
struct NetworkSend {
    epoch: u64,
    bytes: Vec<u8>,
}
#[derive(Clone)]
pub(super) struct Frame {
    pub(super) vertices: Arc<Vec<f32>>,
    #[allow(
        dead_code,
        reason = "Experimental glyph stream is consumed by the direct host; SDL recovery has no Slug pass"
    )]
    pub(super) glyph_vertices: Arc<Vec<f32>>,
    #[allow(
        dead_code,
        reason = "Ordered glyph stream is consumed by the direct host"
    )]
    pub(super) draws: Arc<Vec<u32>>,
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
    pub(super) state_confirmed: bool,
    pub(super) pending_commands: usize,
    pub(super) input_busy: bool,
    pub(super) intent: Intent,
    pub(super) controls: otrios_core::dashboard::controls::State,
    pub(super) updates: u64,
    pub(super) received_updates: u64,
    pub(super) remote_sample: Option<(u64, Instant)>,
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
    pub(super) font: Option<Arc<otrios_core::dashboard::font::Font>>,
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
    confirmed: bool,
    updates: u64,
    received_updates: u64,
    remote_sample: Option<(u64, Instant)>,
}
fn application_worker(
    o: Options,
    mut replica: Replica,
    work: mpsc::Receiver<Work>,
    publish: mpsc::SyncSender<Published>,
) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel::<NetworkSend>(1);
    let (reply_tx, reply_rx) = mpsc::sync_channel::<NetworkEvent>(2);
    let stream_ready = Arc::new(AtomicBool::new(false));
    let worker_ready = stream_ready.clone();
    let connection = Arc::new(AtomicU64::new(0));
    let abort = Arc::new(AtomicU64::new(0));
    let transport_connection = connection.clone();
    let transport_abort = abort.clone();
    let network_thread = o.url.clone().map(|url| {
        let release_file = o.release_file.clone();
        thread::spawn(move || {
            network(
                url,
                rx,
                reply_tx,
                release_file,
                worker_ready,
                transport_connection,
                transport_abort,
            )
        })
    });
    let started = Instant::now();
    let mut acknowledged = 0;
    let mut last_sync = Instant::now() - Duration::from_secs(1);
    let mut epoch = None;
    let mut session = SyncSession::default();
    let mut outgoing = None;
    let mut stopping = None;
    let mut published_key = None;
    let mut pending = None;
    let mut stress = Stress::new()?;
    let mut hz = 0;
    let mut stall_ms = 0;
    let mut stalled = Instant::now();
    let mut received_updates = 0;
    let mut remote_sample = None;
    loop {
        if network_thread.as_ref().is_some_and(|h| h.is_finished()) {
            return Err("Network worker stopped unexpectedly".into());
        }
        let mut stop = stopping.is_some();
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
        let mut received = false;
        // Protocol frames retain order; only published views may coalesce.
        for _ in 0..2 {
            let Ok(event) = reply_rx.try_recv() else {
                break;
            };
            match event {
                NetworkEvent::Connected(id) => {
                    epoch = Some(id);
                    session = SyncSession::default();
                    outgoing = None;
                    received = true;
                }
                NetworkEvent::Packet(id, received_at, bytes)
                    if epoch == Some(id) && connection.load(Ordering::Acquire) == id =>
                {
                    let result = if bytes.starts_with(b"OTML1") {
                        replica.set_presentation(if bytes.len() == 5 {
                            None
                        } else {
                            Some(&bytes[5..])
                        })
                    } else {
                        let result = replica.receive_sync(&mut session, &bytes);
                        if result.is_ok() {
                            stream_ready.store(true, Ordering::Release);
                        }
                        result
                    };
                    match result {
                        Ok(changed) => {
                            received_updates += u64::from(changed);
                            if changed && !bytes.starts_with(b"OTML1") {
                                remote_sample = Some((received_updates, received_at));
                            }
                            received = true;
                        }
                        Err(e) => {
                            eprintln!("Rejected stream: {e}");
                            abort.store(id, Ordering::Release);
                            epoch = None;
                            outgoing = None;
                            stream_ready.store(false, Ordering::Release);
                        }
                    }
                }
                NetworkEvent::Packet(..) => {}
            }
        }
        stress.tick(started.elapsed().as_secs_f64() * 1000., hz)?;
        if stall_ms > 0 && stalled.elapsed() >= Duration::from_secs(1) {
            thread::sleep(Duration::from_millis(stall_ms));
            stalled = Instant::now();
        }
        if stop && stopping.is_none() {
            stopping = Some(Instant::now());
        }
        let connected = epoch.is_some_and(|id| connection.load(Ordering::Acquire) == id);
        if !connected {
            outgoing = None;
        }
        if connected {
            if outgoing.is_none()
                && (stop || received || last_sync.elapsed() >= Duration::from_millis(100))
            {
                if let Some(bytes) = replica.generate_sync(&mut session)? {
                    outgoing = Some(NetworkSend {
                        epoch: epoch.expect("connected epoch"),
                        bytes,
                    });
                }
                last_sync = Instant::now();
            }
            if let Some(packet) = outgoing.take() {
                match tx.try_send(packet) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(packet)) => outgoing = Some(packet),
                    Err(_) => return Err("Network transport disconnected".into()),
                }
            }
        }
        let online = connected && stream_ready.load(Ordering::Acquire);
        let confirmed = network_thread.is_none() || (online && replica.is_confirmed(&session));
        if stop && (confirmed || stopping.is_some_and(|t| t.elapsed() >= Duration::from_secs(3))) {
            return Ok(());
        }
        let key = (
            replica.snapshot().id(),
            acknowledged,
            online,
            confirmed,
            stress.updates,
            received_updates,
        );
        if published_key != Some(key) {
            pending = Some(Published {
                replica: Arc::new(replica.clone()),
                acknowledged,
                online,
                confirmed,
                updates: stress.updates,
                received_updates,
                remote_sample,
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
    // --state is accepted for old supervisors, but clients never read or write it.
    let mut replica = Replica::new()?;
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
    if let Some(screen) = &o.screen {
        shell.open_name(screen);
    }
    shell.set_stress(o.load);
    let started = Instant::now();
    let mut sent_work = None;
    let mut confirmed = false;
    let mut online = false;
    let mut dirty = true;
    let mut pending = None;
    let mut serial = 0;
    let mut received_updates = 0;
    let mut remote_sample = None;
    let mut measured_sample = 0;
    let mut sync_to_present_ms = None;
    let mut scene_cache = None;
    let font = match &o.font {
        Some(font) => font.clone(),
        None => crate::font_resource::load()?,
    };
    shell.set_font(font.clone());
    let mut mesh_cache = Arc::new(Vec::new());
    let mut glyph_cache = Arc::new(Vec::new());
    let mut draws_cache = Arc::new(Vec::new());
    let mut copy_cache = Arc::new(Vec::new());
    let mut updates = 0;
    let (ink_tx, ink_out) = mpsc::sync_channel::<(Instant, Vec<u8>)>(1);
    let (ink_publish, ink_rx) = mpsc::sync_channel(64);
    let ink_connected = o.url.is_some();
    if let Some(url) = o.url.clone() {
        thread::spawn(move || ink_network(url, ink_out, ink_publish));
    }
    let mut ink_pending = None;
    loop {
        if app_worker.is_finished() {
            return app_worker
                .join()
                .map_err(|_| "Application worker panicked")?
                .and(Err("Application worker stopped unexpectedly".into()));
        }
        let now = started.elapsed().as_millis() as u64;
        let wall_seconds = otrios_core::dashboard::signal::wall_time_seconds();
        replica.set_wall_time(wall_seconds);
        dirty |= shell.set_wall_time(wall_seconds);
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
                    if let Some((id, at)) = frame.remote_sample
                        && id > measured_sample
                    {
                        measured_sample = id;
                        sync_to_present_ms = Some(at.elapsed().as_secs_f64() * 1000.);
                    }
                    if let Some(path) = &o.ready_file {
                        let info = serde_json::json!({"scanout_format":display.format,"color_bits":display.color_bits,"stride":display.stride,"video":display.video,"rotation":display.rotation,"output":display.output,"touch_calibrated":display.touch_calibrated,"touch_devices":display.touch_devices,"motion":{"playing":frame.intent.playing,"speed":frame.intent.speed,"count":frame.intent.count},"controls":{"level":frame.controls.level,"dial":frame.controls.dial,"pad":frame.controls.pad},"pid":std::process::id(),"snapshot":frame.snapshot,"renderer":renderer,"stream_ready":frame.online,"state_confirmed":frame.state_confirmed,"sync_to_present_ms":sync_to_present_ms,"pending_commands":frame.pending_commands,"input_busy":frame.input_busy,"screen":frame.screen,"keyboard_visible":frame.keyboard,"layout":frame.layout,"release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default()});
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
                || confirmed != next.confirmed
                || replica
                    .pending()
                    .iter()
                    .any(|c| c.sequence <= next.acknowledged)
                || updates != next.updates;
            replica.acknowledge(next.replica, next.acknowledged)?;
            online = next.online;
            confirmed = next.confirmed;
            updates = next.updates;
            received_updates = next.received_updates;
            remote_sample = next.remote_sample;
        }
        dirty |= shell.tick(now);
        for _ in 0..64 {
            let Ok(bytes) = ink_rx.try_recv() else {
                break;
            };
            match shell.receive_ink(&bytes, now) {
                Ok(changed) => dirty |= changed,
                Err(e) => eprintln!("Rejected ink: {e}"),
            }
        }
        if ink_connected {
            if ink_pending.is_none() {
                let bytes = shell.ink_outgoing(now);
                if !bytes.is_empty() {
                    ink_pending = Some((Instant::now(), bytes));
                }
            }
            if let Some(bytes) = ink_pending.take()
                && let Err(mpsc::TrySendError::Full(bytes)) = ink_tx.try_send(bytes)
            {
                ink_pending = Some(bytes);
            }
        }
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
                let mesh = render_scene_with_font(&scene, &font);
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
                glyph_cache = Arc::new(mesh.glyph_vertices);
                draws_cache = Arc::new(mesh.draws);
                scene_cache = Some(scene);
            }
            serial += 1;
            pending = Some(Frame {
                vertices: mesh_cache.clone(),
                glyph_vertices: glyph_cache.clone(),
                draws: draws_cache.clone(),
                copy_regions: copy_cache.clone(),
                snapshot: replica.snapshot().hex_id(),
                layout: replica.snapshot().layout().hex_id(),
                screen: shell.screen_name().into(),
                keyboard: shell.keyboard_visible(),
                editing: shell.editing(),
                online,
                state_confirmed: confirmed && replica.pending().is_empty(),
                pending_commands: replica.pending().len(),
                input_busy: shell.input_busy(),
                intent: replica.motion()?,
                controls: replica.controls()?,
                updates,
                received_updates,
                remote_sample,
                serial,
            });
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
fn ink_network(
    url: String,
    outgoing: mpsc::Receiver<(Instant, Vec<u8>)>,
    incoming: mpsc::SyncSender<Vec<u8>>,
) {
    use tungstenite::{Error, Message, stream::MaybeTlsStream};
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut url = url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    for suffix in ["/stream", "/sync"] {
        if let Some(root) = url.strip_suffix(suffix) {
            url = format!("{root}/demo/ink");
            break;
        }
    }
    loop {
        let initial = match outgoing.try_recv() {
            Ok(p) => Some(p),
            Err(mpsc::TryRecvError::Disconnected) => return,
            Err(_) => None,
        };
        let mut socket = match tungstenite::connect(url.as_str()) {
            Ok((s, _)) => s,
            Err(_) => {
                thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        let tcp = match socket.get_mut() {
            MaybeTlsStream::Plain(s) => Some(s),
            MaybeTlsStream::Rustls(s) => Some(&mut s.sock),
            _ => None,
        };
        if let Some(tcp) = tcp {
            let _ = tcp.set_read_timeout(Some(Duration::from_millis(5)));
            let _ = tcp.set_write_timeout(Some(Duration::from_secs(1)));
        }
        let mut pending = initial;
        loop {
            let packet = pending.take().or_else(|| outgoing.try_recv().ok());
            if let Some((at, bytes)) = packet
                && at.elapsed() < Duration::from_secs(1)
                && socket.send(Message::Binary(bytes.into())).is_err()
            {
                break;
            }
            match socket.read() {
                Ok(Message::Binary(bytes)) => {
                    if bytes.len() > 1024 * otrios_interfaces::ink::SERVER_BYTES
                        || incoming.try_send(bytes.to_vec()).is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {
                    let _ = socket.flush();
                }
                Err(Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
            match outgoing.try_recv() {
                Ok(packet) => pending = Some(packet),
                Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
}
fn network(
    url: String,
    rx: mpsc::Receiver<NetworkSend>,
    reply: mpsc::SyncSender<NetworkEvent>,
    release_file: Option<String>,
    stream_ready: Arc<AtomicBool>,
    connection: Arc<AtomicU64>,
    abort: Arc<AtomicU64>,
) {
    use tungstenite::{Error, Message, stream::MaybeTlsStream};
    let url = url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let url = if let Some(prefix) = url.strip_suffix("/sync") {
        format!("{prefix}/stream")
    } else {
        url
    };
    let mut announced_release = String::new();
    let mut epoch = 0_u64;
    loop {
        stream_ready.store(false, Ordering::Release);
        connection.store(0, Ordering::Release);
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
            let _ = tcp.set_read_timeout(Some(Duration::from_millis(5)));
            let _ = tcp.set_write_timeout(Some(Duration::from_secs(2)));
        }
        epoch = epoch.checked_add(1).expect("Connection epoch exhausted");
        connection.store(epoch, Ordering::Release);
        if reply.send(NetworkEvent::Connected(epoch)).is_err() {
            return;
        }
        eprintln!("Live state byte transport connected");
        loop {
            if abort.load(Ordering::Acquire) == epoch {
                break;
            }
            loop {
                match rx.try_recv() {
                    Ok(packet) if packet.epoch == epoch => {
                        if socket.send(Message::Binary(packet.bytes.into())).is_err() {
                            connection.store(0, Ordering::Release);
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if connection.load(Ordering::Acquire) == 0 {
                break;
            }
            match socket.read() {
                Ok(Message::Binary(bytes)) => {
                    if bytes.len() > otrios_core::dashboard::MAX_DOCUMENT_BYTES + 5 {
                        break;
                    }
                    if reply
                        .send(NetworkEvent::Packet(epoch, Instant::now(), bytes.to_vec()))
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(Message::Text(text)) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                        && value["type"] == "release"
                    {
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
        connection.store(0, Ordering::Release);
        stream_ready.store(false, Ordering::Release);
        thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_owner_receives_remote_edits_and_resyncs_after_transport_reconnect() {
        use std::net::TcpListener;
        use tungstenite::Message;
        enum ServerAction {
            Level(u16),
            Disconnect,
            Stop,
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (actions, commands) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut authority = Replica::new().unwrap();
            authority.set_control(0, [705, 0]).unwrap();
            'accept: loop {
                let (tcp, _) = listener.accept().unwrap();
                tcp.set_read_timeout(Some(Duration::from_millis(5)))
                    .unwrap();
                let mut wire = tungstenite::accept(tcp).unwrap();
                let mut session = SyncSession::default();
                loop {
                    for command in commands.try_iter() {
                        match command {
                            ServerAction::Level(n) => {
                                authority.set_control(0, [n, 0]).unwrap();
                            }
                            ServerAction::Disconnect => {
                                let _ = wire.close(None);
                                continue 'accept;
                            }
                            ServerAction::Stop => return,
                        }
                    }
                    if let Some(bytes) = authority.generate_sync(&mut session).unwrap()
                        && wire.send(Message::Binary(bytes.into())).is_err()
                    {
                        continue 'accept;
                    }
                    match wire.read() {
                        Ok(Message::Binary(bytes)) => {
                            authority.receive_sync(&mut session, &bytes).unwrap();
                        }
                        Ok(Message::Close(_)) => continue 'accept,
                        Ok(_) => {
                            let _ = wire.flush();
                        }
                        Err(tungstenite::Error::Io(e))
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                            ) => {}
                        Err(_) => continue 'accept,
                    }
                }
            }
        });
        let (work, receive) = mpsc::sync_channel(1);
        let (publish, views) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            application_worker(
                Options {
                    url: Some(format!("ws://{address}/stream")),
                    ..Options::default()
                },
                Replica::new().unwrap(),
                receive,
                publish,
            )
        });
        let wait = |level, dial, ack| {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let next = views.recv_timeout(Duration::from_secs(2)).unwrap();
                let c = next.replica.controls().unwrap();
                if next.online
                    && next.confirmed
                    && c.level == level
                    && c.dial == dial
                    && next.acknowledged >= ack
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Latest confirmed view did not arrive"
                );
            }
        };
        wait(705, 350, 0);
        actions.send(ServerAction::Level(804)).unwrap();
        work.send(Work {
            commands: vec![Command {
                sequence: 1,
                edit: otrios_core::dashboard::presentation::Edit::Control(1, [620, 0]),
            }],
            stress: 0,
            stall_ms: 0,
            stop: false,
        })
        .unwrap();
        wait(804, 620, 1);
        actions.send(ServerAction::Disconnect).unwrap();
        actions.send(ServerAction::Level(905)).unwrap();
        wait(905, 620, 1);
        actions.send(ServerAction::Stop).unwrap();
        server.join().unwrap();
        work.send(Work {
            commands: vec![],
            stress: 0,
            stall_ms: 0,
            stop: true,
        })
        .unwrap();
        worker.join().unwrap().unwrap();
    }
    #[test]
    fn client_ignores_disk_and_blocked_application_cannot_block_exact_release() {
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
        let original = Replica::new().unwrap();
        let saved = b"invalid legacy document that must never be loaded";
        fs::write(&path, saved).unwrap();
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
            fs::read(&path).unwrap(),
            saved,
            "Client wrote state to disk"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
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
        // The single frame slot stays full while navigation progresses.
        thread::sleep(Duration::from_millis(100));
        tx.send(Input::Back).unwrap();
        thread::sleep(Duration::from_millis(100));
        let first = fr.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first.screen, "3d");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let latest = fr.recv_timeout(Duration::from_secs(1)).unwrap();
            if latest.screen == "home" {
                assert!(latest.serial > first.serial);
                break;
            }
            assert!(Instant::now() < deadline);
        }
        assert!(!path.exists());
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        for suffix in ["", ".screen", ".presentation.flex"] {
            let _ = fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
