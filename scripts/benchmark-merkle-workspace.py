#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run alternating local-tree probes on a Linux frame; no renderer or authority service changes."""
import argparse
import hashlib
import json
import os
import pathlib
import shlex
import subprocess
import tarfile
import time

parser = argparse.ArgumentParser(description=__doc__)
for name in ['reference', 'candidate', 'known-hosts', 'output']:
    parser.add_argument('--' + name, type=pathlib.Path, required=True)
parser.add_argument('--frame', required=True)
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
if (output / 'results.jsonl').exists():
    raise RuntimeError('Choose a fresh output directory')
remote = '/var/lib/otrios-dashboard/bench-pipeline-micro-' + str(os.getpid())
ssh = ['ssh', '-oBatchMode=yes', '-oConnectTimeout=8', '-oStrictHostKeyChecking=yes',
       '-oUserKnownHostsFile=' + str(args.known_hosts.resolve()), args.frame]
runner = '''import json,os,pathlib,subprocess,sys,time
root=pathlib.Path(sys.argv[1])
for repetition in range(3):
 for label in (['reference','candidate'] if repetition%2==0 else ['candidate','reference']):
  started=time.monotonic();p=subprocess.Popen([str(root/label)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  data,errors=p.stdout.read(),p.stderr.read();_,status,usage=os.wait4(p.pid,0)
  p.returncode=os.waitstatus_to_exitcode(status)
  if p.returncode:raise RuntimeError(errors.decode())
  print(json.dumps(dict(label=label,repetition=repetition,seconds=time.monotonic()-started,cpu_user_seconds=usage.ru_utime,cpu_system_seconds=usage.ru_stime,max_rss_kib=usage.ru_maxrss,stdout=data.decode())),flush=True)
'''
(output / 'run-frame.py').write_text(runner)
archive = output / 'input.tar'
with tarfile.open(archive, 'w') as tar:
    for label, folder in [('reference', args.reference), ('candidate', args.candidate)]:
        tar.add(folder / 'merkle_tree_cost', arcname=label)
    tar.add(output / 'run-frame.py', arcname='run-frame.py')
metadata = {label: json.loads((folder / 'build.json').read_text())
            for label, folder in [('reference', args.reference), ('candidate', args.candidate)]}
metadata['method'] = 'Same generic tree workload; reference allocates hash buffers per node, candidate reuses a bounded workspace; 100 warm-up and 1000 timed edits per tree size; live frame renderer remains running; no network/durability inside timed operations.'
metadata['input_sha256'] = hashlib.sha256(archive.read_bytes()).hexdigest()
(output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
process = None
try:
    with archive.open('rb') as data:
        subprocess.run(ssh + ['mkdir -m 700 ' + shlex.quote(remote) + ' && tar -xf - -C ' + shlex.quote(remote)], stdin=data, check=True)
    with (output / 'results.jsonl').open('w') as result:
        process = subprocess.Popen(ssh + ['exec python3 ' + shlex.quote(remote + '/run-frame.py') + ' ' + shlex.quote(remote)], stdout=result)
        if process.wait(timeout=240):
            raise RuntimeError('Frame tree probe failed')
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    for attempt in range(5):
        time.sleep(1)
        if subprocess.run(ssh + ['pkill -f ' + shlex.quote('^' + remote + '/(reference|candidate)$') + ' || true; rm -rf -- ' + shlex.quote(remote)]).returncode == 0:
            break
    else:
        raise RuntimeError('Frame probe cleanup failed: ' + remote)
