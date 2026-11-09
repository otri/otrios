<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB library L1 implementation

Date: 2026-11-09. Status: **implemented, signed, deployed and focused live-qualified; awaiting user review**.
Branch: `feat/oib-library-l1`, based on canonical D2 commit `7b88756`.

This implements the data-only L1 selected in
[RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md). Component Wasm ABI L2,
new renderer/input primitives, typography/images and XR remain separate work.

## Scope and responsibilities

The package role owns distinct library metadata, canonical integrity/signature
admission and rejection tests. The graph role owns exact import bindings,
resolution, compatibility checks and read-only imported definitions. The state
role owns expected-generation baseline adoption, causal-state preservation and
recovery. Integration owns authoring/publication tooling, authority activation,
protocol coordination and live qualification. Terra supplied the read-only
release/adoption inventory.

The signed app contains independently signed library trees under
`Dependencies/<revision>.otlib/`. This reuses the established complete signed
app/release closure and unchanged-host install workflow. An app manifest pins
exact revisions. Canonical local definitions and import bindings remain separate
from verified immutable library bodies transported to clients; the compiler
resolves them into disposable renderer output.

## Required checks

- Reject package kind/version confusion, invalid signatures/hashes, missing or
  mismatched exports, changed class contracts and aggregate budget violations.
- Preserve document-local definition IDs, node/property IDs, occurrence ancestry,
  semantic epoch, collection identity, business values and causal overrides.
- Reject edits to imported definitions while allowing local wrappers, instance
  overrides and admitted app connections.
- Adopt through a separate expected-generation/expected-lock operation. Validate
  and compile the candidate before one durable publication. Rejected/stale or
  failed persistence retains the prior complete baseline.
- Recover the exact accepted lock after authority restart; retain coherent pinned
  readers and bound slow-reader/retired-closure retention.
- Publish a signed library-only style/default revision and compatible rollback
  with unchanged host binary/module hashes on the existing Frame/Web system.
  Activation waits for confirmed edits and completed gestures/composition.

State protocol advances to v30 for the new baseline representation; the opening
release-control envelope remains v1. Existing D2 source equality remains strict
outside the explicit compatible-library adoption path.

## Results

Local implementation checks now include 19 signed library/app package cases,
three unsigned-library cases, ten import/resolver cases, ten adoption cases and
sixteen existing structural-batch cases. Two real authority process cases passed
upgrade/restart/rollback and held-offer coalescing with synthetic already-verified
bodies;
this is separate from signature/publication acceptance. The actual Client held
slider and final receipt while a new baseline arrived, then activated it after
confirmation. Nineteen ordinary state tests, nine server tests and nineteen
shared-session/update Node tests passed. CLI source/pack/sign/bind/verify passed
using a production library, as did ordinary-build pin inheritance and explicit
newer-pin preservation. The signed ARM/Wasm build and actual live results are
recorded below.

The packer reuses the already locked Serde and serde_json packages for closed,
typed JSON authoring. Serde becomes an explicit optional dashboard dependency;
Cargo.lock adds only that dependency edge, with no new resolved package/version.

## Library authoring workflow

`canvas-bundle` provides `library-source`, `library-pack`, `library-inspect`,
`library-sign`, `library-verify` and `library-bind`. Library JSON is authoring
input; the package contains canonical bounded FlexBuffers, exact integrity and
an independent publisher signature.

To extract reusable local definitions from a verified app, write a source spec:

```json
{
  "library": "<publisher-public-key-sha256>:oib-2d",
  "version": "0.1.0",
  "exports": [{"id": 1, "name": "SettingsPanel", "definition": 2}]
}
```

The actual definition IDs come from the app source, not these illustrative
numbers. `library-source APP.otapp SPEC.json SOURCE.json --key PUBLIC --openssl
PATH` includes each export's complete reference closure. Edit the resulting
source's appearance/defaults, then pack and sign:

```sh
canvas-bundle library-pack SOURCE.json candidate.otlib
canvas-bundle library-sign candidate.otlib signed.otlib --key PRIVATE --openssl PATH
canvas-bundle library-verify signed.otlib --key PUBLIC --openssl PATH
```

`library-bind APP.otapp signed.otlib BINDINGS.json candidate.otapp --key PUBLIC
--openssl PATH` creates an unsigned app candidate with the exact dependency pin.
Bindings explicitly map library definition IDs into the app's stable definition
namespace. For a later revision, keep the same binding file; the tool rejects an
implicit rebind. Sign the app candidate with the ordinary `sign` command and
publish it through `scripts/install-app.py`. That workflow reuses the qualified
host assets and verifies their hashes before deployment.

Exact pins select reproducible bytes; they do not define an update policy.
A future compatible-update policy can select a newly published revision and
record the accepted exact pin through the same validation/adoption path. L1
requires an explicit app dependency update and has no floating `latest` resolver.


