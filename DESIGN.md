<!-- SPDX-License-Identifier: Apache-2.0 -->
# OTriOS design reference

Date: 2026-11-22. Status: **shared shell and Designer chrome implemented**.
This is the human-readable reference for OTriOS shell, Designer and base 2D
controls. Shared breadcrumbs, account access and paired shell appearances are
implemented; the full authored control/style catalog remains incremental work.
The technical contract, resolution rules and remaining delivery gates are tracked
in [RFC 0037](docs/rfcs/0037-oib-design-system.md).

## Character

OTriOS should feel precise, calm and purposeful. Use quiet surfaces, clear
hierarchy, readable headings and recognizable controls. Color supports identity,
actions and state rather than replacing labels or interaction cues.
Controls should remain readable and recognizable on the Frame and in a browser.

The selected references are the existing OTriOS apps:

- **Dark:** The Brass Relay's charcoal surfaces, warm brass/copper palette,
  admitted font hierarchy, buttons and heading navigation.
- **Light:** Home's warm paper surfaces, ink text, soft color families and
  recognizable app-icon/card treatment.

These supersede the earlier monochrome/SpaceX-inspired palette proposal. Home
supplies visual references, not the complete Designer command set. Designer must
retain equivalent navigation and capabilities in both appearances.

## Home and experiment hierarchy

Home presents a small set of primary destinations in each Space. For Public,
those are **The Brass Relay**, **OIB Designer** and **Test Playground**. Home is
the launcher itself, so it has no self-launching Home tile. Monitoring remains
in the separately authorized Operations Space.

Test Playground owns the experiment collection: Components, Layouts, Long
Document, Text Panning, Text Input, 2D Animations, 3D Animations and Bundle Counter.
Show these once inside that collection rather than duplicating all of them on
Home. Keep OIB Designer as a primary authoring tool rather than a Playground
experiment/shortcut.

The launcher now uses one signed navigation hierarchy to drive collection views,
shared card/icon components and heading breadcrumbs. Home displays primary entries; Test
Playground displays its children. Installation, Space admission and independent
app data remain separate from launcher placement: grouping an experiment must
not uninstall it, detach its collection or reset its values.

Navigation returns from an experiment to Test Playground, then to its Space/Home.
Preserve useful direct links when establishing canonical routes. Collection arrangement
must retain reordering and access to child app information, sharing, export,
replacement and removal under the same permissions; Home simplification must
not make those functions unreachable. Apply the same hierarchy in Light/Dark,
using the selected visual references rather than maintaining two app lists.

## Shared breadcrumbs

Implemented in the shared Rust client for all current catalog routes. See the
[shared shell release record](docs/experiments/shared-shell-2026-11-22.md) for
build, deployment and acceptance evidence.

Use one global location row, with the account control consistently at its right:

`OTriOS > Current app                                            Profile`

`OTriOS > Collection > Current app                               Profile`

Root Home displays `OTriOS`, with Public and Operations as content sections.
Spaces are omitted from every breadcrumb. The visible path describes only the
app hierarchy. Space identity continues to travel in `?space=<id>` for routing,
admission and data selection. Ancestor navigation retains that selected Space;
access checks and discovery visibility continue to apply. Space settings and
membership remain accessible through their existing management controls.

| Canonical route | Breadcrumb |
| --- | --- |
| `/` | **OTriOS** |
| `/brass-relay` | OTriOS > **The Brass Relay** |
| `/oib-designer` | OTriOS > **OIB Designer** |
| `/test-playground` | OTriOS > **Test Playground** |
| `/test-playground/components` | OTriOS > Test Playground > **Components** |
| `/test-playground/layouts` | OTriOS > Test Playground > **Layouts** |
| `/test-playground/long-document` | OTriOS > Test Playground > **Long Document** |
| `/test-playground/text-panning` | OTriOS > Test Playground > **Text Panning** |
| `/test-playground/text-input` | OTriOS > Test Playground > **Text Input** |
| `/test-playground/animations-two-d` | OTriOS > Test Playground > **2D Animations** |
| `/test-playground/animations-three-d` | OTriOS > Test Playground > **3D Animations** |
| `/test-playground/bundle-counter` | OTriOS > Test Playground > **Bundle Counter** |
| `/monitoring` | OTriOS > **Monitoring** |

