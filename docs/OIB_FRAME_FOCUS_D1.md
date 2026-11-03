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

Goal created and required project documents read. New implementation branch
created in the parent tool-managed worktree. No runtime changes, new builds or
deployments have occurred for this milestone yet. Update this section with actual
commands, source/release identities, measurements, review results and limits.
