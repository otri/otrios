#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reproducible, single-host Darwin/arm64 transport and ownership experiment."""
import argparse
import atexit
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--swiftc', required=True, help='Swift.org compiler with Embedded Swift libraries')
parser.add_argument('--wasmtime', required=True, help='Extracted Wasmtime 49.0.3 C API directory')
parser.add_argument('--rustc', default='rustc')
parser.add_argument('--clang', default='clang')
parser.add_argument('--trials', type=int, default=7)
parser.add_argument('--output', type=Path, default=ROOT/'bench-results'/'transport')
args = parser.parse_args()
if platform.system() != 'Darwin' or platform.machine() != 'arm64':
    raise SystemExit('This first harness is explicitly Darwin/arm64; port the link flags before using another host.')
if args.trials < 3:
    raise SystemExit('Use at least three process trials.')
out = args.output.resolve(); out.mkdir(parents=True, exist_ok=True)
swiftc = os.path.abspath(args.swiftc)  # Preserve the swiftc driver symlink name.
wasmtime = Path(args.wasmtime).resolve()
lld = str(Path(swiftc).parent/'lld')
env = dict(os.environ, MACOSX_DEPLOYMENT_TARGET='14.0')
commands = []
def run(command):
    command = [str(x) for x in command]
    started = time.perf_counter()
    result = subprocess.run(command, env=env, text=True, capture_output=True)
    elapsed = (time.perf_counter()-started)*1000
    commands.append({'argv': command, 'elapsed_ms': elapsed, 'returncode': result.returncode,
                     'stdout': result.stdout, 'stderr': result.stderr})
    (out/'commands.json').write_text(json.dumps(commands, indent=2))
    if result.returncode:
        raise RuntimeError(f'{command[0]} failed: {result.stderr}')
    return result.stdout.strip(), elapsed

def measured_build(command):
    # One unrecorded warm-up command, then independent compiler processes with warm OS caches.
    run(command)
    return [run(command)[1] for _ in range(args.trials)]

swift = [swiftc, '-enable-experimental-feature', 'Embedded', '-wmo', '-O', '-parse-as-library',
         '-module-cache-path', out/'module-cache', '-module-name', 'OTriBench']
rust = [args.rustc, '-O', '-C', 'panic=abort', '--edition=2024', '--crate-type=lib']
result = {'host': {'system': platform.platform(), 'machine': platform.machine()},
          'versions': {}, 'producers_ms': {}, 'receiver_compile_ms': {}, 'artifacts': {},
          'runs': {}, 'startup': {}, 'ownership': {}, 'method': {
              'process_trials': args.trials, 'inner_warmups': 2, 'inner_samples': 7,
              'compute_iterations': 8000000, 'scan_bytes': 65536, 'scan_rounds': 64,
              'receiver_optimizer': 'clang -O3 or Wasmtime Cranelift speed',
              'native_target': 'arm64-apple-macosx14.0', 'wasm_stack_bytes': 65536,
              'compile_cache_policy': 'OS caches warm; fresh compiler/runtime process; no persistent compiler cache',
              'wasm_version': '49.0.3 (official C API archive)',
              'ownership_scope': 'native only, one thread, platform allocator; no cross-process or Wasm ownership claim'}}
atexit.register(lambda: (out/'partial-results.json').write_text(json.dumps(result,indent=2)))
for name, cmd in [('swift', [swiftc,'--version']), ('rust', [args.rustc,'--version','--verbose']),
                  ('clang',[args.clang,'--version'])]:
    result['versions'][name] = run(cmd)[0]
for name, source in [('swift','kernel.swift'), ('rust','kernel.rs')]:
    bc = out/f'{name}.bc'; wasm = out/f'{name}.wasm'; library = out/f'{name}.dylib'
    command = (swift + ['-target','arm64-apple-macosx14.0','-emit-bc',HERE/source,'-o',bc]
               if name == 'swift' else rust + ['--emit=llvm-bc',HERE/source,'-o',bc])
    result['producers_ms'][name+'_bitcode'] = measured_build(command)
    if name == 'swift':
        object_file = out/'swift-wasm.o'
        # Emit object directly: Swift 6.4.1 -emit-bc for this Wasm target crashes in its summary writer.
        first = swift + ['-target','wasm32-unknown-none-wasm','-emit-object',HERE/source,'-o',object_file]
        second = [lld,'-flavor','wasm','--no-entry','--export=kernel','--export=scan','--export=noop',
                  '--export=__heap_base','--strip-all','-zstack-size=65536',object_file,'-o',wasm]
        run(first); run(second)
        timings=[]
        for _ in range(args.trials): timings.append(run(first)[1]+run(second)[1])
        result['producers_ms'][name+'_wasm'] = timings
    else:
        command = [x for x in rust if x != '--crate-type=lib'] + ['--crate-type=cdylib',
            '--target=wasm32-unknown-unknown','-C','link-arg=-zstack-size=65536',
            '-C','link-arg=--strip-all','-C','link-arg=--export=__heap_base',HERE/source,'-o',wasm]
        result['producers_ms'][name+'_wasm'] = measured_build(command)
    command = [args.clang,'-O3','-target','arm64-apple-macosx14.0','-dynamiclib','-Wl,-dead_strip',
               '-Wl,-exported_symbol,_kernel','-Wl,-exported_symbol,_scan','-Wl,-exported_symbol,_noop',bc,'-o',library]
    result['receiver_compile_ms'][name+'_bitcode'] = measured_build(command)
    print('built',name,flush=True)