The obsolete `/bundle-counter` and `/test-playground/oib-designer` aliases are
removed. Use only the canonical routes above; retired Live UI also stays absent.

### Common behavior and geometry

- Render ancestors as links with one `>` separator; render the final item as the
  current page title. Each ancestor has a distinct destination. OTriOS opens the
  launcher; a collection crumb opens that collection in the same Space.
  Back follows the nearest
  available admitted parent and preserves existing unsaved-change handling.
- Build the visible row from the signed catalog's placement and admitted app
  titles. Use current Space metadata for admission and navigation context. An absent attached parent uses the launcher's existing
  nearest-attached-parent fallback. Respect access visibility throughout.
  Remove the stale authored Designer-to-Playground ancestor and supply Counter's
  actual Playground parent through this shared model.
- Use the same top inset, minimum row height, type role, alignment, separator,
  divider and account target in both appearances. Keep the header at the viewport
  top. App titles share one scale; body layout and fitted content cannot move the
  header down. Resolve the paired Brass Relay/Home semantic palette.
- Home, collections, chat and Monitoring have a compact phone header. Other
  current paths retain their landscape content minima (1280 × 800 logical
  units). Before adding longer/deeper paths, provide labeled overflow or reflow
  while preserving the title and usable touch targets; overflow is not implemented.
- Use the same Profile/Sign in entry on every app. Place Sign out inside the
  account menu. Chat participants and chat identity remain room-specific actions
  under their existing permissions and identities.
- Put app commands in a separate toolbar immediately below the location row.
  Collection arrangement actions occupy that toolbar; Designer's editing actions,
  save status and Edit/Test control occupy its toolbar. Separate component scope
  navigation from global app location.

### Views within a route

Designer retains `OTriOS > OIB Designer` while Library, Layers and
Inspect switch workspace panels. Its local scope row identifies the current
document, component and instance; a component name cannot replace the global app
title. Appearance and Edit/Test remain modes. Apply/Discard and property drafts
retain their existing leave guards and transactional behavior.

Profile, app information, Space settings, membership/access, retained data,
sharing, export/replacement, move and removal flows keep their local panel/dialog
titles and the underlying location context. These are existing local views,
without separate public URL routes in the current catalog. The account entry
remains outside the breadcrumb path.

### Monitoring's fixed content

Monitoring may preserve a fixed dashboard canvas matching the Frame design,
scaled proportionally within the area below the shared header. Keep the global
`OTriOS > Monitoring` row anchored and sized like every other app;
letterboxing belongs to dashboard content. The account control stays at normal
header size. Fitting the dashboard must preserve its internal chart arrangement
and leave header navigation independent of dashboard scaling.

### Delivered scope and further qualification

The shell shares location/account geometry, paired semantic colors, button
geometry, app icons and separate app toolbars across the current routes. Home and
Playground use “Arrange apps” rather than a generic Edit action. Designer has its
own document/component scope, save status and editing commands below the global
header. The account menu contains System/Light/Dark, account settings and sign-out
when signed in. Web remembers only the local appearance preference; System follows
the browser/OS. Native hosts without an appearance signal default to Dark.

Focused scene/protocol checks cover every current route in both modes, matching
commands and checked state, account barriers and Monitoring fit. Existing leave
checks and a draft-preservation regression cover Designer. Physical Frame and
production browser pixel review remain acceptance follow-ups; Frame is offline.
The complete base-control state/contrast catalog below remains a design target.

## Shared navigation and functional parity

Use one Designer shell and command model with Light/Dark appearances. Command
availability depends on the document, selection, permissions and editing state;
appearance never removes functionality or changes what an action does.

