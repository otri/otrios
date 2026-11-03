<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer Frame Focus milestone

Date: 2026-11-04. Status: **active; shared-style follow-up in progress**.
Branch: `feat/oib-frame-focus`, from approved design baseline `d904e49`.
Design: [OIB Designer usability](OIB_DESIGNER_USABILITY.md).

## Goal and scope

Deliver readable Frame tooling with a canvas and properties dock, a local Layers
drawer, explicit composition hierarchy with collapsed component boundaries and
correct instance names, practical typed text/number/color editing, source-aware
reset and protected app bindings. Measure responsive retained scrolling on the
actual Frame. Preserve nested identities, causal conflict/publication semantics,
held input and the existing application state.

Desktop Studio, D2 structural authoring, rich typography, images and XR remain
deferred. Runtime implementation and runtime tests remain LGPL in core. No new
dependency, service, identity, trust root, unsafe code or wire format was added.
State protocol remains v27; release discovery remains control v1 on the shared
WebSocket, independent of state decoding.

## Ownership and execution gates

| Owner | Scope |
| --- | --- |
| Parent integrator | Contract freeze, derived Frame shell, client integration, bounded window optimizations, signed integration/deployment and actual Frame acceptance |
| Hierarchy lane | Typed composition/Components projection, full stable addresses, wrapper fusion, nonvisual ownership and retired-root filtering |
| Draft lane | Bounded local text/number/RGBA buffers and focused core tests |
| Source lane | Typed source chains, masked sources and version-guarded conflict alternatives |
| Browser-input lane | Explicit cancellation and acknowledged Tab traversal, IME/stale-field guards |
| Native telemetry lane | Bounded input/worker/renderer timing with actual present returns and distinct DRM flip identity |
| Fresh independent reviews | Concrete integrated changes and every review/performance follow-up in separate tool-managed worktrees |

Children had bounded nonoverlapping files and no deployment, push, PR, private
configuration, new checkout creation or subdelegation permission. The parent
integrated reviewed commits and owns the existing Frame/Web deployment.

Execution gates: freeze additive contracts; implement the readable shell and
hierarchy; integrate practical editors; verify conflict/reset/input/resource
invariants; independently review; sign/deploy with compatible rollback; complete
actual Frame scrolling and authoring checks; finish visual acceptance and release
control restoration before completion or a ready PR.

## Implemented behavior

- A 352-unit properties dock and 320-unit modal Layers drawer preserve authored
  preview geometry. Tooling uses scale-2 text (measured editor font size about
  22.5 logical pixels), 52-unit fields and at least 48-unit primary hit targets.
  Canvas zoom does not shrink tooling. Split buttons and swatches retain their
  intended widths; full rows retain complete hit coverage.
- Composition follows visual parents, fuses reference wrappers, collapses reusable
  component boundaries by default and separates nonvisual Objects. Components
  has its own tree. Names are labels; full typed addresses supply identity.
- Local bounded drafts support text, finite constrained numbers, hex color and
  opacity, validation, caret editing, Apply/Enter/blur, Cancel/Escape and shared
  software keyboard input. Typing does not publish per keystroke. Invalid or
  hidden dirty drafts remain associated with their original property address.
- Provenance displays authored/inherited/live sources. Reset causally clears an
  occurrence override. Conflict choices require the exact current register
  version and alternative. App bindings remain read-only to literal edits.
- Edit mode selects without activating app controls; Test mode uses confirmed
  actions. Captures pin geometry/publication. Tab reveals admitted below-fold
  fields; source navigation waits for confirmation and revalidates its target.
- Retained 304-unit windows anchor both edges to one slab. Placement-only pans
  reuse paint buffers; the fixed span equals the former maximum aligned span.
  Existing scene/control/cache/coordinate limits remain unchanged.

## Focused validation and review

Actual commands used Rust 1.90.0, isolated Cargo/target directories and
`--locked --offline`:

- `cargo test -p otrios-core --features canvas --lib oib_designer`: 28 passed on
  the final implementation.
- Same command filtered to `retained_text`: 5 passed independently.
- The runtime/fixture/hierarchy/draft/source suites previously passed 57 tests;
  their semantic implementations were unchanged by the final window follow-ups.
- `OTRIOS_SPACES_SERVER=.../dashboard-server cargo test -p otrios-core --features
  canvas --test spaces_process d1:: -- --nocapture --test-threads=1`: 3 passed,
  0 ignored, with an actual server binary (94.46s). An initial sandbox bind denial
  was rerun with authorized access. This is a real authority-process boundary,
  not proof of shared-memory Linux IPC.
- Browser text/worker tests: 33 passed. Web host native/wasm checks and module
  syntax checks passed; the wasm check reported three existing core warnings.
- Focused formatting, whitespace and strict core Clippy checks passed during
  integration. Broad repository preflight was intentionally not repeated.

