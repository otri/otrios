# OTriOS transport comparison experiment

Compare LLVM bitcode followed by native compilation against WebAssembly followed by Wasmtime compilation. Use allocation-free Embedded Swift and Rust no_std kernels, plus a separate native ownership experiment for Embedded Swift ARC, Rust Rc, and Rust Arc.

This is an isolated experiment, not the OTriOS runtime or a change in the runtime language decision. Read the [measured report](../../docs/experiments/transport-comparison.md) before interpreting the numbers.

## Prerequisites and reproduction

The first harness targets **Darwin arm64**. It fails explicitly on other hosts because its native link flags, library format, and RSS units must be ported and validated first.

Use Python 3, Rust 1.90.0 with the `wasm32-unknown-unknown` target, a Swift.org 6.4.1 toolchain including Embedded Swift libraries, Apple's clang 21, and the Wasmtime 49.0.3 C API distribution for aarch64 macOS. The Xcode compiler alone may lack the Embedded Swift standard-library modules. Download tools from their official distributions; the harness does not install them or alter global toolchain settings.

```sh
rustup target add wasm32-unknown-unknown --toolchain 1.90.0
python3 benchmarks/transport/run.py \
  --swiftc /path/to/swift-6.4.1/usr/bin/swiftc \
  --wasmtime /path/to/wasmtime-v49.0.3-aarch64-macos-c-api \
  --clang /path/to/clang
```

Results, exact commands, generated artifacts, and partial checkpoints go into ignored `bench-results/transport/`. Both runners compile with warnings denied. A full run checks all compute and scan results against independent Python implementations, checks startup outputs, and verifies ownership reads during allocation and reference churn.

## What is measured

- Producer time: source to native-target LLVM bitcode, or source to Wasm.
- Receiver time: bitcode to a native shared library, or Wasm to a serialized Wasmtime native cache.
- Warm execution: 8 million dependent integer iterations and 64 rounds over a 64 KiB buffer.
- Boundary calls: one million no-op calls through a native function pointer or the checked Wasmtime C API.
- Startup: a fresh process with warm OS filesystem caches, including compilation or using the generated native cache.
- Ownership: native retain/release pairs and allocate/read/release operations, with the platform allocator, on one thread.

The process trial count defaults to seven. Execution and ownership trials each use two warm-ups and seven timed inner samples. Native and Wasm execution modes alternate within each language to reduce ordering bias. Summaries use the median of each process's inner median, then the median across processes. Raw observations are retained.

## Boundaries of the experiment

The Wasm scan reads a host-populated linear memory. It does not demonstrate an OTriOS read-only mmap, cross-process zero-copy access, capabilities, subscriptions, or object lifetime. The C ABI ownership functions keep class and reference-count layouts private, but their pointer handles are local-process test fixtures, not wire capabilities.

No Wasm ownership benchmark or multithreaded contention test is included. Rust Rc is single-threaded; the tested Embedded Swift ARC uses atomic reference counts, making Rust Arc the closer comparison. The Rust ownership module uses no_std plus alloc and explicitly links the platform allocator; a crate-only bitcode file does not include that dependency closure, so rustc performs final linking for this separate ownership test.

Only trust `.cwasm` files produced locally by this harness. Wasmtime native-cache deserialization is not safe for arbitrary downloaded input. Native dylibs likewise execute native code. Transport signing, validation, sandbox enforcement, and dependency closure are future OTriOS work.

## Toolchain findings

Swift 6.4.1 crashed when emitting LLVM bitcode for the freestanding Wasm probe, reporting “Unexpected anonymous function when writing summary.” Direct Wasm object emission succeeded and is the path used here. Native Swift bitcode emission succeeded. This is a toolchain observation, not evidence that Wasm execution is incorrect.

The larger Swift native bitcode includes Embedded Swift runtime definitions that the receiver's native linker can strip. Wasm linking strips unreferenced code before transport. Artifact size therefore describes these concrete packaging pipelines, not an inherent lower bound for either IR.

## Official sources

[Swift installation](https://www.swift.org/install/macos/), [Embedded Swift and Wasm SDKs](https://www.swift.org/documentation/articles/wasm-getting-started.html), [Rust no_std](https://doc.rust-lang.org/embedded-book/intro/no-std.html), [LLVM bitcode](https://llvm.org/docs/BitCodeFormat.html), [LLVM target data layout](https://llvm.org/docs/LangRef.html#data-layout), [Wasmtime C API release](https://github.com/bytecodealliance/wasmtime/releases/tag/v49.0.3), and [Wasmtime module serialization](https://docs.wasmtime.dev/examples-serialize.html).
