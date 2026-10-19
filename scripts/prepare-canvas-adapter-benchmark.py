#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Add identical host-only benchmark instrumentation to an archived baseline."""
import argparse
import pathlib
import shutil

p = argparse.ArgumentParser()
p.add_argument('baseline', type=pathlib.Path)
a = p.parse_args()
repo = pathlib.Path(__file__).resolve().parent.parent
host = a.baseline / 'crates/otrios-dashboard'
for name in ['canvas_adapter_bench.rs', 'bench_telemetry.rs']:
    shutil.copyfile(repo / 'crates/otrios-dashboard/src' / name, host / 'src' / name)
manifest = host / 'Cargo.toml'
text = manifest.read_text()
if 'name = "canvas-adapter-bench"' not in text:
    manifest.write_text(text + '\n[[bin]]\nname = "canvas-adapter-bench"\npath = "src/canvas_adapter_bench.rs"\nrequired-features = ["canvas", "host-sync"]\n')
worker = host / 'src/native_worker.rs'
text = worker.read_text()
if 'mod bench_telemetry;' not in text:
    for old, new in [
        ('enum NetworkEvent {', '#[path = "bench_telemetry.rs"]\nmod bench_telemetry;\nenum NetworkEvent {'),
        ('let mut measured_sample = 0;', 'let mut measured_sample = 0;\n    let mut bench_present = bench_telemetry::Recorder::from_env()?;'),
        ('measured_sample = id;', 'measured_sample = id;\n                        bench_present.presented(id, at, frame.intent.count)?;'),
    ]:
        if text.count(old) != 1:
            raise ValueError('Unexpected baseline host: ' + old)
        text = text.replace(old, new, 1)
    worker.write_text(text)
print('Prepared identical probe and bounded presentation recorder; core/backend untouched')
