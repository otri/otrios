<!-- SPDX-License-Identifier: Apache-2.0 -->
# Offline app bundle qualification — 2026-10-29

Scope: [RFC 0025](../rfcs/0025-standalone-app-bundles.md) B1, on feature branch
`feat/installable-app-bundles`, macOS aarch64, pinned Rust 1.90.0. This qualifies
offline authoring/integrity inspection only. No installation, signature trust,
native AOT execution, physical Frame acceptance or Linux shared-memory PoC
acceptance is claimed. Existing release/state/control formats are unchanged.

## Implementation

The new `canvas-bundle` tool exports one generated catalog package to a flat
`.otapp` tree and inspects its versioned Info.flex plus complete SHA-256 file
closure. The LGPL core owns validation; Apache interfaces own contract types;
the Apache CLI owns bounded filesystem operations. No new dependency, lockfile
change or unsafe code was introduced. Guest inspection never instantiates or
executes code, including Wasm start functions.

All output explicitly reports `publisher: unauthenticated; installable: no`.
Current app dependency strings retain descriptive source/SDK/runtime hints;
inspection does not resolve them or grant capabilities. Monitoring preserves its
private catalog classification. B2/B3 installation/admission and
[RFC 0026](../rfcs/0026-target-wasm-compilation.md) native compilation are deferred.

## Focused validation

```sh
cargo test -p otrios-core --lib --features canvas --locked --offline bundle::
cargo test -p otrios-dashboard --features canvas --test bundle_cli --locked --offline
bash scripts/check.sh
cargo check -p otrios-web-host --target wasm32-unknown-unknown --locked --offline
```

The focused core suite passed **8 tests**: pinned digest and SHA-256 vector;
tampering/missing/unlisted content; optional metadata versus unknown required
features; wrong primitive types/headers; duplicate keys and catalog mismatch;
schema/UI/code binding; no guest execution; path/case collisions; exact size/count
limits; and truncated/2,048 seeded mutated metadata inputs without panics.

The filesystem suite passed **1 real CLI process test**, exercising export,
inspection, byte-identical repeated output, private/no-code and public/reducer
packages, refusal to overwrite, unsafe app IDs, tampering, extra files, empty
directories, symlinked files/roots/source files and staging cleanup. It is an
offline build-tool process test, not Linux IPC acceptance.

The first sandboxed required check was stopped by an OS permission denial in
the existing collection-process socket test. It was rerun with local socket and
child-process permissions. The rerun of **bash scripts/check.sh passed**, including
formatting, strict workspace/all-target Clippy, workspace Rust tests (including
the new bundle suites) and the required Python suites. This was a macOS run;
the Linux-only touch script was not executed.
The browser Wasm target check also passed. This checks compilation of the shared
core with the new module, not browser execution or AOT compilation of an app.

## Existing package exports

Generated the current packages in an isolated ignored output root, copying only
the reducer WAT source needed by canvas-pack, then ran for each catalog ID:

```sh
target/debug/canvas-pack "$APP_ROOT"
target/debug/canvas-bundle export "$APP_ROOT" "$APP_ID" "$OUTPUT.otapp" --revision 1
target/debug/canvas-bundle inspect "$OUTPUT.otapp"
```

All **11** exports and inspections succeeded. They cover UI v2–v6; only Component
Playground contains reducer code; Monitoring is private. Export/inspection reports
matched. The CLI process test separately verifies repeat-export byte equality.

| Package | Files | Total bytes |
| --- | ---: | ---: |
| home | 4 | 27,481 |
| brass-relay | 4 | 327,890 |
| test-playground | 4 | 65,627 |
| component-playground | 5 | 56,495 |
| layout-lab | 4 | 59,167 |
| layout-document | 4 | 31,934 |
| layout-text-scroll | 4 | 331,741 |
| input-lab | 4 | 350,222 |
| animation-lab | 4 | 30,693 |
| three-d-lab | 4 | 30,675 |
| monitoring | 4 | 11,333 |

The [machine-readable record](data/app-bundles-2026-10-29.json) retains every
bundle digest. These are byte identities, not signed releases. Outputs remain
disposable build artifacts under `target/`; nothing was uploaded or deployed.
