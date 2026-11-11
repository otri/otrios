<!-- SPDX-License-Identifier: Apache-2.0 -->
# Designer completeness audit

Date: 2026-11-11. Trigger: the prototype exposed Fixed W but no Fixed H. The first
M0 pass tested control lifecycles without systematically crossing inspector fields
against their paired axes and data paths. Its passing tests did not establish
inspector completeness. This audit adds that missing check, not a parity claim.

## A01. Concrete defects corrected

| Finding | Correction | Check |
| --- | --- | --- |
| Fixed height had no input and used hard-coded 260 | Independent stored Fixed H alongside Fixed W | All nine width/height mode pairs, measured fixed lengths |
| Fill H used a hard-coded child length | Explicit finite preview-parent inner H 320; Fill uses 100% of it | Actual Fill dimensions on both axes |
| Fixed fields appeared editable in Hug/Fill | Disable each input outside its own Fixed mode, retain authored length | Switching back restores W/H independently |
| Prototype silently forced minimum W 180 and max W 100% | Remove implicit clamp; explicit bounds and scrollable overflow | Min/max validation and measured outer-box clamping |
| Bounds, distribution, clipping missing in prototype | Add both-axis min/max, Packed/Space between, Clip | Invalid bound rejection, reduced alignment choices, actual overflow property |
| Layout validation covered only flow and gap | Validate sizing modes, finite/ranged lengths, padding, bounds, alignment, clipping | Invalid Apply leaves confirmed model unchanged |
| Empty numeric entry became zero | Preserve blank entry/error, block Apply until corrected | Blank gap does not overwrite source as zero |
| Layer name also changed visible Text | Separate name from content/caption | Rename/content changes independent both ways |
| Edit selection badges changed Hug layout | Position editor badges out of flow; blank hint also excluded | Edit/Test size stable; empty outer min-height 48 |
| Pending definition deletion permitted more edits | Clear Pending deletion state; model rejects edits, UI disables them | Undo restores authoring; no discarded hidden additions |
| Exposure guessed Text for unknown controls and ranges | Explicit supported endpoint types; NumberPair for range; unknown composite rejected | Pair/type mismatch and unsupported exposure tests |
| Alignment lacked selected state and distribution semantics | Show selected point; Space between exposes only cross-axis choices | Three meaningful choices, selected choice enabled |
| Empty Hug definition ambiguous about padding/minimum | Outer-box bounds: max(min, content + padding + stroke) | 48 minimum plus 16+16 padding yields 48, not 80 |

## A02. Written-design omissions resolved

- J25 now specifies preset creation, duplication/forking, retirement/deletion and
  existing-instance semantics. Presets copy authored assignments on insertion;
  later preset edits affect future insertion only. Shared style/variable refs
  inside those assignments remain live references.
- J26 specifies style/variable creation, defaults, required fields, mode pairs,
  duplicate with shared or copied variables, forks, retirement and in-use deletion.
- J27/T03.4 define Visible/Hidden/Collapsed separately from enabled/opacity, including
  layout, paint, hit testing, focus, capture and modal consequences. Visibility
  authoring remains capability-gated and is not implemented in the prototype.

## A03. Inspector coverage ledger

| Family | Design specified | Prototype now | Explicit remaining gate |
| --- | --- | --- | --- |
| Per-axis Fixed/Hug/Fill | T03 / S06 | Root both axes, Fixed W/H, measured output | Child sizing and real engine golden fixtures |
| Min/max W/H | T03 | Root bounds with invalid-case checks | Real water-fill distribution/budget cases |
| Flow/alignment/distribution | T03 | Row/column, packed nine-point, space-between | Per-child stretch and nested containers |
| Padding/gap | T03 / S06 | Uniform padding, gap | Independent four-edge editing/link/unlink |
| Overflow | T03 | Clip and viewport scrolling demonstration | Authored scroll container ownership |
| Position/transforms | T03 | No Free/rotate/flip editor | Free-node placement and inverse-hit qualification |
| Appearance | RFC 0037 / DESIGN | Local background/text color pair only | Stroke/radius/opacity stacks, full named styles/modes resolver |
| Visibility/enabled | T03.4 / J27 | Test controls versus Edit isolation only | Real visibility/capture/focus semantics and disabled fixtures |
| Definition/instance | T02 / J09/J10 | One definition plus local text override sample | Complete multiple linked instances and bindings |
| API | T04 / J07 | One explicit endpoint type per supported specimen | Endpoint picker, actions, slots and app bindings |
| Presets/styles/variables | J25/J26 | Not lifecycle editors | Versioned schemas, forms and dependency index |
| Deletion/retirement | T07 / J12–J15 | Staged definition operations, local reference guard | Full typed repair, all dependency kinds and external exports |
| History/conflicts | T06 / J16/J17 | Local undo/redo, simulated stale generation | Conditional applied undo, real diff and durable receipts |
| Publication/adoption | T11 / J19 | RAM snapshot only | Signing, compatibility and native/Web live acceptance |
| Wrap/Grid/assets/XR | T03.5 / C05 | Unavailable | Separate capability/backend milestones |

These are intentional visible limits, not functioning controls hidden behind a
marketing claim. Keep this ledger current whenever a field/family is added.

## A04. Required completeness review method

For every inspector field, follow the full chain: selection/scope → displayed
source → editor → validation → transient preview → Apply → reopen → override/reset
→ undo/cancel/error → dependency consequences. Compare paired axes, bounds, edges,
modes and scopes. Test actual computed/observed values rather than merely the
presence of a DOM element or a successful model method call.

Use the same method for lifecycle records: New, Open, Edit, Duplicate/Fork, Apply,
Retire/Unretire, Delete unused, Delete in-use, Publish and rollback. A test of one
shared insertion method for sixteen controls is useful, but cannot replace tests
of every exposed layout field or each control's actual typed behavior.

This is an audit of design/prototype coverage. Native runtime, real Frame input,
IME/accessibility, 48-unit touch targets, layout performance, signing and production
control qualification remain unrun; see VALIDATION.md and CONTROL_MATRIX.md.
