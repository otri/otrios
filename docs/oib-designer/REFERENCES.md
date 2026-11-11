<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design references and Figma alignment

Reviewed: 2026-11-11. These references inform our original design; they are not
runtime dependencies, file-format compatibility commitments or imported libraries.

## R01. Strongest Figma references

[Figma Simple Design System (SDS)](https://github.com/figma/sds) is the strongest
official application-system example found. It connects variables, styles,
components and Code Connect with a React implementation, and is labeled alpha.
Its [Community file](https://www.figma.com/community/file/1380235722331273046/simple-design-system)
is a useful editable companion. Learn organization from it; OIB does not adopt
React, CSS or Code Connect as its runtime.

For **editor chrome**, Figma's [UI3 retrospective](https://www.figma.com/blog/our-approach-to-designing-ui3/)
describes the tradeoffs around panel attachment, canvas room and labeling. It
supports our choice of docked/resizable desktop panels and deliberate Frame Focus.
The [UI3 kit link](https://www.figma.com/community/file/1486123838948777078/ui3-figmas-ui-kit)
was found as a reference, but its actual Community contents/publisher metadata
were not inspected. Do not claim we audited or imported that kit.

For **layout semantics**, use the [official Auto Layout guide](https://help.figma.com/hc/en-us/articles/360040451373-Guide-to-auto-layout-in-Figma),
[horizontal/vertical flow guide](https://help.figma.com/hc/en-us/articles/31289464393751-Use-the-horizontal-and-vertical-flows-in-auto-layout),
[grid guide](https://help.figma.com/hc/en-us/articles/31289469907863-Use-the-grid-auto-layout-flow),
and [sizing API](https://developers.figma.com/docs/plugins/api/properties/nodes-layoutsizinghorizontal/).
They describe container layout and per-axis sizing. A known difference: Figma
converts a Hug parent to Fixed when a child fills that axis. OIB proposes an
explicit diagnostic/fix choice instead of silently changing another object's size.

For **component contracts**, consult [component properties](https://developers.figma.com/docs/plugins/working-with-component-properties/),
[native slots](https://help.figma.com/hc/en-us/articles/38231200344599-Create-and-use-slots-to-build-flexible-components-in-Figma-Design)
and [variable modes](https://help.figma.com/hc/en-us/articles/15343816063383-Modes-for-variables/).
These are separate from a runtime state machine. Optional API vocabulary sources:
[plugin typings](https://github.com/figma/plugin-typings) and
[REST types](https://github.com/figma/rest-api-spec/blob/main/dist/api_types.ts).
They do not select OIB's wire format.

The supplied [OTriOS layout reference](https://www.figma.com/design/0IKyMGoZ7TemQJhhSedtcY/OTriOS---Layout-Reference?node-id=3-3&m=dev)
was inaccessible to the web reader. Earlier attached inspector/layout-atlas images
remain the directly supplied visual evidence. No authenticated Figma session or
plugin was installed/accessed; do not claim the linked document was inspected.

## R02. Alignment matrix

Here Core means proposed M1–M4, not deployed support. Tests are target acceptance
families in T12/C03; HTML CSS cannot establish engine equivalence.

| Figma concept | OIB term and decision | Core / later / divergence | Required test |
| --- | --- | --- | --- |
| Auto Layout frame | Layout container, Horizontal/Vertical | Core | Nested row/column golden positions |
| Fixed/Hug/Fill | Same visible per-axis terms | Core, versioned new contract | 3×3 sizing combinations, finite root viewport |
| Min/max | Additional bounds per axis | Core | Clamp, overflow, min>max rejection |
| Alignment | Group main/cross placement; nine-point packed UI | Core | 9 positions × both flows × definite/Hug extents |
| Gap/padding | Explicit gap and per-edge padding | Core nonnegative gap | Zero/asymmetric/large values, empty/single child |
| Space between | Distribute spare main-axis space | Core | 0/1/many children, max-clamped Fill |
| Hug parent + Fill child | Dependency path and explicit repair | Deliberate divergence | Reject cycle; repair preserves unrelated properties |
| Wrap | Explicit line breaking and row/column gaps | Later capability | Finite width, line breaks, Fill basis |
| Grid flow | Actual grid layout, distinct from guides | Later bounded grid; no full track solver | Track/count/gap bounds, text heights |
| Absolute position | Free position, excluded from Hug | Core with explicit finite geometry | Paint/hit/scroll/clipping agree |
| Constraints | No corresponding authoring system | Deliberately omitted | UI has no constraint dependency editor |
| Component property | Stable exposed typed property/action | Core | Forwarding/type/default/rename preservation |
| Instance swap | Explicit compatible reference replacement | Core lifecycle, mapped values | Mapping, overrides, slots and role changes |
| Slot | Declared bounded content slot | Core extension of OIB direction | Cardinality/type/ownership and reorder |
| Variant | Definition/preset or explicit component choice | Separate from interaction-state styles | No confusion with selected/pressed/mode |
| Variable mode | Inherited Light/Dark context | RFC 0037 core | Nested override, local host preference |
| Design library update | Signed exact revision/adoption | Existing foundation extended | Compatibility, failed admission, rollback |
| Multiplayer changes | Authorized generation/CRDT model | OIB-specific semantics | Stale Apply/receipt/reconnect; no silent rebase |
| Editor panels | Desktop docked panels; Frame task panel | OIB adaptation | Target viewports, keyboard, retained scroll |

No `.fig` import/export, round-trip compatibility, plugin API compatibility or
full feature parity is promised. A later importer needs its own explicit supported
subset and diagnostics; naming familiarity does not make arbitrary Figma graphs
admissible.

## R03. Control and journey references

[SwiftPieces components](https://swiftpieces.com/components) supplies breadth across
interaction families; [the public flows index](https://pro.swiftpieces.com/library/flows)
distinguishes entry, core task and settings journeys. We reviewed public outlines
including [starting a habit](https://pro.swiftpieces.com/library/flows/cadence-start-a-habit)
and [a conversational task](https://pro.swiftpieces.com/library/flows/nimbus-ask-and-talk).
Only public descriptions were available; private Pro templates and complete Xcode
projects were not inspected. The useful lesson is to follow a task through its
result, recovery and next action, rather than stop at a styled screen.

The [SwiftPieces repository](https://github.com/Saivion/SwiftPieces), tree revision
`5f766abb471871012a712bcd89a824dea345bdc8`, was inspected for its source/metadata/
preview/validation organization. Relevant reference entries include
[RangeSlider](https://swiftpieces.com/docs/components/inputs/range-slider) and
[HoldToConfirm](https://swiftpieces.com/docs/components/controls/hold-to-confirm).
We use interaction problems as prompts for original OIB contracts, not a SwiftUI
port or code extraction. Its [MIT + Commons Clause notice](https://github.com/Saivion/SwiftPieces/blob/main/LICENSE)
includes restrictions on redistribution of the components themselves. No source,
assets or templates from that project are incorporated in this package.

## R04. Visual identity and accessibility

Root [DESIGN.md](../../DESIGN.md) defines the original OTriOS adaptation of the
[SpaceX-inspired design analysis](https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/spacex/DESIGN.md).
Keep visual identity separate from Figma's layout/authoring mental model.

Contrast and target policies reference W3C's
[text contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html),
[non-text contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html)
and [target minimum](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).
Our 48-unit Frame target is a project policy, not a quote of the WCAG AA minimum.
Actual font scaling, focus, semantics, clipping and native accessibility remain
separate qualification work.
