<!-- SPDX-License-Identifier: Apache-2.0 -->
# OTriOS design reference

Date: 2026-11-11. Status: **selected visual direction; proposed style system**.
This is the human-readable reference for future OTriOS shell, Designer and base
2D controls. It does not describe a completed visual cutover. The deployed system
still uses its existing Brass Relay tokens. The executable contract, resolution
rules and delivery gates are proposed in [RFC 0037](docs/rfcs/0037-oib-design-system.md).

## Character

OTriOS should feel precise, calm and purposeful. Use monochrome surfaces, clear
hierarchy, strong headings and restrained outlines. Content supplies most color.
Controls should remain readable and recognizable on the Frame and in a browser.

The starting inspiration is the [SpaceX-inspired analysis in awesome-design-md](https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/spacex/DESIGN.md).
That third-party interpretation describes a marketing website, not an operating
system specification. We borrow its restraint and typography hierarchy; our
control states, accessibility, layout and modes are OTriOS decisions. We do not
copy SpaceX branding, photographs or font assets.

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

## Initial palette

These opaque sRGB values are the starting palette for the first style catalog.
They are design targets, not values already deployed. Validate actual rendered
combinations before acceptance, including nested and overridden surfaces.

| Variable | Dark | Light | Role |
| --- | --- | --- | --- |
| `canvas_background` | `#000000` | `#FFFFFF` | Workspace and page |
| `surface_background` | `#101010` | `#F4F4F4` | Panels and grouped content |
| `surface_raised_background` | `#1C1C1C` | `#E8E8E8` | Menus and floating surfaces |
| `text_primary` | `#F5F5F5` | `#151515` | Main text and icons |
| `text_secondary` | `#B8B8B8` | `#525252` | Descriptions and metadata |
| `divider_stroke` | `#383838` | `#D0D0D0` | Decorative separation |
| `control_stroke` | `#909090` | `#707070` | Identifiable control boundary |
| `action_background` | `#F5F5F5` | `#151515` | Primary action fill |
| `action_text` | `#151515` | `#FFFFFF` | Primary action text/icons |
| `action_background_pressed` | `#D0D0D0` | `#383838` | Pressed primary action |
| `selection_background` | `#303030` | `#DADADA` | Selected rows and segments |
| `focus_ring` | `#FFFFFF` | `#000000` | Keyboard focus indicator |

Use a transparent background only when the parent surface is known. A decorative
divider is deliberately quieter than a control boundary; do not substitute it
where the border is needed to identify the control.

Error, warning and success colors belong to explicit status styles, accompanied
by text or recognizable symbols. They are not a general brand accent. Their
exact palette is deferred until the status catalog is reviewed in both modes.

## Typography

Use the currently admitted font resources for the first implementation. A DIN-like
display face is a future asset choice requiring verified licensing and native/Web
font support; it is not a prerequisite for this visual direction.

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

Use 4-unit radii for fields, 8 for buttons, 12 for panels and 9999 for deliberate
pills or circles. A radius is clamped to the available shape. Default strokes are
1 logical unit, with final device-scale snapping verified on each host.
Avoid decorative shadows, gradients, glass and glow in the base catalog.

Layout follows container flow: horizontal, vertical, then explicitly qualified
wrap/grid support. Children use Fixed, Hug or Fill per axis, with padding, gap
and alignment. There is no constraint solver. Free positioning is explicit and
reserved for overlays or illustrations. Scroll panels when content exceeds the
viewport; avoid Previous/Next navigation for ordinary inspector content.

## Base control catalog

| Style | Appearance and behavior |
| --- | --- |
| `button.primary` | Solid action background, action text/icon, 8-unit radius |
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
