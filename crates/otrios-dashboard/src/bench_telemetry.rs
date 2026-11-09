// SPDX-License-Identifier: Apache-2.0
//! Opt-in bounded diagnostic; clocks and app values are not written in normal operation.
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Instant,
};
pub(super) struct Recorder {
    output: Option<BufWriter<File>>,
    samples: usize,
}
impl Recorder {
    pub(super) fn from_env() -> Result<Self, String> {
        let output = std::env::var_os("OTRIOS_BENCH_PRESENT_LOG")
            .map(|path| File::create(path).map(BufWriter::new))
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self { output, samples: 0 })
    }
    pub(super) fn presented(&mut self, id: u64, at: Instant, count: u16) -> Result<(), String> {
        if let Some(output) = &mut self.output
            && self.samples < 8192
        {
            writeln!(
                output,
                "{{\"sample\":{id},\"receive_to_present_us\":{},\"count\":{count}}}",
                at.elapsed().as_micros()
            )
            .map_err(|e| e.to_string())?;
            self.samples += 1;
        }
        Ok(())
    }
}

/// Native local-input diagnostic. Set OTRIOS_NATIVE_PAN_LOG to a path prefix;
/// `<prefix>.{input,worker,renderer}.jsonl` are created once and written only on
/// orderly lane exit (including Result error unwinding), never during a gesture.
/// Each lane retains at most 20,000 rows; its final summary reports dropped rows.
/// Set OTRIOS_NATIVE_PAN_FINISH to a unique absent sentinel path, then create
/// that file after the trial to flush all lanes without stopping the app.
/// All *_us timestamps use process-instant-v1, except drm_usec (kernel DRM clock).
/// Abrupt termination (SIGKILL/SIGTERM default handler) loses buffered rows; use
/// existing --seconds / --exit-file for orderly flush. Prefix must be unique per
/// process/run; files are truncated when opened. No live config change is needed.
/// Do not subtract DRM timestamps from process timestamps. Sampled timestamps may
/// be negative when a device sample predates trace initialization. The original
/// input Instant is preserved through native_input; receive rows precede routing,
/// so rejected/unrouted device samples may have no worker row. Join them by
/// sampled_us; sequence is assigned only on worker dequeue, not device offer.
/// Route-generated cancels have sampled_us=null: no sample time is invented.
///
/// Analysis recipe: load all three JSONL files; reject any summary dropped > 0.
/// Choose a warmed gesture's [a,b] in sampled_us from input/processed phase rows.
/// Join produced, renderer_receive, present and credit by serial. Select present
/// rows with unique_serial && local_update, input.epoch matching the gesture and
/// a <= input.sampled_us <= b. Count each serial once, never repeat animation
/// flips. Check input sequence advancement separately (one input can yield >1
/// non-pan update); these are local-input-associated frames, not semantic proof
/// of a pan. Restrict the trial to the intended local pan and confirm no writes.
/// Sort by returned_us; report count / ((b-a)/1e6), successive return intervals
/// (p95 nearest rank, max), sampled-to-return latency, first return minus a and
/// final return minus b. Include boundary gaps to detect hidden start/end stalls.
/// Use drm_usec intervals separately only with nonzero values and one CRTC.
/// Log credit_us separately: it is owner acknowledgement, not display completion.
/// Composer total includes scene work; its retained subprofile is not exposed by
/// Composer's public API. No glFinish or inferred scanout timestamp is used.
///
/// Exact parse example (save/extract this Python snippet; no new repository tool):
/// ```text
/// python3 - PREFIX A_US B_US EPOCH <<'PY'
/// import json, math, sys
/// prefix, a, b, epoch = sys.argv[1:]
/// a, b, epoch = int(a), int(b), int(epoch)
/// assert b > a
/// lanes = {}
/// for lane in ('input', 'worker', 'renderer'):
///     rows = [json.loads(l) for l in open(prefix+'.'+lane+'.jsonl')]
///     assert rows[0]['version'] == 1 and rows[0]['clock'] == 'process-instant-v1'
///     assert rows[-1]['event'] == 'summary' and rows[-1]['dropped'] == 0
///     lanes[lane] = rows[1:-1]
/// made = {r['serial']: r for r in lanes['worker'] if r['event'] == 'produced'}
/// credit = {r['serial']: r for r in lanes['worker'] if r['event'] == 'credit'}
/// received = {r['serial']: r for r in lanes['renderer'] if r['event'] == 'renderer_receive'}
/// processed = [r for r in lanes['worker'] if r['event'] == 'processed'
///              and r['input']['epoch'] == epoch and r['input']['sampled_us'] is not None
///              and a <= r['input']['sampled_us'] <= b]
/// device = [r for r in lanes['input'] if a <= r['sampled_us'] <= b]
/// print('device samples read',len(device),'processed',len(processed))
/// recv = {r['sampled_us']:r['received_us'] for r in device}
/// print('receive-to-dequeue us', [r['dequeued_us']-recv[r['input']['sampled_us']]
///       for r in processed if r['input']['sampled_us'] in recv])
/// selected = {}
/// for r in lanes['renderer']:
///     i = r.get('input')
///     if r['event'] == 'present' and r['unique_serial'] and r['local_update'] and i:
///         if i['epoch'] == epoch and i['sampled_us'] is not None and a <= i['sampled_us'] <= b:
///             assert r['serial'] not in selected
///             assert made[r['serial']]['input'] == i and r['serial'] in received
///             selected[r['serial']] = r
/// rows = sorted(selected.values(), key=lambda r: r['returned_us'])
/// assert rows and len({r['input']['sequence'] for r in rows}) == len(rows)
/// def stats(xs):
///     xs = sorted(xs)
///     return {'n':len(xs), 'p95_us':xs[math.ceil(.95*len(xs))-1], 'max_us':xs[-1]} if xs else None
/// ret = [r['returned_us'] for r in rows]
/// flips = [r['drm_usec'] for r in rows]
/// print('unique updates/s', len(rows)*1e6/(b-a))
/// print('return intervals', stats([y-x for x,y in zip(ret,ret[1:])]))
/// print('sample-to-return', stats([r['returned_us']-r['input']['sampled_us'] for r in rows]))
/// print('boundary gaps us', ret[0]-a, max(0,b-ret[-1]), 'final deadline us',ret[-1]-b)
/// print('credits handled', sum(r['serial'] in credit for r in rows))
/// if all(flips) and len({r['drm_crtc'] for r in rows}) == 1:
///     assert all(y > x for x,y in zip(flips,flips[1:]))
///     print('DRM flip intervals',stats([y-x for x,y in zip(flips,flips[1:])]))
/// PY
/// ```
pub(crate) struct PanRecorder {
    output: Option<BufWriter<File>>,
    rows: Vec<serde_json::Value>,
    dropped: u64,
    finish: Option<std::path::PathBuf>,
}
const PAN_LIMIT: usize = 20_000;
static PAN_ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
pub(crate) fn process_us(at: Instant) -> i128 {
    let origin = *PAN_ORIGIN.get_or_init(Instant::now);
    if at >= origin {
        at.duration_since(origin).as_micros() as i128
    } else {
        -(origin.duration_since(at).as_micros() as i128)
    }
}
#[derive(Clone, Copy)]
pub(crate) struct LocalInput {
    pub(crate) sequence: u64,
    pub(crate) epoch: u32,
    pub(crate) sampled: Option<Instant>,
    pub(crate) phase: &'static str,
}
impl LocalInput {
    pub(crate) fn json(self) -> serde_json::Value {
        serde_json::json!({"sequence":self.sequence,"epoch":self.epoch,
            "sampled_us":self.sampled.map(process_us),"phase":self.phase})
    }
}
impl PanRecorder {
    pub(crate) fn from_env(lane: &str) -> Result<Self, String> {
        let output = std::env::var_os("OTRIOS_NATIVE_PAN_LOG")
            .map(|prefix| {
                let mut path = prefix;
                path.push(format!(".{lane}.jsonl"));
                File::create(path).map(BufWriter::new)
            })
            .transpose()
            .map_err(|e| e.to_string())?;
        if output.is_some() {
            PAN_ORIGIN.get_or_init(Instant::now);
        }
        Ok(Self {
            rows: if output.is_some() {
                Vec::with_capacity(PAN_LIMIT)
            } else {
                Vec::new()
            },
            output,
            dropped: 0,
            finish: std::env::var_os("OTRIOS_NATIVE_PAN_FINISH").map(Into::into),
        })
    }
    pub(crate) fn enabled(&self) -> bool {
        self.output.is_some()
    }
    pub(crate) fn start(&self) -> Option<Instant> {
        self.enabled().then(Instant::now)
    }
    pub(crate) fn record(&mut self, row: impl FnOnce() -> serde_json::Value) {
        if !self.enabled() {
            return;
        }
        if self.rows.len() == PAN_LIMIT {
            self.dropped = self.dropped.saturating_add(1);
        } else {
            self.rows.push(row());
        }
    }
    /// Close this bounded diagnostic without stopping the live host. Each lane
    /// observes the same operator-owned sentinel after the measured gesture.
    pub(crate) fn finish_if_requested(&mut self) {
        if self.output.is_some() && self.finish.as_ref().is_some_and(|p| p.is_file()) {
            if let Err(error) = self.flush() {
                eprintln!("Native phase trace flush: {error}");
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(mut output) = self.output.take() {
            writeln!(
                output,
                "{}",
                serde_json::json!({"event":"schema","version":1,
                "clock":"process-instant-v1","pid":std::process::id(),"limit":PAN_LIMIT,
                "drm_clock":"kernel DRM event usec; separate domain"})
            )?;
            for row in &self.rows {
                writeln!(output, "{row}")?;
            }
            writeln!(
                output,
                "{}",
                serde_json::json!({"event":"summary","rows":self.rows.len(),"dropped":self.dropped})
            )?;
            output.flush()?;
        }
        Ok(())
    }
}
impl Drop for PanRecorder {
    fn drop(&mut self) {
        if let Err(error) = self.flush() {
            eprintln!("Native pan trace flush: {error}");
        }
    }
}
#[cfg(test)]
mod pan_tests {
    use super::*;
    #[test]
    fn sentinel_finishes_trace_without_stopping_owner() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("otrios-pan-finish-{unique}.jsonl"));
        let finish = path.with_extension("finish");
        let mut recorder = PanRecorder {
            output: Some(BufWriter::new(File::create(&path).unwrap())),
            rows: vec![],
            dropped: 0,
            finish: Some(finish.clone()),
        };
        recorder.record(|| serde_json::json!({"event":"trial"}));
        recorder.finish_if_requested();
        assert!(recorder.enabled());
        std::fs::write(&finish, []).unwrap();
        recorder.finish_if_requested();
        assert!(!recorder.enabled());
        let bytes = std::fs::read(&path).unwrap();
        let rows: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[1]["event"], "trial");
        assert_eq!(rows.last().unwrap()["rows"], 1);
        recorder.record(|| panic!("finished recorder must not evaluate rows"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(finish).unwrap();
    }
    #[test]
    fn bounded_deferred_jsonl_and_disabled_fast_path() {
        let path = std::env::temp_dir().join(format!("otrios-pan-{}.jsonl", std::process::id()));
        let mut r = PanRecorder {
            output: Some(BufWriter::new(File::create(&path).unwrap())),
            rows: Vec::new(),
            dropped: 0,
            finish: None,
        };
        for serial in 0..PAN_LIMIT + 3 {
            r.record(|| serde_json::json!({"event":"present","serial":serial}));
        }
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(r.rows.len(), PAN_LIMIT);
        assert_eq!(r.dropped, 3);
        r.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(rows.len(), PAN_LIMIT + 2);
        assert_eq!(rows[0]["clock"], "process-instant-v1");
        assert_eq!(rows[PAN_LIMIT]["serial"], PAN_LIMIT - 1);
        assert_eq!(rows[PAN_LIMIT + 1]["dropped"], 3);
        r.record(|| panic!("disabled recorder evaluated row"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn original_sample_clock_and_sequence_survive_copy() {
        let now = Instant::now();
        let sample = now - std::time::Duration::from_millis(123);
        let input = LocalInput {
            sequence: 42,
            epoch: 9,
            sampled: Some(sample),
            phase: "move",
        };
        let copy = input;
        assert_eq!(
            process_us(now) - copy.json()["sampled_us"].as_i64().unwrap() as i128,
            123_000
        );
        assert_eq!(copy.json()["sequence"], 42);
    }
}
