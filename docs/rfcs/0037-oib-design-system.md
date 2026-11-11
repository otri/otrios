<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0037: OIB design systems, named styles and variable modes

Date: 2026-11-11. Status: **proposed; no runtime implementation in this change**.
Visual reference: [DESIGN.md](../../DESIGN.md). Builds on
[RFC 0035](0035-oib-component-designer.md),
[RFC 0036](0036-upgradable-oib-component-libraries.md) and the
[incremental presentation work](../experiments/oib-incremental-presentation-2026-11-10.md).

The [complete Designer design package](../oib-designer/README.md) expands the UX,
layout and lifecycle around this style contract. It includes a qualification ledger
and prototype evidence; production style/runtime implementation remains proposed.

## Intent and existing boundary

Provide independently publishable, coordinated control appearances without a
built-in accent color. A component references a named style with typed fields;
fields contain literals or references to typed variables. A variable can resolve
to different values in Light and Dark modes. Names support authoring; stable IDs
support identity, updates, overrides and renaming.

Today `crates/otrios-interfaces/src/theme.rs` contains compiled shared tokens
BG/CARD/TEXT/MUTED/BRASS/COPPER/LINE. OIB exposes bounded color and geometry
properties, and confirmed Color-only updates can retain layout/input/geometry. The existing contracts are in
`crates/otrios-interfaces/src/oib/{designer,graph,library}.rs`; Source currently
supports Literal, Forward and AppBinding, with no style/variable reference.
L1/L2 admit signed exact library closures and bounded guest behavior. They do
not implement the style resolver, mode inheritance or contracts specified here.
This proposal does not reclassify existing qualified behavior as incomplete.

## Scope and delivery

S1 implements named styles, color variables, Light/Dark, explicit state recipes,
source-aware overrides, inspection, contrast diagnostics and a base catalog
using existing supported 2D primitives. S1 may carry admitted existing typography,
padding, radius and stroke fields in a style, but only color fields vary by mode
or interaction state. Geometry edits follow existing layout admission.

S2 may add font metric roles, more field types, gradients, image fills, multiple
strokes, effects and richer layout after separate renderer qualification. Wrap,
grid, rotated input geometry and full Figma inspector parity are not implied by
S1. XR remains a separate backend milestone. No CSS parser, selectors, constraint
solver, runtime Markdown interpretation or general executable theme code.

## Ownership and identities

A signed library can export a design system and its styles. An app can export an
app-local system. A Space-owned OIB document can hold editable systems/variables
within its existing authorization and bounded atomic edit model. Document
references cannot reach another Space or ambient host data. No global registry
of names is searched at runtime.

Each reference is `(owner, stable_id)`, where owner is a resolved exact signed
library identity or a document-local namespace. Labels such as `button.primary`
and `action_background` are display names; they are never reference keys.
Renaming does not change a stable ID. Imported IDs cannot be shadowed by local
names. A fork receives a new owner/identity, with explicit remapping of references.
IDs cannot be reused for different field meaning or type in a compatible update.

Proposed logical records (not a wire schema):

| Record | Required information |
| --- | --- |
| DesignSystem | Stable ID, name, variables, styles, default mode Dark |
| Variable | Stable ID, unique name in system, type, values for both modes |
| StyleContract | Stable ID, named typed fields, required fields, component applicability |
| Style | Stable ID, unique name, contract reference, complete default fields, state patches |
| StyleUse | Exact style reference, document overrides, instance overrides |
| ModeSelection | Inherit, Light or Dark; root additionally supports System |
| FieldValue | Typed literal or exact variable reference |

Use existing OIB semantic property addressing for authorable fields and overrides;
final serialized addresses must be specified alongside the codec before S1 lands.
A color is canonical RGBA8 sRGB, straight alpha. Style fields have explicit types:
Color, supported length/radius, existing typography resource reference, etc.
S1 variables are Color only. There are no expressions or variable-to-variable
aliases in S1; this avoids reference cycles and hidden fallback chains. Reuse is
multiple fields referencing one variable.