run([args.clang,'-O3','-Wall','-Wextra','-Werror',HERE/'native_runner.c','-o',out/'native_runner'])
run([args.clang,'-O3','-Wall','-Wextra','-Werror','-I'+str(wasmtime/'include'),HERE/'wasm_runner.c',
     '-L'+str(wasmtime/'lib'),'-lwasmtime','-Wl,-rpath,'+str(wasmtime/'lib'),'-o',out/'wasm_runner'])

def json_run(command):
    text, elapsed = run(command)
    return {'wall_ms':elapsed, **json.loads(text)}

for name in ['swift','rust']:
    cache = out/f'{name}.cwasm'
    compilations=[json_run([out/'wasm_runner','compile',out/f'{name}.wasm',cache]) for _ in range(args.trials)]
    result['receiver_compile_ms'][name+'_wasm'] = compilations
    result['runs'][name+'_native'] = []
    result['runs'][name+'_wasm'] = []
    # Alternate host paths so one mode does not consistently benefit from thermal or frequency drift.
    for trial in range(args.trials):
        modes=['native','wasm'] if trial%2==0 else ['wasm','native']
        for mode in modes:
            command = ([out/'native_runner',out/f'{name}.dylib'] if mode=='native'
                       else [out/'wasm_runner','cached',cache])
            result['runs'][name+'_'+mode].append(json_run(command))
    result['startup'][name] = {
        'native_cached': [json_run([out/'native_runner',out/f'{name}.dylib','startup']) for _ in range(args.trials)],
        'wasm_cold': [json_run([out/'wasm_runner','startup',out/f'{name}.wasm']) for _ in range(args.trials)],
        'wasm_cached': [json_run([out/'wasm_runner','cached-startup',cache]) for _ in range(args.trials)]}
    for extension in ['bc','wasm','dylib','cwasm']:
        path=out/f'{name}.{extension}'; data=path.read_bytes()
        result['artifacts'][path.name]={'bytes':len(data),'gzip9_bytes':len(gzip.compress(data,compresslevel=9,mtime=0)),
                                     'sha256':hashlib.sha256(data).hexdigest()}
    print('measured',name,flush=True)

for name in ['swift','rust']:
    bc=out/f'{name}-ownership.bc'; library=out/f'{name}-ownership.dylib'
    exports=['handle'] if name=='swift' else ['rc','arc']
    if name=='swift':
        run(swift+['-target','arm64-apple-macosx14.0','-emit-bc',HERE/'ownership.swift','-o',bc])
        link=[args.clang,'-O3','-target','arm64-apple-macosx14.0','-dynamiclib','-Wl,-dead_strip']
        for suffix in ['create','retain','release','value']:
            link.append('-Wl,-exported_symbol,_handle_'+suffix)
        run(link+[bc,'-o',library])
    else:
        # A crate's emitted IR alone does not include its alloc dependency closure.
        # Let rustc link and optimize the complete no_std + alloc ownership module.
        run([x for x in rust if x!='--crate-type=lib']+['--crate-type=cdylib','-C','lto=fat',
            '-C','codegen-units=1',HERE/'ownership.rs','-o',library])
    for prefix in exports:
        result['ownership'][name+'_'+prefix]=[json_run([out/'native_runner',library,'ownership',prefix]) for _ in range(args.trials)]
    result['artifacts'][library.name]={'bytes':library.stat().st_size,
        'dynamic_dependencies':run(['otool','-L',library])[0]}

def reference_kernel(seed,count):
    x=seed
    for _ in range(count):
        x=(x*1664525+1013904223)&0xffffffff; x ^= x>>13
    return x
def reference_scan():
    value=2166136261
    data=[(i*2654435761)&0xffffffff for i in range(16384)]
    for round in range(64):
        for item in data: value=((value^item^round)*16777619)&0xffffffff
    return value
expected_kernel=reference_kernel(123,8000000);expected_scan=reference_scan()
for key,runs in result['runs'].items():
    for trial in runs:
        assert trial['kernel_result']==expected_kernel,(key,'kernel')
        assert trial['scan_result']==expected_scan,(key,'scan')
        assert trial['boundary_result']==999999,(key,'boundary')
for values in result['startup'].values():
    for runs in values.values():
        for trial in runs: assert trial['result']==reference_kernel(123,1000)
result['correctness']={'kernel':expected_kernel,'scan':expected_scan,'all_results_match':True}
result['git_commit']=run(['git','-C',ROOT,'rev-parse','HEAD'])[0]
result['source_sha256']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in HERE.iterdir() if p.is_file()}
(out/'results.json').write_text(json.dumps(result,indent=2))
print('Results:',out/'results.json',flush=True)
for key,runs in result['runs'].items():
    print(key,{metric:round(statistics.median(statistics.median(x[metric]) for x in runs),4)
               for metric in ['kernel_ms','scan_ms','boundary_ns']})
for key,runs in result['ownership'].items():
    print(key,{metric:round(statistics.median(statistics.median(x[metric]) for x in runs),4)
               for metric in ['retain_release_ns','alloc_release_ns']})