For repeated library publication, `scripts/publish-oib-library.py --runtime RUNTIME
--source libraries/oib-2d/source.json --bindings libraries/oib-2d/designer-bindings.json
--app oib-designer --checker CHECKER --openssl PATH` stages a complete signed
app/library release without compiling a host. Add the existing deployment config
with `--deploy-config CONFIG` to activate it. The script checks the reviewed
baseline, freezes the requested source/bindings, verifies both package signatures,
and reuses the ordinary installer's unchanged-host closure checks.


## Signed live qualification

Implementation commits are `b51f60c` and the coalesced-live-state correction
`e1b7882`. The corrected active release is
`ca310b5ca4a75a43b7891a1492085bfae35a04bf29cddff54d4f8160ea39d8e1`,
with state protocol v30 and release-control v1. Designer imports the independently
signed [OIB 2D library](../libraries/oib-2d/README.md), revision
`3ce95edfa948a4d0299e869c7b7f070e4419fb37fea3f7955669cae724113aba`.
Only the initial format/import cutover reset Live UI and Designer (registry
76/77), preserving their installed v2 business schemas. No later update or code
correction reset either collection.

Actual commands used the existing private Rust 1.90.0 build environment:

- `cargo check -p otrios-dashboard --bin canvas-bundle --bin dashboard-server --features server,canvas,host-sync --locked --offline` passed.
- `cargo test -p otrios-core --features canvas,merkle,signed-bundles --lib canvas::state --locked --offline`: 19 passed.
- `cargo test -p otrios-dashboard --features server,canvas,host-sync --bin dashboard-server --locked --offline`: 9 passed.
- Core library Clippy and focused dashboard-bin Clippy with `-D warnings` passed.
- `cargo test -p otrios-core --features signed-bundles --lib --locked --offline -- library::tests bundle::tests`: 19 passed; the unsigned `--features canvas --lib library::tests` run passed 3 cases.
- `cargo test -p otrios-core --features canvas --test oib_library_imports --locked --offline -- --nocapture`: 10 passed.
- `cargo test -p otrios-core --features canvas --test oib_library_adoption --locked --offline -- --nocapture`: 10 passed.
- `cargo test -p otrios-core --features canvas --test oib_edit_batches --locked --offline -- --nocapture`: 16 passed before the live-projection-only correction.
- With the freshly built debug `dashboard-server` supplied in `OTRIOS_SPACES_SERVER`, `cargo test -p otrios-core --features canvas --test spaces_process d1::l1_ --locked --offline -- --nocapture`: 2 actual process cases passed.
- `cargo test -p otrios-core --features canvas --lib compatible_library_activation_waits_for_held_slider --locked --offline -- --nocapture`: the actual Client gesture/receipt test passed.
- `scripts/watch-dashboard.py --once --stage-only` built and signed the ARM/Web
  host. `scripts/install-app.py` staged the initial exact library pin and the
  established full deployment script activated the complete verified release.
- `scripts/publish-oib-library.py` performed update, compatible rollback and
  return to the current source through the existing deployment config. Each
  library-only transfer was **153,600 bytes**. No host compilation command runs
  in this publication path; native/browser asset hashes and the actual authority
  executable hash stayed equal across those three publications.

The source changed the accent to teal and untouched slider width from 440 to
400. An explicit Audio occurrence width of 360 stayed unchanged; all four
business values, collection, semantic epoch and original roots 20/30 survived
update, rollback and return. Browser slider values stayed 40/70. During a
synthetic DOM pointer gesture through the real browser handlers, generation 3's
candidate arrived while generation 2's 440 px scene remained presented. Release
and confirmation activated the new 400 px scene. Platform pointer capture was
skipped only for the synthetic pointer ID; physical Frame touch separately
opened Designer with the calibrated device unchanged.

Native Frame and a current ARM client agreed on revision 5, the library pin,
retained values and 360/400 widths after the corrected full host deployment
restarted the authority. No reset was needed for that correction. Browser and
native readiness passed, Monitoring resumed, and no pixels were exported. Native
RSS was **60,664 KiB** at final acceptance; this is one whole-process observation,
including fonts and GLES resources, not a library-only memory benchmark.
Imported UIs retained zero entries in the optional client UI history cache.

Live leases validate coalesced snapshots, including skipped library adoptions
and ordinary writes in the same frame. Authority journals validate each exact
adoption predecessor separately. Two repros initially failed before that
validation split; ten adoption cases and both real-process cases then passed,
including malformed/stale live witness rejection and strict combined-journal
delta rejection. The corrected code was signed and deployed before final checks.

A compatible code-only fallback is held privately. No app-data backup, migration,
new identity, trust root or service was introduced. Full `scripts/check.sh` and
the separate Linux shared-memory IPC qualification were not run for L1. Accepted
builder maintenance awaits user review. Component behavior ABI L2, D3/D4,
complete OIB data transfer, images and XR remain deferred.