Each variable supplies Light and Dark values. The editor can initialize both from
one supplied color; the saved record still contains both. Missing modes, missing
required fields, unknown IDs, type mismatches and unsupported fields reject the
candidate. A field's authored default is explicit, not a missing-reference fallback.

## Style example

The YAML below illustrates authoring semantics, not accepted current package bytes.
Names stand in for stable IDs only to keep the example readable.

```yaml
variables:
  action_background:
    type: Color
    Light: '#151515FF'
    Dark: '#F5F5F5FF'
  action_text:
    type: Color
    Light: '#FFFFFFFF'
    Dark: '#151515FF'
  action_background_pressed:
    type: Color
    Light: '#383838FF'
    Dark: '#D0D0D0FF'
styles:
  button.primary:
    contract: button.appearance.v1
    default:
      background: { variable: action_background }
      text: { variable: action_text }
      icon: { variable: action_text }
      stroke: '#00000000'
      focus_ring: { variable: action_text }
      radius: 8
    pressed:
      background: { variable: action_background_pressed }
```

The contract defines supported defaults for all remaining fields (for example
stroke width and focus width); a packaged style is normalized to a complete field
map during admission. StyleUse can select a literal background override without
changing the variable or other uses of this style. Styles have no inheritance in
S1. Authoring tools may copy a style to create another; the saved result is explicit.

## Resolution order and states

Resolve a component appearance in this order, from lowest to highest priority:

1. Complete style default.
2. Selected patch, when selected.
3. Hover patch, only when enabled and hovered.
4. Pressed patch, only when enabled and pressed.
5. Disabled patch, when disabled (hover/pressed are then suppressed).
6. Document-authored field overrides.
7. Instance field overrides.
8. Focus overlay, when focus-visible; independent of the body field map.

Later patches replace only fields they contain. Selected and pressed can coexist;
pressed wins for a shared body field. Focus remains visible over either. Disabled
is host/semantic state, not merely a paint value; its activation suppression is
owned by the input system. S1 state patches may change color fields only, not
geometry, text, handlers or hit targets. Transition animation is deferred.

Overrides are explicit `(field, state_scope, value)` entries. `All` applies after
state recipes; a matching named-state override replaces All for that field at
that same override layer, using selected/hover/pressed/disabled priority above.
Instance layer wins over document layer. The inspector defaults to the currently
previewed named state and offers All explicitly; it warns that All can flatten
state feedback. Focus overrides target only the focus overlay fields. They cannot
remove required focus semantics from the base catalog.

Resolve variable references after choosing the field expression, in the receiving
component's effective mode. A style imported from another library uses that
library's resolved variable owner, never a coincidentally named local variable.
Styles declare `text` as the label color; `icon` may reference the same variable
but is separate so authors can deliberately distinguish them. Internal children
receive these through explicit component bindings, not ambient text inheritance.

## Modes and presentation ownership

System/Light/Dark at the app root is an explicit app preference; store a shared
preference through an existing declared app property only when the app chooses
shared behavior. Host appearance and inherited effective mode are local view
context, never automatically replicated as CRDT edits. In System, each host
resolves its own preference; an unavailable host preference resolves to Dark.

A container's Inherit uses its nearest ancestor's effective mode. Explicit Light
or Dark starts a new subtree context. The single Light/Dark context applies to
all systems used in that subtree; independently named collections/mode dimensions
are deferred. Moving a component can change its effective mode without changing
its style reference or overrides. Literal colors do not adapt; the inspector must
make that visible. Do not automatically invert colors.

Designer preview mode and simulated state are local, clearly labeled, and never
emit app intents, change real focus/capture or save disabled/pressed state.
Changing the preview is not a document edit. A real mode change may update colors
during a gesture but must retain semantic input identity and pointer capture.

## Editor behavior

The inspector begins with Style, Effective mode and State preview. Selecting a
style displays its applicable fields with a source indicator: style default,
state patch, document override, instance override, or resolved variable/mode.
Clicking a variable opens its owner, name and Light/Dark values. Editing an
imported variable requires a local fork/override or publication by its authorized
library owner; changing a label never grants write access.

