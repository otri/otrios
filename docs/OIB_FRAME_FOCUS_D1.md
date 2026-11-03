<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame Focus implementation milestone

Date: 2026-11-04. Status: **active**.

The user approved the [usability design](OIB_DESIGNER_USABILITY.md) and explicitly
requested a new implementation goal, with Frame Focus first. Integration branch:
`feat/oib-frame-focus`, starting from design baseline `d904e49`.

## Concrete goal

Deliver a readable, responsive OIB Designer on the existing Frame and browser:
a canvas and properties dock, a local Layers drawer, a meaningful collapsed
composition tree, and direct text/number/color property editing with source-aware
reset. An author must be able to select Audio's nested Level control, enter Sound
and a non-preset width, edit color/opacity, inspect the actual inherited source,
and reset the override without changing Volume/Brightness or their bindings.

Frame Focus is the active presentation. Desktop Studio's three persistent panes,
general structure editing, mutable connections, rich typography and images remain
deferred. Keep all runtime implementation and runtime tests under LGPL in core.

## Invariants and contract gate

- Preserve stable document/definition/occurrence/property identities and the
  existing causal store. Tooling projection and raw input buffers are local and
  bounded; neither becomes a second persistent document graph.
- Preserve authored assignment/forwarding/live override precedence, equal-value
  overrides, causal clear and explicit conflict resolution. Bound sources remain
  read-only to literal design edits.
- Preserve capture-pinned publication, modal barriers, pass-through decoration,
  clipped controls and release-outside cancellation. Drawer input must not reach
  the preview beneath it.
- Preserve legacy Live UI and retained D1 app state. A changed companion layout
  must have an explicit compatible path; never reset a collection to make it fit.
- Prefer an additive local/core implementation over signed schema changes where
  the existing literal permissions and value constraints already suffice.
  Freeze this after the mapping pass. If serialized descriptors/wire change,
  coordinate required feature admission and the declared state protocol across
  hosts, preserving release-control v1. Do not silently weaken admission.
- No new dependencies, unsafe code, services, identities or trust roots.

## Bounded ownership

| Role | Initial scope | Boundary |
| --- | --- | --- |
| Parent integrator | Contract freeze, Frame Focus shell, assigned client integration, release/protocol decisions, reviewed integration and deployment | Owns integration branch and all live deployment. |
| Hierarchy implementer | Additive core local hierarchy query/projection and focused core tests | `canvas/oib/document/hierarchy.rs`, its export in `document.rs`, and dedicated tests; leave existing `objects()` behavior intact initially. |
| Tooling-input mapper | Read-only map of existing keyboard, text-input bridge, raw editor buffers and source/conflict APIs | Concrete narrow adapter and editor plan; no concurrent client/designer edits. |
| Performance analyst | Read-only profiling/instrumentation map and local reproduction of the Frame pan path | Identify measured cost and minimal fix boundaries before editing shared presentation code. No live/private configuration access. |
| Independent review | Concrete integrated diff and focused regressions | A fresh tool-managed delegate for each review/recheck. |

All child checkouts use tool-managed worktree isolation and inherit the committed
baseline. No child may push, open PRs, deploy, add dependencies, alter private
configuration, touch sibling/original checkouts or subdelegate. Partition
`client.rs` and `oib_designer.rs` explicitly before implementation. Findings and
scope changes return to the parent before crossing an ownership boundary.

## Execution sequence and acceptance gates

1. **Freeze the integration contract.** Review the hierarchy query and mapping
   reports. Decide derived chrome layout versus a compatible signed sample
   update, exact input routes, property-editor admission and profiling points.
2. **Readable navigation and Frame pan slice.** Build the properties dock and
   drawer, correct names/parents/reference boundaries and default collapse.
   Calibrate font metrics and touch targets on actual hardware. Instrument and
   fix the measured scroll path before adding a large editor surface.
3. **Practical editors.** Reuse shared core input with bounded local buffers keyed
   by full property address. Implement text, numeric and RGBA editors, validation,
   Enter/blur commit, Escape/cancel, source inspection and causal reset. No write
   per keystroke. Preserve invalid drafts against their original address or
   discard them explicitly; never retarget them when selection changes.
