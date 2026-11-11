<!-- SPDX-License-Identifier: Apache-2.0 -->
# Prototype validation and implementation progress

Date: 2026-11-11. Latest results: **V08 text audit** below. Milestone: **M0 design/prototype**. Production M1–M6 and all
Q01–Q16 new-Designer runtime qualifications remain Pending. Existing D2/L1/L2
acceptance is unchanged. This task made no live deployment or state changes.

## V01. What was built

A self-contained [HTML study](prototype/index.html), generated from shell.html,
model.cjs and ui.js by build.py. No dependencies, server, credentials, network
requests, installation, native compiler or copied third-party component source.
The model is original disposable UX simulation under Apache-2.0; it is not the
shared Rust runtime and contains none of its implementation.

Implemented model scope: valid blank editing, insertion/removal/reorder, typed
property exposure for simple fields, explicit Apply/discard/history, stale-base
rejection, in-use deletion blocking, staged unused deletion/retirement, empty
library recovery, separate publication snapshot, sample values and local overrides.
The browser renders 16 representative specimens using native HTML controls.

Prototype simplifications, not production designs:

- One selected definition/root, no full nested container editor, slot routing,
  graph extraction, multi-selection or app binding UI.
- Source-level generation is simulated; rebase is an explicit acknowledgement
  without a remote diff/merge engine. There are no durable receipts or CRDT tests.
- Color pairs are editable per definition/mode; named style/variable IDs, alias
  ownership, state recipes, contract replacement and full RFC 0037 resolver are
  specified in docs, not implemented by the prototype.
- Instance A is a local foreground override demonstration, not a complete second
  linked-instance rendering/state adapter. Dependency uses cover model-local
  component references, not all API/style/action/slot/external dependencies.
- CSS flex sizing approximates flow; no production measurement engine, min/max
  distribution, cycle diagnostics or transformed hit testing is implemented here.
- Menu/select, tabs, progress, list and range widgets are deliberately simplified.
  No IME composition, native accessibility bridge, pointer-capture/evdev or GL proof.
- Publish snapshots RAM only. No signing, packaging, host update or external writes.
- Frame task panels are smaller and commands use 44px native HTML targets in this
  study. Production specifies 48 logical units and 320–360-unit task panels.

## V02. Commands and results

Run from the feature worktree:

```sh
python3 docs/oib-designer/prototype/build.py
node --check docs/oib-designer/prototype/ui.js
node --test docs/oib-designer/prototype/test-model.cjs
python3 docs/oib-designer/validate.py
git diff --check
```

The model suite passed **26 tests**, zero failures/skips. Its 16 per-control tests
exercise common insertion/apply/reopen/edit/remove lifecycle. Additional cases
exercise LabelledSlider numeric exposure, isolation/history, dependency blocking,
retirement, deletion, cycles/types, stale Apply, invalid layout, publication/override
reset, last-definition empty-library recovery and rename collisions. See
[evidence/model-tests.txt](evidence/model-tests.txt).

The browser suite in [browser-tests.js](prototype/browser-tests.js) was injected
only into html_preview, not the deliverable. It passed **24 scenarios / 129
assertions per run**, zero failures, at widths 1280, 1600 and 1024. These are
programmatic DOM interactions and assertions, not recorded human touch gestures.
Light/Desktop presentation was selected after the scenario suite for its final
visual inspection; do not infer an exhaustive Light-mode interaction matrix.

| Browser preview | Actual viewport | Shell height | Horizontal overflow | Outcome |
| --- | --- | --- | --- | --- |
| Frame / Dark | 1280×800 | 765.78px | None | Scenarios passed; presentation inspected |
| Desktop / Light final view | 1600×800 | 765.78px | None | Scenarios passed; desktop/light presentation inspected |
| Smaller Frame study / Dark | 1024×800 | 765.78px | None | Scenarios passed; not 1024×640 acceptance |
| Unsupported portrait | 375px width | Editor hidden | Not authoring | Readable unsupported notice verified |