| Area | Required in both appearances |
| --- | --- |
| Global heading | Consistent OTriOS/collection/app path, with Space context outside the breadcrumb, using Brass Relay's readable heading/navigation pattern |
| Editing scope | Document/component/instance context, equivalent breadcrumbs and return-to-document behavior |
| Workspace | Library, Layers and Inspect, with the same panel availability, selection and scrolling |
| Editing lifecycle | New component, Undo, Redo, Discard, Apply, status and the same conflict/leave confirmations |
| Test interaction | Edit/Test selection with the same sample isolation and return path |
| Identity and access | Equivalent sign-in/profile/sign-out entry points and relevant Space access actions, under the same permission rules |

Home's light presentation does not itself provide component-scope navigation,
Designer panels or the editing lifecycle. Those capabilities must receive the
light treatment rather than disappear. Chat-specific Members/You actions should
map to the appropriate shared Space/account concepts, not be copied blindly.

Keep the global app heading separate from the current editing-scope breadcrumb.
A definition name should not obscure which app or Space the user is in. Use
Home's icon/color language for library and component identity; do not replace
clear action labels with unexplained icons. Preserve the authored preview and
explicit style overrides when changing editor appearance.

Before accepting the consolidation, compare semantic controls and actions for
the same user/document/selection in both modes. Walk the same navigation and
editing journey in each: enter Designer, open a component, return to the document,
edit, Undo/Redo, Apply or Discard, Test, and return to the Space. Theme changes
must preserve navigation, selection, drafts, business values, keyboard focus and
modal/input barriers. Fill missing navigation entry points in the shared shell
using existing commands; a theme is never a reason for a reduced command set.
Capabilities beyond the existing Designer, Home and Space services need their
own explicit scope.

## Organization

A **design system** owns variables and named control styles. A **style** describes
an appearance such as `button.primary`, `button.secondary` or `field.default`.
Each component references a compatible style. Its inspector exposes the fields
that matter to that component, with their source visible.

- Colors: descriptive names such as `action_background`, `action_text` and
  `control_stroke`. There is no required global `accent`.
- Style fields: `background`, `text`, `stroke`, `icon`, `focus_ring`, typography,
  radius, spacing and control-specific fields such as `track` and `thumb`.
- Modes: Light and Dark values under the same variable identity.
- States: default appearance plus selected, hover, pressed, disabled and focus
  treatments. These are distinct from Light/Dark modes.
- Overrides: explicit instance choices. Reset removes the override and resumes
  the style binding; it does not copy today's resolved color.

`background` is meaningful inside a style, not a single global background for all
controls. Several styles can reference the same shared variable intentionally.

## Reference palette

These opaque sRGB values map the existing app motifs to shared semantic roles.
The shared shell and Designer chrome use these paired values. The authored
preview retains its own styles and explicit literals. Complete base-control
state/contrast qualification remains required, including nested/overridden surfaces.

| Variable | Dark | Light | Role |
| --- | --- | --- | --- |
| `canvas_background` | `#171B20` | `#F7F6F0` | Workspace and page |
| `surface_background` | `#23282E` | `#FFFEFA` | Panels and grouped content |
| `text_primary` | `#EFF1ED` | `#2C322B` | Main text and icons |
| `text_secondary` | `#ACB3BB` | `#6D7469` | Descriptions and metadata |
| `divider_stroke` | `#424951` | `#DDDFD4` | Decorative separation |
| `control_stroke` | `#707A84` | `#6D7469` | Interactive control boundaries |
| `action_background` | `#D8B775` | `#344D3D` | Primary action fill |
| `action_text` | `#171B20` | `#FFFEFA` | Primary action text/icons |

Dark secondary emphasis may use the existing copper `#B98460`. Light app/component
identity can use Home's Sand, Sage, Stone, Sky, Lilac and Rose families. These are
identity treatments, not substitutes for selection, focus or error semantics.
Control boundaries, raised surfaces and interaction-state colors still need
explicit paired-mode qualification rather than inheriting decorative dividers.