4. **Focused correctness and independent review.** Verify source masking,
   equal-value override/reset, conflicts, read-only bindings, keyboard/IME,
   drawer/modal/clipping behavior, stale actions, captures, resize and resource
   bounds. Run real authority-process cases where changed semantics require
   them; do not claim process or Linux IPC acceptance from local adapters.
5. **Signed deployment and live acceptance.** Use an isolated build/staging
   directory and compatible signed code rollback. Deploy the completed slice to
   existing Frame/Web, inspect real readability and perform the authoring flow.
   Finish review follow-ups and live gates before opening a ready PR.

### Performance and usability release gates

- The prior baseline observed four presented-readiness reports during a 3.001s
  hierarchy pan with 30 offered samples/s and a 1283ms maximum report gap. This
  corroborates the user's complaint; it is not independent scanout FPS.
- Add real presentation/stage telemetry and measure warmed 30s gestures in the
  hierarchy, inspector and overflowing preview, including window crossings and
  the maximum admitted six-root fixture. Target at least 28 presented updates/s
  at 30 offered samples/s, p95 intervals at most 50ms and no gesture stalls over
  100ms. Record cold first use separately; do not claim 60fps.
- Scrolling must remain local: no OIB/app writes or generation/value changes.
  Keep existing node/control, cell, byte and coordinate ceilings unless separately
  reviewed. Fully expanded valid content must remain usable.
- Essential tooling text must be readable at 100% physical scale; use actual
  font metrics rather than confusing the current 8-unit capital-height scale
  with CSS pixel sizes. Start with scale 2 where appropriate and rows near 52
  units with at least 48-unit touch zones. Canvas zoom must not shrink tooling.
- The drawer and source/details popovers must consume their covered region and
  leave the visible, uncovered portions of controls interactive.

## Evidence

Goal created and required project documents read. The parent owns the feature
branch and integration. Hierarchy query commit `146453e` and local property-draft
engine commit `1d70a50` are integrated from isolated lanes.

The first parent slice projects the existing signed preview into a physical-size
properties dock and Layers drawer, uses explicit hierarchy parents and default
component disclosure, separates the Components tree, and reads cached OIB status
while painting. It preserves signed app geometry, bindings, state and metadata.
The next parent slice wires direct text/number/hex/opacity fields, local color
swatches, bounded drafts, Apply/cancel/Enter/blur, scale-2 field painting and the
shared software keyboard. A hidden dirty draft remains addressable through an
Unapplied edit row. Field IDs stay stable across local repaint; stale IDs and
stale epochs reject. Browser Escape adapter and source/conflict UI are still
pending.

Actual parent checks with Rust 1.90, local Cargo/target directories and locked
offline dependencies:

- `cargo check -p otrios-core --features canvas --lib --locked --offline`: passed.
- `cargo test -p otrios-core --features canvas --lib oib_designer --locked --offline -- --nocapture`:
  17 passed before the second Frame Focus regression was added. Legacy three-pane
  tests explicitly exercise the compatibility presentation; they are not Frame
  Focus usability acceptance.
- `cargo test -p otrios-core --features canvas --lib frame_focus --locked --offline -- --nocapture`:
  2 passed, with real six-root fixtures, landscape/portrait dock/drawer geometry,
  hidden semantics, modal coverage, release-outside cancellation, full expansion
  scroll budgets, separate Components, and unchanged app fields/OIB root.
- Scoped `rustfmt` and `git diff --check`: passed.

These are in-process checks. Source/conflict navigation, integrated practical
editing, software keyboard, native presentation measurement, independent review,
signed deployment and real Frame/Web acceptance remain incomplete. No release
was deployed for this slice. Update this section with actual
commands, source/release identities, measurements, review results and limits.


Editor integration checks (in-process only):

- `cargo test -p otrios-core --features canvas --lib frame_focus --locked --offline -- --nocapture`:
  5 passed, including one queued commit for Sound/377.5, causal reset to authored
  Volume, invalid draft retention/cancellation, RGBA/opacity preview with no write
  until Apply, unchanged app fields, shared software keyboard input and deferred
  resize during held key contact. Confirmation uses local authority adapters.