Independent reviews found and verified fixes for below-fold Tab traversal and
source navigation after a queued blur commit. Final anchored-window review had
no findings: 28 Designer tests, 5 retained-text tests and 5,961 joint panel cases
passed. Largest combined scene: 316 nodes / 73 controls, below 512 / 128. Checks
covered landscape/portrait, legacy/Focus, selected multi-property inspector,
viewport coverage, offscreen/stale actions, captures and held mesh immutability.
Those checks are in-process evidence, distinct from the hardware results below.

## Signed deployment and actual Frame acceptance

Implementation source: `e8a33bb2430cbf0e0a51fc7e53200d919a9012c0`.
Signed release:
`27262dd14d5a99bd136145338f44e50a5d48026a1f688a32a0adccb3909f0a21`.
The final documentation commit has identical build inputs.

ARMv7 and wasm builds passed. Existing-publisher signature and complete signed
app closure verified before activation. Authority/native pointers, v27
negotiation, health, confirmed idle state, rotation 90, calibrated touch, RGB565
800×1280 output and existing monitoring/tunnel services passed live checks.
A deployment SSH reset occurred before key exchange; both active pointers were
checked unchanged before retrying over the existing pinned connection.

Each final pan used six roots, a separate cold gesture and 30 offered samples/s
for 30 seconds on the actual calibrated Frame device. Unique input sequences
were joined across worker production, renderer receipt and actual present return;
DRM flip timestamps were checked separately. No ready-file FPS proxy was used.
Logs had zero dropped rows. Temporary telemetry overrides were removed and the
normal existing native service restored after each run.

| Final pan | Unique updates/s | p95 return interval | Maximum interval | Gate |
| --- | ---: | ---: | ---: | --- |
| Hierarchy | 28.1646 | 35.367 ms | 53.075 ms | Passed |
| Selected slider inspector | 28.3314 | 35.368 ms | 52.926 ms | Passed |
| Overflowing preview | 28.3313 | 35.422 ms | 53.005 ms | Passed |

The unchanged gate is at least 28 updates/s, p95 at most 50ms and no gesture gap
over 100ms. All pans retained generation 22 and released input. Earlier hierarchy
results 27.90 and 27.9974 did not pass; measured boundary rebuilds motivated the
bounded window changes. An initial manual-host profiler failed because stopping
the service removed its release supervisor socket; it supplied no valid timing.

[Timing summaries and restored-fixture evidence](experiments/data/oib-frame-focus-2026-11-04/)
record exact counts, intervals, latency, release identity and zero-drop checks.
Raw timing logs remain in ignored `target/frame-focus/release/`.

Actual Frame keyboard/control flows passed:

- Width 347 stayed local until Enter and confirmed only the nested width override.
- Tab reached Label; Sound stayed local until Enter. The visible Reset override
  control restored authored Volume.
- Typed hex #123456 and opacity 65 each confirmed once. A typed inspector query
  confirmed #123456 and 65.09804%, the expected 8-bit alpha 166.
- On the final release, actual Frame controls restored original width 320 and
  causally reset the temporary color to inherited #5588FF.
- Only temporary roots 65538–65541 were removed. Audio 20 and Display 30 remain;
  app values are unchanged at [440, 700, 1000, 0], authored labels and pre-existing
  explicit overrides remain intact. Final confirmed generation is 28.

Compatible code-only rollback remains verified at signed release
`f62027700e7d8a05a6791866231d22a7ea230f3324079e6e853a250f0dfee8ef`,
source `03e5aab7ef0656617166c1330bfacf699a611777`. It preserves persistent sessions
and protocol compatibility. No app data or identity backup/restore was used.

Release automation is restored. The existing launch agent fetched source pin
`6354cd2417ead19df02d70258b745050784f9db1`; its code inputs match the qualified
`e8a33bb` build. Mac and Frame pointers match the signed release above. The
original persistent build environment and publisher are retained. Previous
release-control files were preserved privately under the date-first record
`2026-11-04-frame-focus-control`. Temporary Frame helper binaries, timing folders
and service environment overrides were removed. The normal native service is
confirmed and idle. No new service was installed.

## Outstanding acceptance

- The user explicitly authorized fixture-only captures. Actual Frame and browser
  fixture images were inspected; the larger type and hierarchy indentation are
  readable. User feedback requests shared button outlines, theme and typography;
  the [styling follow-up](OIB_FRAME_FOCUS_STYLE_FOLLOWUP.md) is in progress.
- The user signed into the shared preview. Actual browser typing/Enter produced
  Sound on Frame at generation 29; Reset recovered authored Volume at 30; Escape
  canceled a replacement without a write; Tab committed exact width 377.5 and
  focused Accent hex; Shift-Tab returned to Width. Enter restored 320 at generation
  32. Native status text rounds fractional widths; the browser field retained the
  exact 377.5 value. Repeat a focused smoke check after the styling cutover.
- No PR is open for this milestone. Do not mark the goal complete or open a ready
  PR until the remaining scoped acceptance and release-control work is finished.