Use a transparent background only when the parent surface is known. A decorative
divider is deliberately quieter than a control boundary; do not substitute it
where the border is needed to identify the control.

Error, warning and success colors belong to explicit status styles, accompanied
by text or recognizable symbols. They are not a general brand accent. Their
exact palette is deferred until the status catalog is reviewed in both modes.

## Typography

Keep the currently admitted font resources and use Brass Relay's readable heading,
body and control hierarchy as the consolidation reference. A replacement font is
not part of this work. The role table below remains a longer-term typography
target; implement only what the existing metrics/scales can represent.

| Role | Target logical size / line height | Weight | Treatment |
| --- | --- | --- | --- |
| Display | 32 / 38 | Bold | Short page or welcome heading |
| Heading | 24 / 30 | Semibold | Page and panel title |
| Section | 18 / 24 | Semibold | Inspector group |
| Body | 16 / 24 | Regular | Reading, labels and input |
| Control | 16 / 20 | Medium | Buttons and segments |
| Supporting | 14 / 20 | Regular | Help and metadata |

These sizes describe intended logical typography, not the current integer font
scale ABI. Until font metrics can express them, map roles to supported scales
without shrinking the existing readable text. Verify wrapping and height on the
actual Frame. Uppercase is optional for short display headings and section
markers. Keep form labels, body text and user content in their natural case.
Do not use letter spacing or tight leading that cuts off localized text.

## Geometry and layout

Use spacing steps **4, 8, 12, 16, 24, 32, 48** logical units. Standard control
padding starts at 12 vertically and 16 horizontally; enforce a 48-unit touch
height for the base catalog. Small icons can sit within a larger effective target.
Interactive targets must not overlap. Dense desktop presentations may use a
separately reviewed density style; never shrink the Frame's controls to fit.

Use the existing Brass Relay button motif as the control reference and Home's
card/icon treatment for light identity surfaces. Shared shell controls use a 12-unit radius in both modes. Consolidate padding
across modes rather than preserving separate per-app recipes. A radius is
clamped to the available shape. Default strokes are
1 logical unit, with final device-scale snapping verified on each host.
Avoid decorative shadows, gradients, glass and glow in the base catalog.

Layout follows container flow: horizontal, vertical, then explicitly qualified
wrap/grid support. Children use Fixed, Hug or Fill per axis, with padding, gap
and alignment. There is no constraint solver. Free positioning is explicit and
reserved for overlays or illustrations. Scroll panels when content exceeds the
viewport; avoid Previous/Next navigation for ordinary inspector content.

## Browser density and phones

The web host uses CSS pixels as device-independent interface units for Home,
collections and chat. One scene unit maps to one CSS pixel at normal browser zoom
when the viewport is at least 320 pixels wide. Device-pixel ratio affects backing
resolution, not text or touch-target size. Do not fit a 1280-unit desktop shell
into a phone width. Rotation and keyboard height changes must not select an
unrelated horizontal density. Narrow headers use a compact heading role and stack
app location beneath the root/account row; launcher cards reflow and scroll.