- `cargo clippy -p otrios-core --features canvas --lib --locked --offline -- -D warnings`:
  passed after fixing one collapsible-if diagnostic.
- Scoped formatting and whitespace checks pass. No deployment occurred.


Source/conflict/browser integration:

- Typed source and guarded conflict APIs integrated as `87b008f`; bounded native
  presentation logs integrated as `50286b0`; browser cancellation/traversal as
  `f6b7aea`. Native log enablement and actual Frame timing remain pending.
- Inspector Info follows the controlling forwarded property (shared panel cell
  100 rather than masked slider 101), exposes authored inheritance and keeps
  read-only app bindings protected. Explicit conflict choices retain a Merkle
  register version and reject stale alternatives. Reset remains a separate causal
  clear action. Fused wrappers remain inspectable through Inspect reference.
- Captured designer pan moves reuse pinned layout geometry; Down and tap-release
  continue using the visible scene. Managed Back uses the Focus header hit area,
  avoiding the generic breadcrumb's overlap with Layers. Browser panel key traps
  yield Escape/Tab to the admitted text bridge.
- Actual focused checks: 23 designer tests passed before the managed Back test;
  all 57 tests across fixture/runtime/hierarchy/property-editor/property-source
  suites passed. The managed Back pointer regression separately passed. The
  browser text/worker suite passed 33 tests; app.js module syntax passed.
- New source-navigation and concurrent conflict-choice UI tests passed through
  local adapters. One initial conflict test incorrectly reused the compiled
  expanded Ui as a fresh signed seed; it now uses the original signed fixture.
  No authority-process or live acceptance claim follows from these local tests.

The existing production pin advanced to `03e5aab` while this branch was being
implemented. That completed persistent-session work must be included before the
next signed build so the deployed Safari login persistence is preserved. No
production config or deployment was changed during this inspection.


Independent review and follow-up (still no live acceptance):

- Native review at `cfadb7b` found no correctness findings. It passed 24 designer,
  5 retained-text and 8 native worker/input/telemetry host tests. Linux renderer
  execution and real timing remain separate gates.
- UI review found two P2 issues: Tab rejected below-fold fields, and Go to source
  silently stayed put after blur queued a write. Parent fixes reveal only a
  currently admitted field before trusted keyboard traversal, retain public
  stale/offscreen-ID rejection, and defer source or hierarchy navigation until
  confirmation. Deferred targets are revalidated before selection. Unavailable
  draft selections show a notice and retain an explicit discard action.
- `cargo test -p otrios-core --features canvas --lib frame_focus --locked --offline -- --nocapture`:
  11 passed, including the two review regressions and hierarchy navigation after
  a valid blur. One initial assertion assumed the wrong signed field order; the
  final test follows Accent/Opacity/Label and wraps back to Width, then explicitly
  scrolls an old field offscreen before testing activation rejection.
- The first staging attempt at frozen `cfadb7b` failed because the new offline
  Docker builder had no flexbuffers registry entry. Public registry cache was
  copied read-only from the earlier builder into the isolated container. Retried
  ARMv7 compilation passed for dashboard-direct/server/monitoring/bootstrap,
  canvas-bundle and oib_instance_probe. That compile predates the review fixes;
  the final candidate must rebuild from their committed source.
- A code-only rollback kit at ignored target/frame-focus/release/rollback-code
  verifies signed release f6202770 and retains authority SHA-256
  5ab9daf0acf87097c68ad044186262dddac2e3563c9115dd271182f60d129364,
  matching the actual Frame. It preserves persistent sessions. No app state or
  identity material was copied; the full backup/inventory script was not used.

### Reviewed final control geometry and live acceptance in progress

- `441519b` preserves the declared widths of split scope/Apply buttons and color
  swatches when fitting a retained row to the properties dock. Full rows still
  fill their panel. Parent focused Frame Focus tests: 12 passed. Fresh independent
  review: no findings; 28 Designer tests and a temporary row-edge/non-overlap
  geometry probe passed. The synthetic narrow-panel probe is clipping evidence,
  not admission or physical-device acceptance.
- The final staged release is
  `cfbb8ea458bb8a200a98a9b3328a707368522c6453e41d1b9365e44b71b6b4aa`,
  built from frozen `441519b6dee2fc52b6074ec74072f95154c4434b`.
  Signature and complete signed app closure verified before activation.
