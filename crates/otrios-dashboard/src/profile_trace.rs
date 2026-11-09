// SPDX-License-Identifier: Apache-2.0
//! Bounded opt-in phase report sink. Operators use a private tmpfs path.
use std::{
    fs::File,
    io::Write,
    sync::{Mutex, OnceLock},
};

const MAX_BYTES: usize = 16 * 1024 * 1024;
struct Sink {
    file: Option<File>,
    bytes: usize,
}
static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();

pub(super) fn flush() {
    use otrios_core::canvas::profiling;
    if !profiling::enabled() {
        return;
    }
    let report = profiling::take();
    if report.events.is_empty() && report.dropped == 0 {
        return;
    }
    let sink = SINK.get_or_init(|| {
        let file = std::env::var_os("OTRIOS_CORE_PROFILE_OUT").and_then(|path| {
            let mut options = std::fs::OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(path)
                .map_err(|e| eprintln!("Phase trace disabled: {e}"))
                .ok()
        });
        Mutex::new(Sink { file, bytes: 0 })
    });
    let Ok(mut sink) = sink.lock() else { return };
    if sink.file.is_none() {
        return;
    }
    let Ok(mut bytes) = serde_json::to_vec(&report) else {
        return;
    };
    bytes.push(b'\n');
    if sink.bytes.saturating_add(bytes.len()) > MAX_BYTES {
        if let Some(file) = &mut sink.file {
            let _ = file.write_all(b"{\"event\":\"trace_limit\",\"bytes\":16777216}\n");
        }
        sink.file = None;
        return;
    }
    if let Some(file) = &mut sink.file
        && let Err(error) = file.write_all(&bytes)
    {
        eprintln!("Phase trace disabled: {error}");
        sink.file = None;
        return;
    }
    sink.bytes += bytes.len();
}
