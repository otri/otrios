<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB library L2: bounded component behavior

Date: 2026-11-10. Status: **implemented, signed, deployed and focused live-qualified**.
Branch: `feat/oib-component-behavior-l2`, based on `7b4ebc8`.

The user selected the bounded [RFC 0036 L2](rfcs/0036-upgradable-oib-component-libraries.md#l2-component-behavior-boundary)
slice: one library-owned Slider, a generic typed Wasmi dispatcher, signed guest
publication and compatible rollback without rebuilding hosts. D3/D4, typography,
assets, XR and general module/AOT execution remain separate work.

The final signed Frame/Web run passed guest-only behavior update and rollback
through catalogue activation with unchanged hosts. Browser session/local mode,
native process, app values, Width 420 override and semantic identities survived.
See [final acceptance](experiments/data/oib-library-l2-2026-11-10/final-acceptance.json)
and the detailed final results below. Earlier failed integration trials are
retained separately to explain the fixes and their regression coverage.

## Frozen boundary

The [component ABI 1](OIB_COMPONENT_BEHAVIOR_ABI.md) freezes export names,
little-endian typed input/output records and golden vectors. It carries a stable
behavior ID, complete semantic object address, normalized pointer phases/local
coordinates, declared effective properties and host-owned local state. Guests
return at most 16 commands for declared locals/outlets and their admitted capture.
They cannot select app fields, write scene trees or issue structural edits.

Library metadata/graph version two explicitly requires the component behavior
feature and `Code/component.wasm`. Version-one packages remain data-only and
byte-identical. The complete signed file manifest covers exact guest bytes;
resolved app transport matches the nested independently verified package exactly.
Export contracts include behavior identities, declared source paths, local types,
outlet types and capture policy, while excluding guest bytes and local default
values. Guest-only revisions therefore change the exact pin without silently
changing admission or namespace bindings.

Code is bounded to 64 KiB, with one fixed 64 KiB memory, no imports/WASI/tables,
recursion 32, stack height 1024 and 10,000 cumulative fuel per call, including
initialization and ABI exports. Input/output records are each at most 8 KiB.
Eight occurrence leases include active and staged replacements in one Client
runtime. Existing 64 KiB total seed/resident and 24 KiB local source budgets remain;
the code ceiling does not promise that every 64 KiB guest fits an app closure.

Each call uses fresh scratch memory/globals. Immutable modules share exact
SHA-256 identities; each retained compiled revision owns its Engine, so retiring
its last reference also releases Wasmi's compiled-code arena. No mutable guest
memory is shared across occurrences. Compatible local state resumes by full
semantic address, behavior identity and declared type.

The initial runtime effect surface is existing admitted numeric Set/Slider and
Boolean Toggle/Activate. Pointer guests receive generic local x/y; their policy
is library code. Outlets still pass existing property constraints, app bindings,
field/effect grants, intent coalescing and durable receipts. This adds no widget
specific ClassKind or action variant. Host-local state supports interaction logic;
this slice does not add a general visual expression binding for guest locals.

The whole output is decoded and validated before any app/local/capture effect.
A trap, malformed or unauthorized result cancels only its affected pointer and
increments a bounded diagnostic counter. Current controls remain usable. Candidate
resources are prepared before presented UI changes; source/guest activation still
waits for finished gestures and confirmation. Ordinary paint updates retain their
existing guest set and the content/layout invalidation optimization.

State protocol is v31 for executable-library transport; release-control opening
v1 is unchanged. App behavior ABI 1 remains independent.

Signed release manifests also advertise component ABI 1 and carry per-app update
contract fingerprints computed from verified packages. The fingerprint binds app
identity, private delivery policy, state references/schema bytes, capabilities,
required features, entry UI and the declared app-guest path. Content revisions
and exact library pins are excluded. It is a compatibility check, not a signature
or capability. In-place package replacement requires the same app set, contracts,
state bytes, host assets and release metadata, with only admitted package paths
allowed to differ. New registry entries and changed admission contracts retain
the existing safe reload path. Catalogue admission still validates the complete
signed candidate before it can replace the active UI/guest set.

## Slider authoring and acceptance

`libraries/oib-2d/slider.wat` implements the numeric policy, using the same generic
ABI as other possible library behaviors. Its normal STEP is one. Qualification
changes that guest constant to 100, publishes it as a compatible signed library
revision, and verifies the different computed value with unchanged host hashes.
The base guest is restored through compatible rollback.

The first L1-to-L2 attachment adds a new export contract, so it is a new admitted
surface rather than a compatible guest update. It must not bypass the strict L1
adoption check. Any initial disposable Designer collection cutover is recorded
separately from subsequent value/override-preserving guest updates.

## Focused checks before staging

- Public wire codec/golden-vector checks: 7 passed; interfaces Clippy with
  warnings denied passed.
- Bounded Wasmi executor: 8 passed, covering imports, memory/table/export/ABI
  rejection, initialization/dispatch fuel, traps, ranges, malformed full output,
  scratch isolation and active/staged leases.
- Signed library tests: 9 passed (5 existing and 4 L2 package cases), including
  code forgery, missing/undeclared closure, stable semantic contracts across
  changed guest bytes/defaults and unchanged L1 vectors.
- Actual Client L2 tests: initially 6 passed, expanded to 9 after live integration findings. They prove guest-calculated values and exact
  hashes/counters, rejection atomicity for app/local state, cancellation limited
  to the affected pointer, old-guest pinning until release/receipt, retained causal
  overrides and active/staged quota rejection. Direct catalogue replacement proves
  the new guest and latest host locals are active before any Tick. A held reconnect
  fills all eight occurrence leases (two active, four other, two staged) and
  activates after release/receipt without allocating a third set. These use already-verified synthetic
  library bodies; signature/publication checks are separate.
- Existing slider checks: 11 passed, including the 100 Moves/Ticks coalescing
  regression. Structural edit batches: 16 passed. Library adoption: 10 passed.
  These overlapping filters are not a count of distinct tests.
- Shared release/session tests: 19 passed, including code discovery with an
  unknown state protocol and reconnect across epochs. Publisher tests: 3 passed.
- Changed ARM probe and library CLI compiled and passed Clippy with warnings
  denied. Core helper/context checks: **18 passed**, including unbound display-only
  components, authority rejection without state changes, combined default/current
  input budgets and atomic preserved local-state resumption. Core Clippy and the six
  affected Client checks passed again after those hooks landed. The actual
  source JSON and Slider WAT packed and passed library admission.

No full repository preflight, general Linux IPC qualification or unrelated
browser campaign is claimed.

## Initial cutover and live findings

Raw technical records are in
[`experiments/data/oib-library-l2-2026-11-10`](experiments/data/oib-library-l2-2026-11-10/).
No credentials, state files or pixels are exported.

The first executable-library export changed the signed admission contract. Under
the user-approved disposable-data decision, the existing authenticated Spaces
DeleteData operation initialized the Designer collection once (generation 78).
Unrelated apps and the business schema were unchanged. Its new semantic epoch is
`b0056babf7e29bc5998e279e3580906593c9274f2b111aaefe13f82784b666a4`.
Subsequent library trials use that same collection. An explicit Display Width
420 override was confirmed at OIB generation two before the trials.

The initial host build `aa987b4f…` passed signed Frame readiness and Web's v30-to-v31
automatic upgrade. Source review then found the catalogue-preserving Client path
could keep old component guests until a Tick. Commit `de449ca` transfers the
qualified guest set atomically and resumes current host locals before activation;
the corrected `226111bb…` host passed readiness.

The first guest-only update (`7e1b48bc…`) and rollback (`4a6a085e…`) each transferred
153,600 bytes and changed no non-app host asset. ARM and real Web pointer input
575 computed 600 under STEP 100 and 575 after rollback. The Display override,
semantic epoch and component/control identities survived. A synthetic DOM held
pointer retained the old guest until release. Calibrated native evdev input
invoked each guest without errors (the STEP-one native coordinate rounded to
574, within one normalized unit of the requested point).

Those first publications also revealed an integration gap: release classification
did not recognize changed nested signed library package paths, so Web fell back
to a full reload and native restarted. Those results establish unchanged binaries
and durable values/overrides, **not retained host locals**. The final qualification
below uses corrected catalogue-path classification. The initial Web observations
remain in `initial-reload-web.json` so the distinction is explicit.

On the initial rollback host, a separate ARM core process ran 100 pointer
Moves/Ticks in 153,307 µs without scene/composition queries: 100 guest calls,
zero guest errors, zero layout/content work in the held phase. One subsequent
composition took 3,306 µs, resolved layout once through the content-update path
and changed one resource. This measures the complete core input loop and one
composition, not individual Wasmi time, native GLES presentation or photon latency.
Web execution is independently observed through exact guest hashes and call/error
counters; no individual Web Wasmi timing is claimed.

## Release classifier correction

Commit `e869cb1` adds the manifest contract/ABI checks described above and matching
Web/native classifiers. It preserves the registry-install reload boundary while
allowing exact nested library dependency replacement through catalogue admission.
The readiness helper also checks the actual running native executable SHA-256
against the active signed manifest; an in-place update correctly keeps its older
`runtime_release` while `ui_release` advances.

Focused correction checks passed:

- `node --test scripts/test-ui-update.mjs`: 7 passed, including forward/rollback
  library revisions, missing ABI/contracts, malformed contract types, changed
  schema/policy/host bytes, non-app authority changes and invalid package paths.
- `scripts/test_dashboard_release.py`: 42 passed, including four native classifier
  and supervisor cases. The supervisor offers the new catalogue to the retained
  Host and does not construct a replacement process.
- `scripts/test_app_publish.py`: 3 passed.
- Core signed package update-contract test: 1 passed. Verified presentation and
  guest-content changes retain the contract; identity, private policy, state bytes
  and required execution changes do not.
- Real-process `SignedBundleInstallation.test_install_upgrade_failure_restart_uninstall_and_explicit_rollback`:
  1 passed. This checks actual package publishing, authority install/upgrade,
  restart, uninstall and signed rollback, independently of the classifier mocks.
- New `canvas-bundle update-contract` CLI built; core/CLI Clippy with warnings
  denied, Cargo formatting, Python syntax and diff checks passed.

The focused Rust command was:

```sh
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features signed-bundles --lib package_update_contract_retains --locked --offline
```

The real-process case used `OTRIOS_TEST_BIN_DIR` pointing to the private runtime's
`build/mac-target/debug`, then:

```sh
python3 /tmp/otrios-d2-run.py python3 scripts/test_app_bundles.py SignedBundleInstallation.test_install_upgrade_failure_restart_uninstall_and_explicit_rollback
```

The classifier-corrected host build used the existing paused watcher with
`scripts/watch-dashboard.py --config <private-l2-config> --once --stage-only`.
It compiled Mac, Wasm and ARM and signed 14 app packages. The three existing Wasm
feature-specific dead-code warnings in Chat/transfers remain; the build passed.
`scripts/test-canvas-host-exports.mjs` passed against that Wasm build (1 test).

The first classifier-corrected host `d94f94c4…` retained native PID 16679 and
Web's original time origin, Test mode and call counters during publication of
`2b512522…`. However, its catalogue mount rejected the signed candidate: the
compatibility check compared the live OIB-compiled UI (including the Width 420
override) against pristine signed bytes. Its fallback error referred to a missing
app-level behavior module. The component guest had independently updated through
the admitted state snapshot, so a new guest hash and computed value alone were
insufficient acceptance evidence. Native readiness correctly failed because its
presented release had not advanced. `classifier-trial-*` records this failed trial
separately; it is not the final catalogue acceptance.

Commit `63f97d9` compares the immutable admitted OIB document, reuses semantic
admission and the existing library import compatibility checks, then checks the
remaining signed chrome/app contract after stripping derived OIB output. It
supports both catalogue-before-DocSync and DocSync-before-catalogue ordering.
It does not permit new export contracts, app source/grants, schemas or delivery
policy to slip through a compatible update.

The final Client filter traverses the complete
`update_catalog → Mount → Mounted → AcceptMount → Activated` exchange with a
Width 420 override, resumed host locals, exact guest hashes and retained counters.
Its rejection matrix covers grants, schema, export contracts and private delivery
metadata. All 9 cases passed:

```sh
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features signed-bundles --lib canvas::client::component_behavior_tests --locked --offline
python3 /tmp/otrios-d2-cargo.py clippy -p otrios-core --features signed-bundles --lib --locked --offline -- -D warnings
python3 /tmp/otrios-d2-cargo.py fmt --all --check
```

## Final signed Frame/Web acceptance

The final implementation is commit `63f97d9`. Its Mac/Wasm/ARM build and 14 signed
packages activated as baseline
`842cdac1d1396f66d15e11bd87c6c7ecc183ee06a083813b329cfcb2c4a47f18`.
The focused Wasm export check passed against this build. Native readiness passed
with GLES2, RGB565, rotation 90, calibrated touch, confirmed state, no pending
commands and all existing services active. No additional collection reset occurred.

| Phase | Signed release | Library revision / guest |
| --- | --- | --- |
| Base | `842cdac1…` | `f1e98216…` / STEP 1 |
| Guest update | `94c452d6…` | `7a512d04…` / STEP 100 |
| Guest rollback, final active | `da9bed85…` | `f1e98216…` / STEP 1 |

The final active release is
`da9bed857c552265afc9706561428d39239690062fe58a248fbe296fb6d71de4`.
Each guest publication used `scripts/publish-oib-library.py` through the task's
frozen-source wrapper, with the exact staged baseline, expected release, existing
checker/signing configuration and `--deploy-config`. Each transferred **163,840
bytes**; neither compiled a host. Full receipts contain the exact source, bindings,
library revision and WAT digests. The final code matches canonical
`libraries/oib-2d/source.json` and `slider.wat` (version 0.3.0, STEP 1).

Both publications preserved all **53 non-app host assets**, the actual authority
binary and the running native executable. Native **PID 17091** remained alive;
its `runtime_release` stayed `842cdac1…`, while `ui_release` advanced to the update
and rollback. Exact running hashes were:

- Native: `112616fda70c376de640c9e0207481e724fbe285dc635e3640c2422b943f368e`.
- Authority: `a688681bb227b826606be35a65bb84d727bdee4115d814c4fedb7c3c719a3e25`.
- Public Wasm: `8d276683b93f09f75e981dd34103a08ff64a98ff7640bf6ba2b4bdaab07701ee`.

Real Web canvas clicks at input 575 computed **575 → 600 → 575**. Separate live
ARM core sessions computed 600 under the updated guest and 575 after rollback,
with exact guest hashes and zero errors. Native calibrated evdev taps invoked
two calls under each guest (counter 0→2→4); the STEP-one coordinate produced
574 because of integer calibration rounding. Native Test mode survived both
catalogue transitions, demonstrated by those taps reaching guest behavior.

Web kept the same time origin/session sentinel, Test mode, semantic control IDs
and monotonic guest-call counters across both acknowledged catalogue updates.
Synthetic DOM held pointers retained the prior guest until Up. During the first
update, a held Move computed 625 with the old guest; catalogue activation retained
625, and the next real click computed 600 with the new guest. Rollback likewise
kept the coarse guest while held and retained 600 at activation. Counters are
frame-published: a same-value Move did not itself publish a new diagnostic frame;
Up exposed the accumulated Move/Up count. This is not a claim of physical browser
pointer capture. The full Client tests separately prove current host-local state
resumption under both catalogue/state message orders.

OIB generations advanced **6→7→8** without changing the post-cutover semantic
epoch, instances 20/30, control IDs 32/38/49/55 or the explicit Display Width 420
override. All four business values were restored and confirmed at **400, 700,
1000, 0**. Both visible clients returned to Edit mode after checks. Guest errors
remained zero. No app state was restored from backup.

On the final rollback, 100 ARM pointer Moves/Ticks took **149,335 µs** and performed
100 guest calls with **no layout/content work** while composition was withheld.
One subsequent core composition took **3,344 µs**, with one content update, one
changed resource and 36,784 changed source vertex bytes. These are complete ARM
core-loop/composition measurements, not isolated Wasmi, evdev, GLES or photon
timings. No Web guest timing is inferred from them.

The public manifest bytes hashed to the final release ID. The public Wasm response
matched its signed SHA-256 and 7,622,546-byte length and used `application/wasm`.
Monitoring resumed successfully. A compatible signed code rollback remains
available privately. Full repository preflight and the separate Linux shared-memory
IPC acceptance were intentionally not run; the focused commands and live results
above are the qualification claimed here.
