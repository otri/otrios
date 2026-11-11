<!-- SPDX-License-Identifier: Apache-2.0 -->
# Representative control qualification ledger

Date: 2026-11-11. This is the bounded representative set for the Designer goal,
not a promise to port an entire SwiftUI library. Each row must eventually pass
its lifecycle, layout, styles, input and real runtime acceptance. A thumbnail or
HTML-native interaction alone cannot mark a production row complete.

## C01. Breadth and selection

The [SwiftPieces catalog](https://swiftpieces.com/components) is a useful breadth
reference across inputs, navigation, feedback, collections, data and media. Its
[repository](https://github.com/Saivion/SwiftPieces) separates component source,
metadata, previews and validation. We adopt that separation as an organizational
lesson; no Swift implementation or assets are copied. The public flow index offers
examples of complete tasks rather than disconnected screens. Our tasks below are
original OIB authoring and settings journeys, not reproductions of Pro templates.

Select controls by different engineering pressures: intrinsic text measurement,
boolean and numeric state, continuous capture, text entry/IME, choices, collection
identity, focus, overlays, compositional APIs and asynchronous feedback. This set
also forces testing the editor itself with those same kinds of fields.

## C02. Required representative set

| ID | Specimen | Exposed contract / compose exercise | Critical layout/style/input case | Prototype coverage | New Designer runtime |
| --- | --- | --- | --- | --- | --- |
| Q01 | Text / Label | text, optional description; no action | Long/multiline/localized text, Hug width/height, contrast | Native text and lifecycle | Pending |
| Q02 | Button | label, enabled, activate | Fixed touch target, Hug label, pressed/focus/disabled | Native button sample action | Pending |
| Q03 | Toggle | checked Boolean, label, changed | Selected + focus, semantic state, narrow row | Native checkbox sample | Pending |
| Q04 | Slider / LabelledSlider | Number value/min/max/step, changed, label | Fill width, capture/cancel, value readout, two instances | Native range; wrapper/API model | Pending |
| Q05 | Text field | Text value, placeholder, validation | Keyboard/IME, selection, wrapping policy, error | Native text input sample | Pending |
| Q06 | Numeric field | Number value, min/max/step, format | Intermediate invalid input, locale, clamping on commit | Native number input sample | Pending |
| Q07 | Choice picker | selected stable option ID, ordered options | Long option labels, empty/missing option, focus | Native select sample | Pending |
| Q08 | Segmented choice | selected option ID, options | Unequal labels, selected/hover/pressed, keyboard arrows | Native buttons sample | Pending |
| Q09 | Progress | value/max, label, indeterminate capability | Display-only semantics, finite value, status text | Native progress + sample advance | Pending |
| Q10 | Tabs with content slot | selected tab ID, tabs, content slot | Header/content sizing, focus vs selection, state retention | Sample tabs/content text | Pending |
| Q11 | Settings list row | stable row IDs, content/accessory slots, activate | Scroll ownership, clipping, long list, reordering | Three sample rows only | Pending |
| Q12 | Confirmation dialog | title/body, confirm/cancel actions, modal state | Barrier, focus return, keyboard, outside-input policy | Native dialog sample | Pending |
| Q13 | Stepper | Number value/min/max/step, changed | Boundary disabling, repeat input, numeric label width | Native sample buttons/readout | Pending |
| Q14 | Range slider | lower/upper/min/max/step | Two identities, noncrossing bounds, capture, thumb collision | Two native ranges/readout | Pending |
| Q15 | Action menu | options with roles/enabled, chosen action | Anchoring, clip escape, dismiss, focus/keyboard | Native select approximation only | Pending |
| Q16 | Status/result panel | state enum, message, optional retry | Empty/loading/success/error, focus announcement | Sample Ready/Complete text | Pending |

“Runtime Pending” means the **new Designer contract** is unqualified for that row.
It does not deny existing bounded Button/Slider/Toggle/Text features in D2/L2.
Progress sample advance is a fixture tool, not the eventual display-only control API.
Menu/select, list and tabs are intentionally simplified prototype substitutes;
none demonstrates production popover, collection virtualization or slot routing.

## C03. Every-row qualification checklist

For each Q row, create a separate fixture/evidence entry and record source revision,
signed release (when applicable), viewport, host, command and actual outcome:

1. Create from blank; insert the control; select and rename; expose its API.
2. Configure Horizontal/Vertical, Fixed/Hug/Fill axes, valid bounds and overflow.
3. Test long labels, zero/one/many children/options, empty and maximum admitted data.
4. Apply a compatible named style. Verify Light/Dark and supported state combinations;
   edit shared variable; override one of two instances; Reset back to inheritance.
5. Test in isolated preview, including disabled, focus, keyboard and touch where
   applicable. Samples must not edit authored defaults or real app values.
6. Modify definition; preserve stable child/instance IDs, app bindings, values,
   surviving local overrides and captured input under eligible paint changes.
7. Duplicate/fork; remove an instance; attempt deletion of the in-use definition;
   retire it; repair dependencies explicitly; delete when truly unused locally.
8. Reject invalid/stale Apply without partial state. Confirm discard/history and
   lost-reply/reconnect behavior through real authority receipts when implemented.
9. Qualify signed compatible update and rollback without host rebuilding where
   supported; breaking contract changes take new admission.
10. Check aggregate editor+keyboard+modal+preview scene/control/layout ceilings and
    measured changed-path timing on Frame/Web. Record unknown/unrun work plainly.

Not every test requires every row to repeat infrastructure-heavy campaigns. Share
proved invariants in a versioned fixture harness, then add the row's distinct
failure cases. Repeat broader checks for concrete new risks, not mechanically.

## C04. Fixture families and journeys

| Fixture | Controls | Purpose / exit |
| --- | --- | --- |
| F01 Audio settings | Q01/Q03/Q04/Q13 | Compose LabelledSlider; independent instances; shared style and override/reset |
| F02 Create and rename | Q01/Q02/Q05/Q06/Q12/Q16 | Blank, field drafts, validation, Apply/cancel, success and failure |
| F03 Theme/choice settings | Q07/Q08/Q10/Q15 | Stable option selection, mode preview, nested scope/slot and menu dismissal |
| F04 Dependency repair | Q02/Q11/Q12/Q16 | Discover use, block deletion, retire, explicit replacement or cancel |
| F05 Long inspector | Q01/Q05/Q11 | Retained scrolling, software keyboard, long text and scene budgets |
| F06 Continuous data | Q04/Q09/Q14 | Capture, multi-value bounds, samples, readouts and paint-only updates |

User narratives are J01–J27 in [UX_JOURNEYS.md](UX_JOURNEYS.md). F01 is the initial
teaching path. F02/F04 force empty/error/destructive outcomes. F03/F05 expose editor
navigation and input friction. F06 is where performance evidence must distinguish
input→model, layout, upload, present and perceived response.

## C05. Broader library roadmap, with honest capability boundaries

| Family | Initial useful form | Later capability / reason |
| --- | --- | --- |
| Cards/panels | Composed containers, text, actions | Swipe decks/3D motion need gesture and animation qualification |
| Dates/times | Typed value plus supported picker adapter | Calendar/time-zone rules and accessibility need dedicated API |
| Navigation | Tabs/rows with explicit routes/slots | Adaptive stacks/deep-link history require app navigation contract |
| Feedback | Status, confirmation, finite progress | Toast queue/indeterminate animation need lifetime/scheduling rules |
| Data | Static/simple admitted presentations | Scrub charts/heatmaps require data/window/selection contracts |
| Text | Plain labels/fields | Rich typography/editing needs font and document layout qualification |
| Media | Explicit unavailable asset placeholder | Import/decoding/lifetime/security and resource budgets first |
| AI/chat | Existing supported text/list primitives | Streaming actions and tool effects belong to app contracts |
| Glass/background effects | Monochrome opaque base surfaces | Blur/shaders/haptics are backend capabilities, not style workarounds |
| XR | No implied support | Separate layout/input/rendering backend admission |

A missing capability is an explicit gate. Do not fake a calendar with hundreds of
unbounded buttons, implement image loading in a style expression, or fork layout
per host just to make a catalog thumbnail look complete.

## C06. Progress updates

Current prototype evidence is in [VALIDATION.md](VALIDATION.md). All 16 rows have
shared model lifecycle tests and a browser-rendered specimen/interaction check;
typed production behavior, per-row exhaustive matrix and native shared runtime
acceptance remain Pending. Runtime implementation tasks should update one Q row
and its F fixture with concrete evidence before advancing it to Qualified.