CSS reference pixels do not promise exact physical millimetres on screens; see
[CSS reference units](https://www.w3.org/TR/css3-values/#absolute-lengths).
Browser pinch zoom remains available for personal magnification. A calibrated
physical-size requirement would need explicit host calibration, not CSS `mm`.

Monitoring keeps its fixed 16:10 chart canvas fitted below its readable shell.
Designer and the experiment canvases retain their current desktop/content minima.
Opening Designer on a phone is allowed through its fitted desktop workspace;
incoming mounts qualify against their own minima while Home remains phone-sized.
This phone pass does not qualify full phone authoring or all experiment content.

## Base control catalog

| Style | Appearance and behavior |
| --- | --- |
| `button.primary` | Solid action background, action text/icon, 12-unit radius |
| `button.secondary` | Surface background, primary text, visible control stroke |
| `button.quiet` | Transparent fill, primary text; hover/pressed surface treatment |
| `field.default` | Surface fill, primary text, secondary placeholder, visible boundary |
| `toggle.default` | Track and thumb variables; position and checked semantics show state |
| `slider.default` | Track, filled track and thumb variables; generous drag target |
| `segment.default` | Selected fill plus text/mark; selection remains clear without hue |
| `panel.default` | Surface fill; decorative divider only where needed |
| `navigation.row` | Flat row, selected fill/marker, full-row activation |

A control's normal/selected/pressed/disabled treatments share geometry. Pressing a
button or dragging a slider must not resize its label or surrounding panel.
Primary hover uses the pressed fill; pressed additionally shows an inset control
stroke without changing layout. Secondary/quiet hover uses the raised surface;
pressed uses the selection fill. Disabled uses the ordinary surface with secondary
text and a decorative divider stroke, without reducing subtree opacity. Toggle
and slider disabled recipes retain their position/value but use secondary text
color for their marks. Selected state persists independently of hover. Keyboard focus adds an indicator
without replacing selected/pressed feedback. Disabled controls block activation
and show a distinct treatment; do not simply fade an entire nested subtree.

Focus uses an inset two-unit ring where clipping would hide an outer ring; the
primary button uses its contrasting `action_text` color for its inset ring.
The ring is decorative and passes input through. Dialogs retain their input
barrier. Clipping must preserve interaction with the visible part of a control.

## Modes and customization

The application root chooses **System**, **Light** or **Dark**. System follows
the host appearance; a host without a preference starts Dark. Descendants inherit
unless a container explicitly selects Light or Dark. The Designer's preview mode
is local presentation, not a write to the app's mode setting.

Changing mode resolves the same variables under another mode. It does not switch
component identity, labels, business values or interaction handlers. Color-only
changes update paint. Geometry-related customization still requests layout.

An author can override a style field with a literal color or another variable.
The inspector must say which source is active. Editing a shared variable affects
all its uses; editing an instance affects only that instance. Renaming a variable
or style preserves its identity and references.

## Readability and reference examples

Target at least **4.5:1** contrast for ordinary text, and **3:1** for visual cues
required to identify controls or states against adjacent colors. These are
focused checks, not a claim of complete WCAG conformance. OTriOS uses the stricter
ordinary-text target for the initial catalog rather than depending on large-text
exceptions. See [text contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)
and [non-text contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).

The Designer checks actual text/background usages, not variable names. It reports
unknown contrast over unresolved imagery or complex transparency. It never
silently rewrites an author's color. Color cannot be the only signal for errors,
selection, toggle state or focus.

The first reference catalog must show all base controls in Light/Dark, default,
selected, pressed, disabled and focus combinations; long labels; nested inverse
surfaces; and an explicit local override followed by Reset. Acceptance includes
actual native Frame and browser presentation, not just an HTML approximation.
While Frame is offline, qualify the active OrbStack/browser flow and shared
native scene behavior, and record physical Frame checks as deferred.

## Rules for contributors and agents

Select an existing named style before inventing a new one. Keep the style's
background, text and stroke coordinated. Add reusable variables when several
intentional uses need shared control; avoid one token for every isolated value.
Do not introduce global accent coupling, arbitrary CSS cascade, per-app renderer
forks, or decorative layers that intercept input. Keep imagery in content areas
and place controls on a predictable surface when their readability is uncertain.

Treat this document as the visual reference and RFC 0037 as the proposed technical
contract. Neither enables unsupported renderer features. Changes to the visual
reference and structured library source must be reviewed together, with the
runtime source remaining in its existing LGPL boundary.

## Designer lifecycle reference

The [Designer design package](docs/oib-designer/README.md) specifies the complete
workspace and component lifecycle, with stable UX journey IDs, technical contracts,
a representative control qualification ledger and a disposable tested prototype.
It targets landscape Frame and desktop authoring; phone portrait is unsupported.
Designer chrome now follows the shared shell palette. Preview appearance remains
a separate local mode, preserving authored literals and existing style bindings.
