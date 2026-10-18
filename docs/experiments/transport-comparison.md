# OTriOS transport and ownership measurements

The first measured comparison supports Wasm as a candidate for portable transport: its payloads are small, and these compute kernels execute close to native speed. It also identifies a cost worth designing around: many tiny calls through the checked Wasmtime API are substantially more expensive than native function-pointer calls. Embedded Swift and Rust no_std both produce small native modules; this experiment does not support assuming that Rust necessarily brings a heavy runtime.

These are measurements of small kernels on one Mac. They do not select the OTriOS runtime language or establish Linux, cross-process, concurrency, or production security behavior.

## Host and method

Measured October 18, 2026 on an Apple M2 Max, 12 logical CPUs, 64 GiB RAM, Darwin arm64. Toolchains: Swift.org 6.4.1 in Embedded mode; Rust 1.90.0 with no_std; Apple clang 21; Wasmtime 49.0.3 with Cranelift's speed optimization and parallel compilation disabled. Native deployment target is macOS 14.0. Wasm linker stack size is 64 KiB.

Each mode has seven fresh process trials. Execution trials have two warm-ups and seven measured samples per process. Values below are medians of process medians. Filesystem caches are warm; “uncached” means no compiled module cache, not cold disk I/O. Compiler invocations are independent processes. No network transfer or signature verification is timed.

Source, reproduction instructions, and raw observations are in [the benchmark directory](../../benchmarks/transport/README.md). [Raw results](../../benchmarks/transport/results/m2-max-2026-10-18.json) include every sample, artifact hashes, tool versions, the baseline commit, and source hashes. The baseline is scaffold commit `e53c4e76adf128bcdb4ee94cbe37209267b40e7f`; the experiment source hashes identify the tested additions.

## Transport and native cache sizes

The compute module exports an integer kernel, a scalar buffer scan, and a no-op. It allocates no heap objects. Debug information is omitted, and Wasm custom/debug sections are stripped.

| Artifact | Embedded Swift | Rust no_std |
| --- | ---: | ---: |
| Native-target LLVM bitcode | 20,992 B | 3,776 B |
| Same bitcode compressed with gzip 9 | 11,240 B | 3,040 B |
| Wasm transport | 570 B | 577 B |
| Same Wasm compressed with gzip 9 | 378 B | 339 B |
| Native dylib cache from bitcode | 16,952 B | 16,840 B |
| Wasmtime serialized native cache | 67,824 B | 50,976 B |

Swift bitcode carries runtime definitions that native linking removes; Wasm linking removes unused code before transport. A bitcode packaging pass that strips unused definitions could change the size comparison and needs its own compatibility test. Native cache containers have format, alignment, and metadata overhead; they are not direct measurements of instruction bytes.

