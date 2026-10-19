#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run on Linux beside baseline/merkle folders; emit per-child CPU and protocol results."""
import json
import os
import pathlib
import subprocess
import time

root = pathlib.Path(__file__).resolve().parent
for repetition in range(3):
    for hz, updates in [(10, 180), (30, 300)]:
        order = ['baseline', 'merkle'] if repetition % 2 == 0 else ['merkle', 'baseline']
        for label in order:
            folder = root / label
            port = 18901 if label == 'baseline' else 18902
            args = [str(folder / 'canvas-adapter-bench'), f'ws://127.0.0.1:{port}/apps/stream', str(folder / 'app-catalog.flex'), str(updates), str(hz), '20', 'paired']
            started = time.monotonic()
            process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            output, errors = (process.stdout.read(), process.stderr.read())
            _, status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(status)
            seconds = time.monotonic() - started
            if process.returncode:
                raise RuntimeError(errors.decode())
            report = json.loads(output)
            report.update(label=label, repetition=repetition, process_seconds=seconds, cpu_user_seconds=usage.ru_utime, cpu_system_seconds=usage.ru_stime, cpu_one_core_percent=100 * (usage.ru_utime + usage.ru_stime) / seconds, max_rss_kib=usage.ru_maxrss)
            print(json.dumps(report), flush=True)