- A bounded loopback observer, using the Frame's existing Space and native
  credentials only on Frame, confirmed the two-root fixture at generation 13
  with app values `[440, 700, 1000, 0]`. Existing overrides are retained. This is
  the baseline for the temporary six-root performance fixture; it does not prove
  scroll performance or direct-editor usability.
- Public signed-out browser presentation correctly exposes read-only properties.
  Authenticated browser editor acceptance remains unproven. Live pixel export
  was rejected by automatic approval review and has not been bypassed; visual
  capture awaits explicit authorization. DOM geometry and native timing remain
  independently available.

### Actual Frame timing and bounded-window follow-up

The signed `cfbb8ea4` release passed live health, matching native/web release,
v27 negotiation, calibrated touch and RGB565 output checks. Four temporary named
duplicates established six roots at generation 17; app values stayed
`[440, 700, 1000, 0]`. A 30-second hierarchy pan offered 901 samples at 30 Hz
through the real calibrated Frame device and normal native service. Timing logs
flushed orderly with zero dropped rows; the temporary service environment was
removed and the normal service restored. This is actual renderer/DRM evidence.

The first trial produced 837 unique input-associated local updates in 30.000337s
(27.90/s), with p95 presentation-return interval 35.411ms. The largest interval
including release was 105.895ms; warmed move intervals peaked at 53.014ms.
Sample-to-return p95 was 65.245ms. This does **not** pass the release gate.
Boundary rebuilds took roughly 10ms composition and uploaded a new overlay mesh,
compared with roughly 3.6ms composition for placement-only frames.

The follow-up doubles bounded retained-window slabs from 152 to 304 units,
reducing topology rebuild frequency while keeping the same presented node/control
ceilings. All 28 Designer tests passed, including real six-root scroll-budget
coverage. Hardware timing for this follow-up remains pending. The initial manual
profiling harness failed because stopping the service removes its runtime path
and release supervisor socket; it produced no valid performance result. The
successful trial instead used a temporary environment override on the existing
service and preserved its release supervisor. No new service was installed.

### Warmed Frame trials on `2e6823c`

Release `26f5d06c07148a8c8387975852e508a02a9a05f141f799e3fbae7ee1c5b3aa55`
passed signed activation, matching host/native readiness and existing display/input
contracts. Each trial used six active roots, a separate cold gesture, 30 offered
samples/s for 30 seconds, unique input sequences joined across worker and actual
renderer returns, DRM flip timestamps, zero dropped timing rows, and orderly log
flush. Temporary runtime environment overrides were removed after each trial;
the normal existing native service was restored.

| Pan | Unique updates/s | p95 return interval | Maximum return interval | Result |
| --- | ---: | ---: | ---: | --- |
| Hierarchy | 27.9974 | 35.383 ms | 53.019 ms | Strict 28/s gate remains open |
| Selected slider inspector | 28.3314 | 35.367 ms | 53.481 ms | Passed |
| Overflowing six-panel preview | 28.3982 | 35.368 ms | 36.095 ms | Passed |

All three pans retained generation 18 and released input. Before these trials,
the actual Frame software keyboard entered non-preset width 347: generation 17
remained unchanged throughout draft editing, then Enter confirmed only cell 202
at generation 18. The original width override 320 is recorded for restoration.

The next focused optimization anchors the retained window's bottom to its top
slab. Previously the bottom crossed a separate boundary halfway through a top
slab, rebuilding topology unnecessarily. A fixed `(ceil(height / step) + 1)` span
equals the previous maximum aligned span, preserving worst-case overscan and
scene/control ceilings. The regression checks reuse across the former lower-edge
boundary as well as the previous 152-unit boundary; offscreen captures still
reject. Actual hardware acceptance of this final follow-up remains pending.

The actual Frame software-keyboard label flow also passed: Tab traversed the
nested slider fields to Label, Sound stayed a local draft until Enter, cell 203
confirmed `Sound (override)`, and the visible Reset override control restored
`Volume (authored)`. Generation advanced only for those two intended writes
(18 → 20); input was released, with the native process/epoch retained.
