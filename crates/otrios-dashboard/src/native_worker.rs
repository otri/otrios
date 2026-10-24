// SPDX-License-Identifier: Apache-2.0
//! Safe app/state/network owner for the direct Linux presentation host.
use otrios_core::{
    canvas::{Client, hash, hex},
    dashboard::{
        display::{Calibration, Surface},
        motion::Intent,
        render_scene_with_font,
    },
};
use otrios_interfaces::scene::PointerEvent;
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
#[path = "bench_telemetry.rs"]
mod bench_telemetry;
#[path = "session_client.rs"]
mod session_client;
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
    pub(super) ui_release: String,
    pub(super) viewport: [f32; 2],
    pub(super) reflow: Option<otrios_core::canvas::reflow::Frame>,
    pub(super) vertices: Arc<Vec<f32>>,
    pub(super) glyph_vertices: Arc<Vec<f32>>,
    pub(super) draws: Arc<Vec<u32>>,
    pub(super) snapshot: String,
    pub(super) layout: String,
    pub(super) screen: String,
    pub(super) keyboard: bool,
    pub(super) online: bool,
    pub(super) state_confirmed: bool,
    pub(super) reload_ready: bool,
    pub(super) pending_commands: usize,
    pub(super) resources: otrios_core::canvas::ResourceUsage,
    pub(super) input_busy: bool,
    pub(super) intent: Intent,
    pub(super) updates: u64,
    pub(super) received_updates: u64,
    pub(super) remote_sample: Option<(u64, Instant)>,
    pub(super) serial: u64,
    pub(super) epoch: u32,
    pub(super) motion_kind: Option<bool>,
    pub(super) state_ref: String,
    pub(super) ui_ref: String,
}
impl Frame {
    fn blank(serial: u64, online: bool) -> Self {
        Self {
            ui_release: String::new(),
            viewport: [1280., 800.],
            reflow: None,
            vertices: Arc::new(vec![]),
            glyph_vertices: Arc::new(vec![]),
            draws: Arc::new(vec![]),
            snapshot: String::new(),
            layout: String::new(),
            screen: "locked".into(),
            keyboard: false,
            online,
            state_confirmed: false,
            reload_ready: false,
            pending_commands: 1,
            resources: Default::default(),
            input_busy: false,
            intent: Intent::default(),
            updates: 0,
            received_updates: 0,
            remote_sample: None,
            serial,
            epoch: 0,
            motion_kind: None,
            state_ref: String::new(),
            ui_ref: String::new(),
        }
    }
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
    CanvasPointer(u32, PointerEvent),
    CanvasCancel(u32),
    #[cfg(test)]
    Back,
    Presented(Box<Frame>, String, DisplayInfo),
    Stop,
}
#[derive(Clone, Default)]
pub(super) struct Options {
    pub(super) font: Option<Arc<otrios_core::dashboard::font::Font>>,
    pub(super) url: Option<String>,
    pub(super) state: Option<String>,
    pub(super) apps: Option<String>,
    pub(super) frames: Option<usize>,
    pub(super) capture: Option<String>,
    pub(super) capture_after: usize,
    pub(super) seconds: Option<u64>,
    pub(super) exit_file: Option<String>,
    pub(super) ready_file: Option<String>,
    pub(super) release_socket: Option<String>,
    pub(super) catalog_update_socket: Option<String>,
    pub(super) screen: Option<String>,
    pub(super) report: Option<String>,
    pub(super) load: u8,
    pub(super) objects: u16,
    pub(super) stall_ms: u64,
    pub(super) rotation: u16,
    pub(super) portrait: bool,
    pub(super) touch_matrix: Option<Calibration>,
    #[cfg(test)]
    pub(super) state_gate: Option<Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>>,
}
pub(super) fn options() -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or("Missing argument value")?;
        match arg.as_str() {
            "--layout" => {
                o.portrait = match value.as_str() {
                    "portrait" => true,
                    "landscape" => false,
                    _ => return Err("Choose portrait or landscape layout".into()),
                }
            }
            "--sync" => o.url = Some(value),
            "--state" => o.state = Some(value),
            "--apps" => o.apps = Some(value),
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
            "--catalog-update-socket" => o.catalog_update_socket = Some(value),
            "--release-socket" => o.release_socket = Some(value),
            "--screen" => o.screen = Some(value),
            "--report" => o.report = Some(value),
            "--objects" => o.objects = value.parse::<u16>().map_err(|e| e.to_string())?,
            "--load-hz" => o.load = value.parse::<u8>().map_err(|e| e.to_string())?,
            "--worker-stall-ms" => o.stall_ms = value.parse::<u64>().map_err(|e| e.to_string())?,
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    if o.state.is_some() {
        return Err("Clients keep state in RAM; remove the obsolete --state option".into());
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
/// Legacy command-line route aliases are an entry-point convenience only.
/// Rendering and input dispatch use the admitted app graph, never these strings.
fn entry_app(name: &str) -> Result<&str, String> {
    let id = match name {
        "controls" => "component-playground",
        "keyboard" => "input-lab",
        "animation" => "animation-lab",
        "3d" => "three-d-lab",
        "layout" => "layout-lab",
        value => value,
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
        return Err("Invalid app entry point".into());
    }
    Ok(id)
}
fn app_stream(url: &str) -> String {
    let url = url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    if url.ends_with("/session") {
        return url;
    }
    let root = ["/apps/stream", "/demo/stream", "/stream", "/sync"]
        .iter()
        .find_map(|suffix| url.strip_suffix(suffix))
        .unwrap_or(url.trim_end_matches('/'));
    format!("{root}/session")
}
fn app_resources(o: &Options) -> Result<(std::path::PathBuf, Vec<u8>), String> {
    let adjacent = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("No host folder")?
        .join("app-catalog.flex");
    let root = o
        .apps
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("apps"));
    let catalog = if o.apps.is_none() && adjacent.is_file() {
        adjacent
    } else {
        root.join("catalog.flex")
    };
    let bytes =
        fs::read(&catalog).map_err(|e| format!("App catalog {}: {e}", catalog.display()))?;
    Ok((root, bytes))
}
fn mount_local(client: &mut Client, root: &std::path::Path, app: &str) -> Result<(), String> {
    let folder = root.join(app).join("package");
    let ui = fs::read(folder.join("ui.flex")).map_err(|e| e.to_string())?;
    let description = otrios_core::canvas::decode_ui(&ui)?;
    let code = if description.behavior.is_some() {
        fs::read(folder.join("behavior.wasm")).map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    client
        .mount_local(&ui, &format!("app:{app}/state"), &code, &[])
        .map(|_| ())
}
pub(super) fn state_worker(
    o: Options,
    input: mpsc::Receiver<Input>,
    frames: mpsc::SyncSender<Frame>,
) -> Result<(), String> {
    if o.state.is_some() {
        return Err("Clients keep state in RAM; remove the obsolete --state option".into());
    }
    let (resources, catalog) = app_resources(&o)?;
    let mut client = Client::default();
    client.set_viewport(if o.portrait {
        [800., 1280.]
    } else {
        [1280., 800.]
    })?;
    client.admit_catalog(&catalog)?;
    let app = entry_app(o.screen.as_deref().unwrap_or("home"))?;
    if o.url.is_some() {
        client.open(&format!("app:{app}/ui"), &format!("app:{app}/state"));
    } else {
        mount_local(&mut client, &resources, app)?;
    }
    let font = match &o.font {
        Some(font) => font.clone(),
        None => crate::font_resource::load()?,
    };
    client.set_font(font.clone());
    let (tx, rx) = mpsc::sync_channel::<NetworkSend>(1);
    let (reply_tx, reply_rx) = mpsc::sync_channel::<NetworkEvent>(8);
    let stream_ready = Arc::new(AtomicBool::new(false));
    let connection = Arc::new(AtomicU64::new(0));
    let abort = Arc::new(AtomicU64::new(0));
    let (ink_tx, ink_out) = mpsc::sync_channel::<(Instant, Vec<u8>)>(1);
    let (ink_publish, ink_rx) = mpsc::sync_channel(64);
    let ink_active = Arc::new(AtomicBool::new(false));
    let active_release = Arc::new(Mutex::new(
        std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default(),
    ));
    let network_thread = o.url.as_ref().map(|url| {
        let url = app_stream(url);
        let ready = stream_ready.clone();
        let connected = connection.clone();
        let aborted = abort.clone();
        let release = o
            .release_socket
            .clone()
            .or_else(|| std::env::var("OTRIOS_RELEASE_SOCKET").ok());
        let active = ink_active.clone();
        let code_release = active_release.clone();
        thread::spawn(move || {
            network(
                url,
                rx,
                reply_tx,
                release,
                ready,
                connected,
                aborted,
                ink_out,
                ink_publish,
                active,
                code_release,
            )
        })
    });
    #[cfg(unix)]
    let catalog_socket = if let Some(path) = &o.catalog_update_socket {
        let socket = std::os::unix::net::UnixDatagram::bind(path).map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        Some(socket)
    } else {
        None
    };
    let mut ui_release = std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default();
    let mut catalog_candidate: Option<(String, Vec<u8>)> = None;
    let started = Instant::now();
    let mut epoch = None;
    let mut outgoing = None;
    let mut ink_pending = None;
    let mut pending = None;
    let mut dirty = true;
    let mut serial = 0;
    let mut received_updates = 0;
    let mut remote_sample = None;
    let mut measured_sample = 0;
    let mut bench_present = bench_telemetry::Recorder::from_env()?;
    let mut sync_to_present_ms = None;
    let mut last_status = None;
    let mut scene_cache = None;
    let mut mesh_cache = Arc::new(Vec::new());
    let mut glyph_cache = Arc::new(Vec::new());
    let mut draws_cache = Arc::new(Vec::new());
    let mut stopping: Option<Instant> = None;
    let mut next_stall = Instant::now() + Duration::from_secs(1);
    let mut stress_epoch = None;
    loop {
        if network_thread.as_ref().is_some_and(|h| h.is_finished()) {
            return Err("Network worker stopped unexpectedly".into());
        }
        let now = started.elapsed().as_millis() as u64;
        let stall_ms = o.stall_ms.max(client.stress_stall_ms());
        if stall_ms > 0 && Instant::now() >= next_stall {
            // Deliberately stall only this app/state owner for the diagnostic;
            // the direct display's clock and retained buffers remain independent.
            thread::sleep(Duration::from_millis(stall_ms));
            next_stall = Instant::now() + Duration::from_secs(1);
        }
        for event in input.try_iter() {
            match event {
                Input::Stop => {
                    stopping.get_or_insert_with(Instant::now);
                }
                Input::CanvasPointer(generation, e) => dirty |= client.pointer(generation, e)?,
                Input::CanvasCancel(generation) => {
                    if generation == client.epoch() {
                        dirty |= client.cancel()?;
                    }
                }
                #[cfg(test)]
                Input::Back => dirty |= client.back()?,
                Input::Presented(frame, renderer, display) => {
                    if let Some((id, at)) = frame.remote_sample
                        && id > measured_sample
                    {
                        measured_sample = id;
                        bench_present.presented(id, at, frame.intent.count)?;
                        sync_to_present_ms = Some(at.elapsed().as_secs_f64() * 1000.);
                    }
                    if let Some(path) = &o.ready_file {
                        let info = serde_json::json!({"scanout_format":display.format,"color_bits":display.color_bits,"stride":display.stride,"video":display.video,"rotation":display.rotation,"output":display.output,"touch_calibrated":display.touch_calibrated,"touch_devices":display.touch_devices,"motion":{"playing":frame.intent.playing,"speed":frame.intent.speed,"count":frame.intent.count},"pid":std::process::id(),"snapshot":frame.snapshot,"renderer":renderer,"stream_ready":frame.online,"state_confirmed":frame.state_confirmed,"reload_ready":frame.reload_ready,"sync_to_present_ms":sync_to_present_ms,"pending_commands":frame.pending_commands,"runtime_resources":frame.resources,"input_busy":frame.input_busy,"screen":frame.screen,"keyboard_visible":frame.keyboard,"layout":frame.layout,"ui_ref":frame.ui_ref,"state_ref":frame.state_ref,"epoch":frame.epoch,"release":frame.ui_release,"runtime_release":std::env::var("OTRIOS_RELEASE_ID").unwrap_or_default(),"ui_updates":true,"ui_release":frame.ui_release});
                        let tmp = format!("{path}.tmp");
                        fs::write(&tmp, info.to_string())
                            .and_then(|_| fs::rename(tmp, path))
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        let blocked = false;
        #[cfg(test)]
        let blocked = blocked
            || o.state_gate
                .as_ref()
                .map(|gate| {
                    gate.0
                        .lock()
                        .map(|held| *held)
                        .map_err(|_| "State gate poisoned")
                })
                .transpose()?
                .unwrap_or(false);
        if !blocked {
            // Wire messages retain order. Only immutable presentation views coalesce.
            for _ in 0..2 {
                let Ok(event) = reply_rx.try_recv() else {
                    break;
                };
                match event {
                    NetworkEvent::Connected(id) => {
                        epoch = Some(id);
                        client.reset_connection();
                        outgoing = None;
                    }
                    NetworkEvent::Packet(id, received_at, bytes)
                        if epoch == Some(id) && connection.load(Ordering::Acquire) == id =>
                    {
                        match client.receive(&bytes) {
                            Ok(changed) => {
                                stream_ready.store(true, Ordering::Release);
                                if changed {
                                    received_updates += 1;
                                    remote_sample = Some((received_updates, received_at));
                                    dirty = true;
                                }
                            }
                            Err(e) if client.rejected_mount_pending() => {
                                catalog_candidate = None;
                                eprintln!("Rejected app candidate; continuing current app: {e}");
                            }
                            Err(e) => {
                                eprintln!("Rejected app stream: {e}");
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
        }
        ink_active.store(
            matches!(
                client.presentation(),
                Some(otrios_interfaces::canvas::Presentation::TouchPad { .. })
            ),
            Ordering::Release,
        );
        #[cfg(unix)]
        if let Some(socket) = &catalog_socket {
            let mut notice = [0; 8193];
            if let Ok(n) = socket.recv(&mut notice) {
                let candidate = (|| -> Result<(String, Vec<u8>), String> {
                    if n > 8192 {
                        return Err("Oversized UI notice".into());
                    }
                    let v: serde_json::Value =
                        serde_json::from_slice(&notice[..n]).map_err(|e| e.to_string())?;
                    let id = v["release"]
                        .as_str()
                        .filter(|id| otrios_interfaces::release::valid_hash(id))
                        .ok_or("Invalid UI release")?;
                    let path = v["catalog"].as_str().ok_or("Missing UI catalog")?;
                    let bytes = {
                        use std::io::Read;
                        let mut bytes = Vec::new();
                        fs::File::open(path)
                            .map_err(|e| e.to_string())?
                            .take(512 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())?;
                        if bytes.len() > 512 * 1024 {
                            return Err("UI catalog budget exceeded".into());
                        }
                        bytes
                    };
                    if v["sha256"].as_str() != Some(&hex(&hash(&bytes))) {
                        return Err("UI catalog digest rejected".into());
                    }
                    Ok((id.into(), bytes))
                })();
                match candidate {
                    Ok(c) => catalog_candidate = Some(c),
                    Err(e) => eprintln!("UI update notice: {e}"),
                }
            }
        }
        if let Some((id, bytes)) = &catalog_candidate
            && !client.catalog_update_pending()
            && client.catalog_hash() == hash(bytes)
        {
            ui_release = id.clone();
            if let Ok(mut current) = active_release.lock() {
                *current = ui_release.clone();
            }
            catalog_candidate = None;
            dirty = true;
        }
        if catalog_candidate.is_some()
            && !client.catalog_update_pending()
            && client.can_update_catalog()
        {
            let (_, bytes) = catalog_candidate.as_ref().expect("candidate");
            if client.catalog_hash() != hash(bytes) {
                match client.update_catalog(bytes) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("UI update retained current view: {e}");
                        catalog_candidate = None;
                    }
                }
            }
        }
        dirty |= client.tick(now, otrios_core::dashboard::clock::wall_time_seconds())?;
        for bytes in ink_rx.try_iter().take(64) {
            dirty |= client.receive_ink(&bytes, now)?;
        }
        if o.url.is_some() {
            if ink_pending.is_none() {
                let bytes = client.ink_outgoing(now);
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
        let connected = epoch.is_some_and(|id| connection.load(Ordering::Acquire) == id);
        if !connected {
            outgoing = None;
        }
        if connected {
            if outgoing.is_none() {
                outgoing = client.outgoing()?.map(|bytes| NetworkSend {
                    epoch: epoch.expect("connected epoch"),
                    bytes,
                });
            }
            if let Some(packet) = outgoing.take() {
                match tx.try_send(packet) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(packet)) => outgoing = Some(packet),
                    Err(_) => return Err("Network transport disconnected".into()),
                }
            }
        } else if o.url.is_none() {
            // Local benchmark navigation loads admitted package bytes, with no
            // client persistence and no implicit state-service replacement.
            if let Some(bytes) = client.outgoing()?
                && let otrios_interfaces::canvas::ClientMessage::Mount { ui_ref, .. } =
                    otrios_core::canvas::decode_client(&bytes)?
            {
                let id = ui_ref
                    .strip_prefix("app:")
                    .and_then(|v| v.strip_suffix("/ui"))
                    .ok_or("Invalid local app reference")?;
                mount_local(&mut client, &resources, entry_app(id)?)?;
                dirty = true;
            }
        }
        let online = o.url.is_none() || (connected && stream_ready.load(Ordering::Acquire));
        let confirmed = o.url.is_none() || (online && client.confirmed());
        if !client.ui_ref().is_empty() && stress_epoch != Some(client.epoch()) {
            if o.load > 0 {
                dirty |= client.set_stress(o.load)?;
            }
            stress_epoch = Some(client.epoch());
        }
        dirty |= client.stress_tick(now as f64)?;
        let updates = client.stress_updates();
        let reload_ready = client.reload_ready();
        let status = (
            online,
            confirmed,
            updates,
            client.input_busy(),
            reload_ready,
        );
        dirty |= last_status != Some(status);
        last_status = Some(status);
        if stopping.is_some_and(|at| confirmed || at.elapsed() >= Duration::from_secs(3)) {
            return Ok(());
        }
        if dirty && client.ui_ref().is_empty() {
            serial += 1;
            let mut frame = Frame::blank(serial, online);
            frame.reload_ready = reload_ready;
            frame.pending_commands = client.pending_count();
            pending = Some(frame);
            scene_cache = None;
            dirty = false;
        }
        if dirty && !client.ui_ref().is_empty() {
            let scene = client.scene()?;
            if scene_cache
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &scene))
            {
                let mesh = render_scene_with_font(&scene, &font);
                mesh_cache = Arc::new(mesh.vertices);
                glyph_cache = Arc::new(mesh.glyph_vertices);
                draws_cache = Arc::new(mesh.draws);
                scene_cache = Some(scene);
            }
            let motion = client.motion_intent()?;
            let mut intent = motion.map(|v| v.1).unwrap_or_default();
            if o.objects > 0 {
                intent.count = o.objects;
            }
            let ui_ref = client.ui_ref();
            let state_ref = client.state_ref();
            serial += 1;
            pending = Some(Frame {
                ui_release: ui_release.clone(),
                viewport: {
                    let scene = client.scene()?;
                    [scene.root().frame.width, scene.root().frame.height]
                },
                reflow: client.reflow_frame()?,
                vertices: mesh_cache.clone(),
                glyph_vertices: glyph_cache.clone(),
                draws: draws_cache.clone(),
                // These telemetry fields now identify logical app roots, not old
                // dashboard snapshot/layout hashes.
                snapshot: state_ref.clone(),
                layout: ui_ref.clone(),
                screen: ui_ref
                    .strip_prefix("app:")
                    .and_then(|v| v.strip_suffix("/ui"))
                    .unwrap_or("app")
                    .into(),
                keyboard: client.keyboard_visible(),
                online,
                state_confirmed: confirmed,
                reload_ready,
                pending_commands: client.pending_count(),
                resources: client.resource_usage(),
                input_busy: client.input_busy(),
                intent,
                updates,
                received_updates,
                remote_sample,
                serial,
                epoch: client.epoch(),
                motion_kind: motion.map(|v| v.0),
                state_ref,
                ui_ref,
            });
            dirty = false;
        }
        if let Some(frame) = pending.take() {
            match frames.try_send(frame) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(frame)) => pending = Some(frame),
                Err(_) => return Ok(()),
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}
#[cfg(unix)]
fn notify_release(
    path: &Option<String>,
    event: &otrios_interfaces::session::ServerControl,
) -> bool {
    if let Some(path) = path
        && event.release().is_some()
    {
        let result = std::os::unix::net::UnixDatagram::unbound().and_then(|socket| {
            socket.set_nonblocking(true)?;
            let bytes = serde_json::to_vec(event).map_err(std::io::Error::other)?;
            socket.send_to(&bytes, path)
        });
        if let Err(e) = result {
            eprintln!("Release supervisor notice: {e}");
            return false;
        }
    }
    true
}
#[cfg(not(unix))]
fn notify_release(_: &Option<String>, _: &otrios_interfaces::session::ServerControl) -> bool {
    true
}
#[allow(clippy::too_many_arguments)]
fn network(
    url: String,
    rx: mpsc::Receiver<NetworkSend>,
    reply: mpsc::SyncSender<NetworkEvent>,
    release_socket: Option<String>,
    stream_ready: Arc<AtomicBool>,
    connection: Arc<AtomicU64>,
    abort: Arc<AtomicU64>,
    ink_out: mpsc::Receiver<(Instant, Vec<u8>)>,
    ink_in: mpsc::SyncSender<Vec<u8>>,
    ink_active: Arc<AtomicBool>,
    active_release: Arc<Mutex<String>>,
) {
    use otrios_interfaces::session::{self as wire, ServerControl};
    use tungstenite::{Error, Message, stream::MaybeTlsStream};
    let _ = rustls::crypto::ring::default_provider().install_default();
    let url = app_stream(&url);
    let mut epoch = 0_u64;
    loop {
        stream_ready.store(false, Ordering::Release);
        connection.store(0, Ordering::Release);
        let mut socket = match tungstenite::client::connect_with_config(
            url.as_str(),
            Some(
                tungstenite::protocol::WebSocketConfig::default()
                    .max_message_size(Some(otrios_core::canvas::MAX_BYTES + 1))
                    .max_frame_size(Some(otrios_core::canvas::MAX_BYTES + 1)),
            ),
            0,
        ) {
            Ok((s, _)) => s,
            Err(e) => {
                eprintln!("Session disconnected: {e}");
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
            let _ = tcp.set_nodelay(true);
        }
        let current = active_release
            .lock()
            .ok()
            .map(|id| id.clone())
            .filter(|r| otrios_interfaces::release::valid_hash(r));
        if socket.send(session_client::hello(current, true)).is_err() {
            thread::sleep(Duration::from_millis(250));
            continue;
        }
        let mut greeted = false;
        let mut compatible = false;
        let mut available = false;
        let mut subscribed = false;
        let mut heard = Instant::now();
        loop {
            if connection.load(Ordering::Acquire) != 0 && abort.load(Ordering::Acquire) == epoch {
                break;
            }
            // State backpressure causes bounded reconnect/resync, never blocks control reads.
            loop {
                match rx.try_recv() {
                    Ok(packet) if greeted && compatible && available && packet.epoch == epoch => {
                        if socket
                            .send(Message::Binary(
                                wire::frame(wire::STATE, packet.bytes).into(),
                            ))
                            .is_err()
                        {
                            connection.store(0, Ordering::Release);
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if greeted && compatible && available && connection.load(Ordering::Acquire) == 0 {
                break;
            }
            if greeted && compatible && available {
                let active = ink_active.load(Ordering::Acquire);
                if active != subscribed {
                    if socket
                        .send(Message::Text(
                            serde_json::to_string(&wire::ClientControl::Ink { enabled: active })
                                .expect("ink control")
                                .into(),
                        ))
                        .is_err()
                    {
                        break;
                    }
                    subscribed = active;
                }
                if let Ok((at, bytes)) = ink_out.try_recv()
                    && active
                    && at.elapsed() < Duration::from_secs(1)
                    && socket
                        .send(Message::Binary(wire::frame(wire::INK, bytes).into()))
                        .is_err()
                {
                    break;
                }
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    heard = Instant::now();
                    let Ok(event) = session_client::decode(&text) else {
                        break;
                    };
                    if !greeted && !matches!(event, ServerControl::Hello { .. }) {
                        break;
                    }
                    // The stable hello/upgrade reaches the supervisor before any codec work.
                    if !notify_release(&release_socket, &event) {
                        break;
                    }
                    let was = compatible && available;
                    match event {
                        ServerControl::Hello {
                            compatible: c,
                            state_available: a,
                            ..
                        } if !greeted => {
                            greeted = true;
                            compatible = c;
                            available = a;
                            eprintln!("Host session negotiated compatible={c} state_available={a}");
                        }
                        ServerControl::Upgrade {
                            compatible: c,
                            state_available: a,
                            ..
                        } => {
                            compatible = c;
                            available = a;
                        }
                        ServerControl::StateStatus { available: a, .. } => available = a,
                        _ => break,
                    }
                    if compatible && available && !was {
                        epoch = epoch.checked_add(1).expect("Session epoch exhausted");
                        connection.store(epoch, Ordering::Release);
                        if reply.try_send(NetworkEvent::Connected(epoch)).is_err() {
                            break;
                        }
                    } else if !compatible || !available {
                        connection.store(0, Ordering::Release);
                        stream_ready.store(false, Ordering::Release);
                    }
                }
                Ok(Message::Binary(bytes)) => {
                    heard = Instant::now();
                    if !greeted || !compatible || !available {
                        break;
                    }
                    match bytes.split_first() {
                        Some((&wire::STATE, payload))
                            if !payload.is_empty()
                                && payload.len() <= otrios_core::canvas::MAX_BYTES =>
                        {
                            if reply
                                .try_send(NetworkEvent::Packet(
                                    epoch,
                                    Instant::now(),
                                    payload.to_vec(),
                                ))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Some((&wire::INK, payload))
                            if payload.len() <= 1025 * otrios_interfaces::ink::SERVER_BYTES =>
                        {
                            if ink_active.load(Ordering::Acquire) {
                                let _ = ink_in.try_send(payload.to_vec());
                            }
                        }
                        _ => break,
                    }
                }
                Ok(Message::Ping(_) | Message::Pong(_)) => {
                    heard = Instant::now();
                    if socket.flush().is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => break,
                Err(Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if heard.elapsed() > Duration::from_secs(if greeted { 60 } else { 5 }) {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Session connection: {e}");
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
    struct Fixture {
        root: std::path::PathBuf,
        ui: Vec<u8>,
        schema: otrios_interfaces::canvas::StateSchema,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn fixture() -> Fixture {
        use otrios_interfaces::{canvas::*, layout::*};
        let root = std::env::temp_dir().join(format!(
            "otrios-canvas-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let schema = StateSchema {
            id: "worker-motion-v1".into(),
            fields: [
                (1, "playing", 1000.),
                (2, "speed", 200.),
                (3, "count", 175.),
                (4, "load", 0.),
            ]
            .into_iter()
            .map(|(id, name, n)| Field {
                id,
                name: name.into(),
                default: Value::Number(n),
                writable: true,
            })
            .collect(),
        };
        let number = |n| Expr::Literal(Value::Number(n));
        let mut entries = Vec::new();
        let mut motion_ui = Vec::new();
        for (id, motion) in [("home", false), ("three-d-lab", true)] {
            let layout = otrios_core::dashboard::layout::Layout::encode(&Blueprint {
                composition: None,
                schema: "otrios-layout-v2".into(),
                templates: vec![Template {
                    id: 1,
                    name: "Root".into(),
                    parameters: vec![],
                    root: TemplateNode {
                        id: 0,
                        resize: None,
                        frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                        visible: Expr::Literal(Value::Boolean(true)),
                        shape: Shape::Canvas { clip: true },
                        responder: motion.then_some(Responder::AppEvent {
                            control: 0,
                            drag: true,
                        }),
                        semantics: None,
                        children: vec![TemplateNode {
                            id: 1,
                            resize: None,
                            frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                            visible: Expr::Literal(Value::Boolean(true)),
                            shape: Shape::Superellipse {
                                radius: number(0.),
                                exponent: 4.,
                                color: Expr::Literal(Value::Color([16, 28, 41, 255])),
                            },
                            responder: None,
                            semantics: None,
                            children: vec![],
                        }],
                    },
                }],
                root: Instance {
                    id: 0,
                    template: 1,
                    frame: [0., 0., 1280., 800.],
                    properties: vec![],
                    children: vec![],
                },
            })
            .unwrap();
            let ui = otrios_core::canvas::encode_ui(&Ui {
                format: "otrios-canvas-v2".into(),
                ui_ref: format!("app:{id}/ui"),
                title: id.into(),
                state_schema: if motion {
                    schema.clone()
                } else {
                    StateSchema {
                        id: "worker-home-v1".into(),
                        fields: vec![],
                    }
                },
                layout,
                selectors: if motion {
                    vec![Selector {
                        local: 1,
                        field: 3,
                        projection: Projection::Identity,
                    }]
                } else {
                    vec![]
                },
                handlers: if motion {
                    vec![Handler {
                        control: 0,
                        event: 0,
                        kind: GestureKind::Slider,
                        operation: Operation::Set,
                        fields: vec![3],
                    }]
                } else {
                    vec![]
                },
                navigation: vec![],
                behavior: None,
                presentation: motion.then_some(Presentation::Motion {
                    three_d: true,
                    playing: 1,
                    speed: 2,
                    count: 3,
                    load: 4,
                }),
            })
            .unwrap();
            let folder = root.join(id).join("package");
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("ui.flex"), &ui).unwrap();
            entries.push(CatalogEntry {
                app: App {
                    id: id.into(),
                    title: id.into(),
                    ui_ref: format!("app:{id}/ui"),
                    state_ref: format!("app:{id}/state"),
                    status: "canvas-prototype".into(),
                    dependencies: vec![],
                },
                ui_hash: otrios_core::canvas::hash(&ui),
                private: false,
            });
            if motion {
                motion_ui = ui;
            }
        }
        fs::write(
            root.join("catalog.flex"),
            otrios_core::canvas::encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: entries,
            })
            .unwrap(),
        )
        .unwrap();
        Fixture {
            root,
            ui: motion_ui,
            schema,
        }
    }
    #[test]
    fn captured_release_stays_local_and_slow_frame_consumer_cannot_block_navigation() {
        use otrios_interfaces::scene::PointerPhase;
        let fixture = fixture();
        let gate = Arc::new((std::sync::Mutex::new(true), std::sync::Condvar::new()));
        let options = Options {
            apps: Some(fixture.root.to_string_lossy().into()),
            screen: Some("3d".into()),
            state_gate: Some(gate),
            ..Options::default()
        };
        let (tx, rx) = mpsc::sync_channel(128);
        let (ft, fr) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || state_worker(options, rx, ft));
        let first = fr.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(first.motion_kind, Some(true));
        assert_eq!(first.intent.count, 12);
        for (phase, x) in [
            (PointerPhase::Down, 128.),
            (PointerPhase::Move, 640.),
            (PointerPhase::Up, 2000.),
        ] {
            tx.send(Input::CanvasPointer(
                first.epoch,
                PointerEvent {
                    id: 9,
                    phase,
                    position: [x, 400.],
                },
            ))
            .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let next = fr.recv_timeout(Duration::from_secs(2)).unwrap();
            if next.intent.count == 64 && !next.input_busy {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Captured release waited for a blocked transport"
            );
        }
        // Leave the single-slot frame mailbox unread while navigation proceeds.
        tx.send(Input::Back).unwrap();
        thread::sleep(Duration::from_millis(100));
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let next = fr.recv_timeout(Duration::from_secs(2)).unwrap();
            if next.ui_ref == "app:home/ui" {
                assert!(next.serial > first.serial);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "A full frame mailbox blocked navigation"
            );
        }
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        assert!(!fixture.root.join("three-d-lab.automerge").exists());
    }
    #[test]
    fn app_transport_receives_remote_edits_and_resynchronizes_after_reconnect() {
        use otrios_core::canvas::{Document, Session, decode_client, encode_server};
        use otrios_interfaces::canvas::{ClientMessage, ServerMessage};
        use tungstenite::Message;
        enum ServerAction {
            Count(u16),
            Disconnect,
        }
        let fixture = fixture();
        let schema = fixture.schema.clone();
        let ui = fixture.ui.clone();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let (actions, commands) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut authority = Document::new(schema, "app:three-d-lab/state").unwrap();
            authority.set(&[(3, 500)]).unwrap();
            'accept: loop {
                if server_stop.load(Ordering::Acquire) {
                    return;
                }
                let (tcp, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("Server accept: {e}"),
                };
                tcp.set_nonblocking(false).unwrap();
                tcp.set_read_timeout(Some(Duration::from_millis(5)))
                    .unwrap();
                let mut socket = tungstenite::accept(tcp).unwrap();
                loop {
                    match socket.read() {
                        Ok(Message::Text(t)) => {
                            let h: serde_json::Value = serde_json::from_str(&t).unwrap();
                            assert_eq!(h["type"], "hello");
                            break;
                        }
                        Err(tungstenite::Error::Io(e))
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                            ) => {}
                        other => panic!("{other:?}"),
                    }
                }
                socket.send(Message::Text(serde_json::json!({"type":"hello","version":1,"release":null,"refresh":false,"compatible":true,"state_available":true}).to_string().into())).unwrap();
                let mut sync = Session::default();
                let mut epoch = None;
                let mut staged = None;
                let mut staged_sync = None;
                loop {
                    if server_stop.load(Ordering::Acquire) {
                        return;
                    }
                    for action in commands.try_iter() {
                        match action {
                            ServerAction::Count(n) => {
                                authority.set(&[(3, n)]).unwrap();
                            }
                            ServerAction::Disconnect => {
                                let _ = socket.close(None);
                                continue 'accept;
                            }
                        }
                    }
                    if let Some(epoch) = epoch
                        && let Some(bytes) = authority.generate(&mut sync).unwrap()
                        && socket
                            .send(Message::Binary(
                                otrios_interfaces::session::frame(
                                    1,
                                    encode_server(&ServerMessage::Sync { epoch, bytes }).unwrap(),
                                )
                                .into(),
                            ))
                            .is_err()
                    {
                        continue 'accept;
                    }
                    match socket.read() {
                        Ok(Message::Binary(bytes)) => {
                            match decode_client(bytes.strip_prefix(&[1]).unwrap()).unwrap() {
                                ClientMessage::Mount {
                                    epoch: id,
                                    writer,
                                    resume,
                                    ..
                                } => {
                                    staged = Some(id);
                                    let (mut prepared, _) =
                                        authority.open_session(&writer, resume).unwrap();
                                    let state = authority.generate(&mut prepared).unwrap().unwrap();
                                    staged_sync = Some(prepared);
                                    socket
                                        .send(Message::Binary(
                                            otrios_interfaces::session::frame(
                                                1,
                                                encode_server(&ServerMessage::Mounted {
                                                    epoch: id,
                                                    ui: ui.clone(),
                                                    ui_hash: otrios_core::canvas::hash(&ui),
                                                    code: vec![],
                                                    state,
                                                })
                                                .unwrap(),
                                            )
                                            .into(),
                                        ))
                                        .unwrap();
                                }
                                ClientMessage::AcceptMount { epoch: id } if staged == Some(id) => {
                                    epoch = staged.take();
                                    sync = staged_sync.take().unwrap();
                                }
                                ClientMessage::CancelMount { epoch: id } if staged == Some(id) => {
                                    staged = None;
                                }
                                ClientMessage::Sync { epoch: id, bytes } if epoch == Some(id) => {
                                    authority.receive(&mut sync, &bytes).unwrap();
                                }
                                _ => {}
                            }
                        }
                        Ok(Message::Close(_)) => continue 'accept,
                        Ok(_) => {
                            let _ = socket.flush();
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
        let options = Options {
            apps: Some(fixture.root.to_string_lossy().into()),
            screen: Some("3d".into()),
            url: Some(format!("ws://{address}/stream")),
            ..Options::default()
        };
        let (tx, rx) = mpsc::sync_channel(128);
        let (ft, fr) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || state_worker(options, rx, ft));
        let wait = |count| {
            let deadline = Instant::now() + Duration::from_secs(6);
            loop {
                let next = fr.recv_timeout(Duration::from_secs(3)).unwrap();
                if next.state_confirmed && next.online && next.intent.count == count {
                    return next;
                }
                assert!(
                    Instant::now() < deadline,
                    "Confirmed remote app view did not arrive"
                );
            }
        };
        wait(33);
        actions.send(ServerAction::Count(1000)).unwrap();
        wait(64);
        actions.send(ServerAction::Disconnect).unwrap();
        actions.send(ServerAction::Count(0)).unwrap();
        wait(1);
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        stop.store(true, Ordering::Release);
        server.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn incompatible_hello_notifies_supervisor_without_entering_the_state_decoder() {
        let fixture = fixture();
        let path = std::path::PathBuf::from(format!(
            "/tmp/otrios-hello-{}-{}.sock",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let notice = std::os::unix::net::UnixDatagram::bind(&path).unwrap();
        notice
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut s = tungstenite::accept(tcp).unwrap();
            assert!(matches!(s.read().unwrap(), tungstenite::Message::Text(_)));
            let id = "c".repeat(64);
            s.send(tungstenite::Message::Text(serde_json::json!({"type":"hello","version":1,"release":{"id":id,"web_entry":format!("/releases/{id}/web/index.html"),"sync_protocol":"future-v99"},"refresh":true,"compatible":false,"state_available":false}).to_string().into())).unwrap();
            assert!(!matches!(s.read(), Ok(tungstenite::Message::Binary(_))));
        });
        let options = Options {
            apps: Some(fixture.root.to_string_lossy().into()),
            url: Some(format!("ws://{address}/session")),
            release_socket: Some(path.to_string_lossy().into()),
            ..Options::default()
        };
        let (tx, rx) = mpsc::sync_channel(8);
        let (ft, fr) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || state_worker(options, rx, ft));
        let mut bytes = [0; 8192];
        let n = notice.recv(&mut bytes).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(value["release"]["id"], "c".repeat(64));
        let frame = fr.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(!frame.online);
        assert!(frame.ui_ref.is_empty());
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        server.join().unwrap();
        drop(notice);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn retired_initial_mount_still_presents_clean_reload_readiness() {
        use std::io::Write;
        let fixture = fixture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .write_all(b"HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        let options = Options {
            apps: Some(fixture.root.to_string_lossy().into()),
            url: Some(format!("ws://{address}/apps/stream")),
            ..Options::default()
        };
        let (tx, rx) = mpsc::sync_channel(8);
        let (ft, fr) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || state_worker(options, rx, ft));
        let frame = fr.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(frame.ui_ref.is_empty());
        assert!(!frame.online && !frame.state_confirmed);
        assert!(frame.reload_ready);
        assert_eq!(frame.pending_commands, 0);
        tx.send(Input::Stop).unwrap();
        worker.join().unwrap().unwrap();
        server.join().unwrap();
    }
    #[test]
    fn legacy_entry_aliases_do_not_drive_rendering_and_urls_keep_app_scope() {
        assert_eq!(entry_app("3d").unwrap(), "three-d-lab");
        assert_eq!(entry_app("controls").unwrap(), "component-playground");
        assert_eq!(entry_app("input-lab").unwrap(), "input-lab");
        assert!(entry_app("../monitoring").is_err());
        for input in [
            "https://host/stream",
            "https://host/sync",
            "https://host/demo/stream",
            "https://host/apps/stream",
            "https://host/",
        ] {
            assert_eq!(app_stream(input), "wss://host/session");
        }
    }
    #[test]
    fn obsolete_client_state_path_is_rejected_before_any_read_or_write() {
        let (tx, rx) = mpsc::channel();
        let (frames, _) = mpsc::sync_channel(1);
        let error = state_worker(
            Options {
                state: Some("/unreadable/legacy-state".into()),
                ..Options::default()
            },
            rx,
            frames,
        )
        .unwrap_err();
        assert!(error.contains("RAM"));
        drop(tx);
    }
}