LLVM bitcode is the binary encoding of LLVM IR. Textual `.ll` is another representation of that IR, not a more portable execution format. Ordinary generated IR embeds target data-layout assumptions. This experiment compiles native-target bitcode for the same architecture; it does not demonstrate one bitcode artifact working safely on unrelated architectures. See [LLVM's bitcode format](https://llvm.org/docs/BitCodeFormat.html) and [data-layout requirements](https://llvm.org/docs/LangRef.html#data-layout).

## Compilation and startup

All times are milliseconds. Receiver process time includes reading input and writing its native cache; the Wasm receiver also constructs an engine and serializes its result. Wasmtime's internal compiler time is shown separately.

| Measurement | Embedded Swift | Rust no_std |
| --- | ---: | ---: |
| Producer source to bitcode | 256.31 | 48.96 |
| Producer source to Wasm | 272.90 | 68.01 |
| Receiver bitcode to native dylib, process wall time | 66.83 | 39.65 |
| Receiver Wasm to native cache, process wall time | 7.56 | 5.93 |
| Wasmtime internal compilation | 1.72 | 1.11 |
| Native cached startup, process wall time | 3.14 | 3.29 |
| Wasm startup with compilation, process wall time | 5.86 | 6.08 |
| Wasm cached startup, process wall time | 4.40 | 4.96 |
| Native module load within running process | 0.153 | 0.162 |
| Cached Wasm deserialize and instantiate within running process | 0.122 | 0.100 |

The last Wasm row excludes engine construction. A persistent OTriOS authority could amortize engine startup. These tiny modules do not establish compilation behavior for large applications or lazy function loading. The Wasm pipeline uses Cranelift rather than LLVM for receiver code generation, so this compares usable pipelines rather than isolating file format from compiler implementation.

## Execution and boundary costs

The integer kernel performs eight million dependent wrapping integer updates. The scan performs 64 rounds over 64 KiB, including a multiply for every element; it is a scalar workload rather than a pure memory-bandwidth test.

| Path | Integer kernel | Scalar scan | One no-op boundary call |
| --- | ---: | ---: | ---: |
| Embedded Swift native from bitcode | 11.789 ms | 1.532 ms | 0.893 ns |
| Embedded Swift through Wasmtime | 11.837 ms | 1.524 ms | 80.505 ns |
| Rust native from bitcode | 11.662 ms | 1.527 ms | 0.860 ns |
| Rust through Wasmtime | 11.727 ms | 1.531 ms | 80.468 ns |

The compute differences are small enough that these runs do not establish a useful language ranking. The checked Wasmtime host API clearly costs more than a direct native function pointer for trivial calls. This is a host-to-guest C API result, not a measurement of every Wasm function call or every possible runtime embedding. Batch object operations and avoid one boundary crossing per field.

Peak resident memory during the cached compute process was about 1.72 MiB for the native runner and 4.45 MiB for Swift/Wasmtime or 4.36 MiB for Rust/Wasmtime. These totals include the test process, loaded libraries, engine, and buffers. Guest linear memory after setup was 192 KiB for Swift and 128 KiB for Rust; differing heap offsets and page rounding contribute to this gap. They do not measure a complete application or per-object allocation cost.

## Native ownership boundary

Opaque pointers expose create, retain, read, and release operations through a C ABI; object layouts stay private. These are local test handles. Each allocation operation includes allocation, a checked read, and final release through exported functions. Retain/release timing uses an already allocated object and includes two exported function calls.

| Ownership implementation | Retain plus release | Allocate, read, release | Native module file |
| --- | ---: | ---: | ---: |
| Embedded Swift ARC | 5.90 ns | 27.73 ns | 34,088 B |
| Rust Rc, single-threaded | 3.86 ns | 24.65 ns | 35,672 B shared Rc/Arc module |
| Rust Arc, atomic | 4.12 ns | 23.99 ns | Same module |

The Rust native module includes both ownership variants. Its file size is not an isolated Arc-versus-Swift comparison. Both modules link only to `libSystem`; neither needs a separate Swift or Rust dynamic runtime library here. Rust's tested module uses no_std plus alloc and the platform allocator. Swift's generated retain and release operations use atomic instructions, making Arc the closer semantic comparison. All timing is single-threaded and uncontended.

ARC may offer the preferred programming model, but this run does not establish that it is intrinsically lighter than Rust ownership. Both languages can wrap a stable retain/release boundary. Neither native reference-count layout is suitable as an on-wire or cross-process contract. OTriOS still needs authority-owned handles, pinned snapshots, leases or another lifetime protocol, and explicit capability checks.

## Decision and next experiment

Keep both pipelines available for exploration and keep the runtime language decision open. Use a language-independent capability and lifetime ABI. Wasm is a promising portable application-module transport; native-target bitcode remains useful for controlled compilation and optimization experiments.

Both formats can travel in the same OTriOS content-addressed module envelope. A signed manifest should bind the code hash, format and feature profile, interface versions, capability imports, dependencies, and chunk hashes. Verify each required chunk before compiling or executing it. Native cache identity should additionally bind the compiler/runtime version, target CPU features, ABI, and compilation options. Code identity and permission to perform an operation remain separate checks.

MLIR bytecode is worth a later experiment if OTriOS needs to preserve domain-specific operations, such as merge primitives, until hardware selection. Its encoding includes dialect and version information, so we would still need to define the accepted dialects and their execution semantics. It is not measured here. See [the MLIR bytecode format](https://mlir.llvm.org/docs/BytecodeFormat/).

The next decisive test is a real object boundary: a native host owns immutable mapped bytes and both Swift and Rust modules read them through a versioned interface. Compare batching, explicit copies into Wasm memory, and a carefully specified mapping mechanism where supported. Repeat on Linux x86_64 and aarch64, add multithreaded reference-count contention, test capability revocation and failed calls, and measure larger module compilation and authenticated streaming before selecting a production format.

Swift 6.4.1's freestanding Wasm bitcode emission crashed in the probe's summary writer; direct Wasm object emission succeeded and is used by the harness. This is recorded as a toolchain limitation, not silently replaced with standard Swift or a different language mode.
