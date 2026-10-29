<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0026: Target-specific Wasm compilation

Date: 2026-10-29. Status: proposal; no backend selected or integrated. Depends on
[RFC 0025](0025-standalone-app-bundles.md). B1 offline bundle packaging is the
completed implementation scope; compilation remains a later experiment.

## Goal and scope

Keep portable Wasm as immutable app content. Compile that exact Wasm for the
host at installation, background preparation or first launch, then reuse a
disposable host-owned artifact. Compiler/CPU changes do not change app identity.
Execution remains runtime-managed, with bounded memory, imports, stack,
interruption and host command validation. Compiled bytes are not permission grants.

Today identical pure reducer bytes run in Wasmi 1.1.0 on native ARM and inside the
browser's outer Wasm host. ABI v1 has no WASI/filesystem/network imports. The
compositor is already native Rust on Frame; compiling reducers does not by itself
accelerate layout, text, GPU or authority work. Measure app-level benefit before
replacing the safe interpreter.

## Candidates

| Host | Candidate | Qualification needed |
| --- | --- | --- |
| RK3126 Frame, ARMv7 hard-float Linux | WAMR AOT through wamrc/LLVM | Exact CPU/features, hard-float ABI, bounded interruption, FFI boundary |
| AArch64/x86-64 native hosts | Wasmtime/Cranelift AOT or local compilation | Compiler/runtime/configuration identity and pinned Rust compatibility |
| Portable baseline | Current Wasmi | Preserve bounded semantics |
| Browser | Current nested Wasmi initially | Direct engine guest execution is separate; native caches are browser-owned |

As checked 2026-10-29, [Wasmtime platform support](https://docs.wasmtime.dev/stability-platform-support.html)
has no 32-bit Cranelift target. Pulley can interpret on 32-bit ARM, but is not ARM
native compilation. WAMR documents ARMv7 configurations and target/ABI/CPU options;
its [AOT guide](https://github.com/wasm-micro-runtime/wasm-micro-runtime/blob/main/doc/build_wasm_app.md)
recommends matching compiler/runtime versions. Those are candidate capabilities,
not evidence for this Frame, Rust 1.90 compatibility or performance. Re-check
pinned versions, licenses and feature support before integration.

## Cache request and identity

Version the canonical compilation request and SHA-256 it, with golden vectors.
It binds original Wasm hash, guest ABI/host contract, compiler backend/build
(including LLVM), runtime build/ABI/features, target triple/pointer width/byte
order/calling convention, CPU model/normalized required features, enabled Wasm
proposals, optimization options, bounds/trap/stack/interrupt/metering settings
and instrumentation version. App version or file path alone is insufficient.

Verify local CPU features against requirements before loading. Runtime or safety
configuration changes invalidate the cache. Eviction changes launch cost only;
it cannot alter app identity, admission or confirmed state.

## Pipeline and trust

```text
trusted portable bundle + compatible Wasm
  -> bounded compilation request
  -> compatible cache hit OR isolated compiler worker
  -> authenticated artifact + full request binding
  -> host compatibility check
  -> fresh runtime instance with current grants and limits
```

Start by cross-compiling on the existing build machine so LLVM stays off Frame.
An uncached device target can trigger preparation without adding native binaries
to the app bundle. Evaluate local/offline compilation where resources permit.
Do not create a network compilation service for the first spike. Compiler workers
need queue, input/output, CPU, memory and deadline limits.

A remote compiler has its own trusted identity. Its signed artifact receipt must
bind artifact hash, canonical request hash, original Wasm hash and compiler build.
An app signature over Wasm does not authenticate arbitrary native output. A hash
or successful deserialization cannot establish compiler provenance.
[Wasmtime precompilation](https://docs.wasmtime.dev/examples-pre-compiling-wasm.html)
requires trusted serialized artifacts. Apps must not write to executable caches;
matching a filename never authorizes loading.

Never disable bounds checks, command validation or interruption for speed. Prove
finite per-call execution in AOT mode, including infinite loops and recursion.
If equivalent fuel is unavailable, specify and qualify a bounded alternative
before admission. Equal fuel counts across engines are not assumed. Host memory
and authority state stay accessible only through admitted interfaces.

Compilation failure preserves the active instance. Use the bounded interpreter
when compatible, otherwise report unavailability. Activate fresh instances only
at safe boundaries, never under an active gesture/reducer. Cache reuse must not
restore mutable guest memory from another app/state scope.

## C1 experiment (deferred)

1. Pin/review WAMR/compiler licenses and wrapper/build requirements. Preserve
   Rust 1.90. Any FFI/unsafe loader requires a narrow LGPL boundary, documented
   invariants and a reviewed local lint amendment before implementation.
2. Cross-compile the same known Wasm for measured Frame architecture/features.
   Compare outputs/errors with Wasmi in real separate processes on the physical
   device. A successful build is not device acceptance.
3. Cover reducers and expensive bounded workloads: malformed modules, denied
   imports, forbidden writes, bad commands, traps, recursion, infinite loops,
   memory/table limits, corrupt/swapped artifacts and CPU/runtime/config mismatch.
4. Measure builder cold compile/peak memory, transfer bytes, cold/warm load and
   instance time, Frame RSS and p50/p95/p99 call cost. Separate startup benefit,
   steady-state benefit and metering overhead.
5. Clear caches, reject stale artifacts and demonstrate failures preserve state.
   Production admission still requires RFC 0025 signing/lifecycle qualification.

Select no universal backend in advance. A later C2 compares Wasmtime on a 64-bit
host and defines the backend interface from measured semantics. General WASI,
component/WIT interfaces, effects, automatic hot tiering and native plugins need
separate RFCs.

## Rollout

Keep release-control v1 independent of decoder/compiler/state failures. Notify
upgrades only after complete signed content is active. Preserve confirmed state
during rollback; derived caches are disposable. Production changes require the
existing signed-release verification, real browser/Frame acceptance and last-good
rollback workflow. This proposal changes no state protocol or live deployment.