See [evidence/browser-previews.json](evidence/browser-previews.json). Scenario
assertions include complete blank→Text+Slider→Expose→Apply, flow/gap preview,
undo/discard, Edit/Test input isolation, colors/override/reset, stale Apply,
interaction and retained active elements across all 16 specimens, blocked delete,
staged retirement, simulated publication and scrolling to lifecycle actions.

Required production target **1920×1080** was designed but not browser-qualified by
this tool (html_preview width is capped at 1600). Native **1280×800** is a design
reference; this browser-size test is not a fresh physical Frame measurement.
No broad Rust suite, ARM/Wasm builds, signed release verification, actual Frame
input/performance or native/Web runtime acceptance was run for this docs study.

## V03. Findings corrected during this iteration

| Finding | Correction | Evidence |
| --- | --- | --- |
| Last definition removal left a phantom draft; Discard threw | Explicit empty library/no selection, New remains usable | Dedicated model regression |
| Delete/retire bypassed shared Apply model | Stage in local draft; explicit Apply confirms | Model and browser retirement assertions |
| Range input recreated during each sample | Update range/readout in place; clamp crossing values | Browser active-element retention assertions |
| Other sample buttons could lose focus on rerender | Update readouts/selection in place | Retained active DOM nodes checked |
| Create/rename had inconsistent name rules | Reject duplicate names on Apply too | Model rename-collision regression |
| Blank minimum differed between UX/technical docs | Both now specify 48 | Documentation consistency check |
| Initial Frame HTML exceeded 800px height | Bounded scroll panes, compact toolbar sizing | 765.78px shell, no horizontal overflow |

The separate design review examined lifecycle/contract consistency and prototype
safety. Small findings were fixed locally. No fresh independent review is claimed
for the final corrections; the focused regressions above verify their reproductions.

## V04. Production qualification goal

Work through [Q01–Q16](CONTROL_MATRIX.md) in order of dependency, beginning with
Q01/Q02/Q03/Q04 and F01 LabelledSlider. For each row, validate UX journeys, typed
contracts, real compilation/input, styles/modes, overrides, dependency lifecycle,
resource limits and signed update behavior. A production row stays Pending until
its actual native/browser evidence is recorded. Share infrastructure checks where
proved; add the distinct failure cases for each control.

Next concrete task is M1 contract/measurement fixtures, followed by M2 atomic
working-copy Apply. Finalize mixed property/structural transactions and serialized
addresses before exposing UI promises. This package supplies the restart guide,
journey IDs, technical gates and representative ledger needed to resume that work.

## V05. Completeness audit follow-up

The Fixed-W-only finding triggered [COMPLETENESS_AUDIT.md](COMPLETENESS_AUDIT.md),
which supersedes any implication that V01–V04 established inspector completeness.
The design now has **27 journeys**, including preset/style/variable lifecycle and
visibility semantics. Prototype visibility, complete style resolution and nested
child layout remain explicitly unimplemented.

The corrected prototype adds independent Fixed W/H, measured sizes, finite-parent
Fill, both-axis bounds, distribution and clipping; removes hidden size clamps;
separates layer name/content; excludes editor hints from Hug measurement; freezes
pending deletion edits; and rejects guessed exposure types.

Commands repeated: build.py, node --check for model/UI/browser tests, node --test,
validate.py and git diff --check. **30 model tests** passed. Browser injection at
1280×800 passed **28 scenarios / 180 assertions**, including nine width/height
mode combinations, actual Fixed W/H and Fill dimensions, invalid bounds/blank
numbers, source-independent layer naming, blank outer height 48, Edit/Test size
stability, distributed alignment, clipping and Pending deletion/Undo.

Actual browser output and current model results are in
[evidence/completeness-browser.json](evidence/completeness-browser.json) and
[evidence/completeness-model-tests.txt](evidence/completeness-model-tests.txt).
No broader production or native qualification was performed. Earlier evidence
files intentionally retain their earlier counts and narrower tested source.