Expose actions for Bind variable, Set literal and Reset override. Reset removes
only the selected scope's override, revealing the next source. A shared-variable
edit shows the number of affected bindings within the admitted document. It does
not claim knowledge of remote apps. Rename preserves references. Deletion of a
referenced variable/style is rejected unless the same atomic batch replaces or
removes all its references. Required field deletion is a contract change.

Use the existing generation-checked draft/apply model. Apply batches a mode pair,
style patches and references atomically. Stale drafts remain available for review;
there is no silent overwrite or partial Light/Dark save. Color-picker drags may
preview locally and coalesce publication; confirmation and cancellation semantics
must be explicit. Preview cannot rebuild/import the library for every sample.

## Compilation, invalidation and performance

Admit and normalize once per immutable system revision, then build a reverse
index from `(variable owner, ID)` to field uses, style to occurrences, and each
container mode boundary to inheriting descendants. Retain semantic identities.
Extend the existing `canvas/oib/paint.rs` Basis proof with exact style/variable
source revisions and mode dependencies; comparing only property registers cannot
prove a mode/style edit safe. Untracked or composed color dependencies retain the
existing full compilation fallback. Cache resolved expressions by exact system/style revision, override revision,
effective mode and state mask. Cache growth is bounded by admitted occurrences
and a fixed small number of state variants, not every historical edit/sample.
Release old dependencies and cache entries when their pinned UI is retired.

| Change | Required work |
| --- | --- |
| Rename/display metadata | Inspector metadata; no scene geometry change |
| Color value, state color, mode | Resolve affected colors and update paint |
| Radius/stroke width | Rebuild affected shape geometry; preserve layout if bounds are unchanged |
| Padding, font metrics, size | Invalidate affected layout dependencies and resulting paint |
| Structure, unknown dependency | Existing full candidate compilation/admission |

Mode changes traverse only inheriting uses; stop propagation at an explicit
child mode boundary. Compare resolved values before marking dirty. A changed
unused variable needs no scene work. Coalesce multiple dirty causes into the
strongest required work per frame. Keep color as retained draw paint, without
recreating GL vertex resources for an unchanged shape. Do not promise zero GPU
uploads for geometry changes or unproven dependencies.

Only visible content needs materialized draw resources, consistent with existing
viewport support. This RFC does not claim new virtualization. Offscreen layout
can remain necessary for Hug/scroll extents. Parallel resolution of independent
immutable partitions is optional later work: first remove unnecessary compilation
and measure phase costs. Apply results from one pinned revision atomically; never
mix modes/revisions within a submitted scene. Record resolution, layout, geometry,
upload and actual present timing separately; a 100K-elements/s goal is not evidence.

## Accessibility diagnostics

Associate label/icon uses with their actual backdrop through explicit compiler
usage metadata. Names such as `text` do not by themselves establish contrast.
Use sRGB relative luminance and contrast `(Llighter + .05)/(Ldarker + .05)`, with
standard sRGB linearization, no rounding up at thresholds. Composite straight
alpha in the same color/compositing convention as the renderer before measuring.
Unknown imagery, gradients or backdrop chains produce Unknown, never a passing
result. Check both modes and applicable state combinations, including overrides.

The base catalog targets 4.5:1 ordinary text and 3:1 required non-text cues against
adjacent surfaces. Decorative dividers are separate from required boundaries.
Do not claim disabled exemptions establish good usability. Report warnings with
component, field, mode, state, foreground and backdrop source, and offer manual
correction; never silently change values. Base catalog acceptance rejects known
failures; custom authoring may retain warnings. Complete WCAG conformance is
outside these focused contrast checks. Focus, semantics, target geometry and
keyboard access need separate tests.

Sources: [W3C text contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html),
[non-text contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).
The 48-unit touch target is OTriOS policy, not a claim that WCAG's minimum is 48;
see [target size minimum](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).

## Admission, packaging and compatibility