## V06. Responsive workspace and distinct control roles

The Frame/Desktop selector and device-based layout branch are removed. A single
canvas/task-dock layout adapts to viewport width and available height. Library,
Layers and Inspect are underlined semantic tabs; Edit/Test is a labeled segmented
interaction control; Preview appearance is a labeled dropdown. Revision status
and command buttons remain separate from these choices. See S02 and J22.

The prototype retains each task panel's scroll position and observes preview
geometry changes so measured W/H stays current after resizing. Work height is
computed from viewport minus measured chrome, bounded to 240–720px for the inline
study. Production height fills the available app area under its own host contract.
No device profile preference is stored.

Repeated model suite: **30 passed**. Browser injection at **728×800, 1280×800 and
1600×800**: **30 scenarios / 205 assertions per run**, zero failures and no
horizontal overflow. Output height was 800px at each width. Added checks cover
selected tab/panel agreement, arrow-key tab navigation, independent interaction
mode, absence of the profile selector, scroll restoration, width changes on the
same DOM without source/selection resets, and controlled available-height changes.
The latter invokes the sizing helper with synthetic 600/900px heights; it is not
a native window-manager resize or keyboard acceptance test.

Evidence: [responsive-workspace.json](evidence/responsive-workspace.json). Build,
JS syntax, local document links, generated-HTML reproducibility and whitespace
checks passed. Native deployment, shared-runtime layout and production keyboard/
accessibility qualification remain unperformed. Earlier evidence retains the
profile-based source and counts it actually tested; V06 supersedes that UX choice.

## V07. Matching appearance switch and useful status

Dark/Light now uses the same segmented-control component as Edit/Test. Both are
local preview contexts with explicit labels, independent pressed states and no
document mutation. The generation/base counter moved into collapsed Prototype &
technical details, along with the simulation commands and RAM-only limitations.
The duplicate canvas label is removed. A single command-area status reports All
changes applied, Unapplied changes, validation problems, review required or pending
deletion. Stale Apply has a user-facing review instruction rather than counter jargon.

Browser checks at 728×800, 1280×800 and 1600×800 passed **31 scenarios / 218
assertions per run**, no failures, no horizontal overflow, 800px output height.
New assertions cover matched switch components, appearance selection and state
isolation, clean/dirty/discard status, invalid/conflicting status, one status
surface and collapsed revision diagnostics. See
[evidence/appearance-status.json](evidence/appearance-status.json). JS syntax,
package links/IDs, reproducible HTML and whitespace checks passed. The unchanged
model suite was not repeated for this UI-only follow-up. No runtime deployment
or native acceptance was performed. V07 supersedes V06's appearance-dropdown choice.

## V08. Critical text audit

[COPY_AUDIT.md](COPY_AUDIT.md) records keep/remove/move decisions for all prototype
text families: chrome, canvas, panels, selected/empty states, fixtures, dialogs,
feedback and accessibility names. Persistent tutorial copy, repeated layer/type
badges, empty-selection prompts, empty API results and routine success narration
were removed. Help, override demonstration and prototype limits are collapsed.
Useful labels, units, variable names, actual content, API names, scoped dependency
counts and destructive/error feedback remain. The fake panel.default style label
was replaced by Colors rather than implying an implemented style binding.

Canvas selection now uses an accessible transparent Edit-only selection button
instead of a visible text badge. Existing exposed component API stays visible
without the contextual creation form. Tests exercise selection after copy removal,
source isolation, Test-mode overlay removal, collapsed help, and API visibility.

Browser checks at 728×800, 1280×800 and 1600×800 passed **32 scenarios / 227
assertions per run**, zero failures and no horizontal overflow, 800px output.
See [evidence/copy-audit.json](evidence/copy-audit.json). JS syntax, document links,
generated artifact reproducibility and whitespace checks passed. Model contracts
were unchanged, so the model suite was not repeated. No native build, deployment
or production accessibility acceptance was run.