Provisional S1 caps per admitted app closure: 8 systems, 256 total variables,
128 styles, 32 fields/style, four body patches plus focus, and 256 KiB canonical
style bytes. Count imported systems once by exact identity; bound aggregate
expanded field/reference/override counts using existing stricter OIB limits.
Additionally cap normalized field/patch entries at 8,192, variable-reference uses
at 8,192, and explicit document/instance override entries at 4,096 per closure;
count occurrences before allocation and reject overflow with checked arithmetic.
Names are 1–64 ASCII identifier/path characters, unique within their record kind
and system. The concrete codec must enumerate allowed characters, byte ordering,
duplicate handling and canonical map ordering. These caps are proposals to verify
on Frame; they cannot raise existing scene/control/layout ceilings silently.

Store structured style payloads in the verified signed library closure; DESIGN.md
is guidance, never execution input. Add explicit style schema/contract versions
and host capability admission. Canonical bytes use the established versioned
encoding and SHA-256, never process hashing. Signatures and policy remain separate
from content hashes. Reject unsupported required capabilities before activation.

Compatible updates preserve exported system/style/variable identities, contracts,
field types, required imports and component bindings. Values, display names and
state color recipes may change. Added private records can be compatible; removing
an export or changing a type/meaning requires new admission and explicit reference
repair. Compatibility fingerprints and the release classifier must include this
contract before library-only activation is allowed. Do not classify every style
file replacement as automatically compatible.

Validate the complete candidate style graph, resolved typed references, compiled
scene and resource limits before swapping. Failure leaves active UI and values
unchanged. Use the established connected-confirmation/gesture safety boundary for
library activation; local mode changes need not wait for a library update.
Retain exact signed rollback and ordinary release-control v1 discovery. A new
state encoding requires a synchronized native/server/browser state protocol bump;
no version number is allocated by this documentation change.

## Implementation boundaries and cutover

Public schema/contracts go in `crates/otrios-interfaces`, authoring sources and
library tooling in Apache-2.0 paths. Resolution, compilation, invalidation and
runtime tests belong in LGPL `crates/otrios-core`. Native/Web hosts supply only
appearance preference and existing rendering/input services. They must not each
implement an alternative style resolver. Wasmi guests consume declared typed
properties; this design adds no ambient theme query or unrestricted guest API.

First deliver the schema/resolver and focused tests, then source-aware Designer
editing and a reference catalog, then signed Frame/Web acceptance. The existing
Rust theme constants remain until shell/tooling use the same admitted base style
source. Do not prematurely map BRASS to action_background or overwrite authored
app colors. The base library changes deliberately; third-party/app themes remain
explicit choices. If replacing disposable PoC state simplifies a later cutover,
record it at that implementation step; this RFC performs no reset or migration.

## Acceptance plan

1. Golden canonical bytes and digest; malformed/duplicate/over-budget references,
   missing mode/type and unsupported capabilities reject without partial mutation.
2. Table-driven resolution covers all state masks, both modes, explicit nested
   modes, All/state-specific overrides, reset, rename and style replacement.
3. Real component label/icon bindings verify actual foreground/backdrop contrast;
   include threshold, alpha, unknown backdrop and dark-in-light subtree cases.
4. Palette/mode/slider-color changes preserve layout, control identities, capture
   and vertex resources; geometry/font changes take the correct stronger path.
5. Concurrent/stale shared edits use existing conflict semantics. Separate native
   and browser clients can use different System appearances with the same shared
   document; local preview creates no durable edit.
6. Signed library value update and rollback retain instance overrides, app values,
   focus and stable semantic IDs, with existing host hashes unchanged when eligible.
7. Inspect the actual base catalog on Frame and browser: long/localized text,
   scroll/clipping, touch/keyboard, focus and all state/mode combinations. Measure
   real changed-path timing; record actual commands and results.

No tests, builds or deployments of this proposed system have run. Before S1 code,
finalize the concrete codec/property addresses and capability identifiers; qualify
font/stroke mappings against existing primitives. Those are implementation gates,
not an invitation to imply Figma parity or advance deferred milestones.
