<!-- SPDX-License-Identifier: Apache-2.0 -->
# OTriOS visual language

Home and The Brass Relay use charcoal surfaces, warm metal accents, restrained
outlines and the shared Roboto face. The room should feel like a useful instrument
with a little personality. Keep clear hierarchy and readable content ahead of
ornament.

## Surfaces and controls

| Role | Color |
| --- | --- |
| Background / inset field | `#171b20` |
| Raised surface | `#23282e` |
| Quiet outline | `#424951` |
| Main text | `#eff1ed` |
| Secondary text | `#acb3bb` |
| Brass action / selection | `#d8b775` |
| Copper icon | `#d0aa89` |

Use a 1.5 logical-unit keyline, 12-unit control/modal corners and 20-unit launcher
card corners. Primary actions use a brass fill with a lighter brass edge. Secondary
actions retain a charcoal fill and quiet outline. Selected avatar tiles use a
brass outline and warm dark inset, preserving icon contrast. Focus uses the same
brass accent. Let spacing do most of the grouping; avoid boxes inside boxes.

Use 24-unit modal padding, clear gaps between labels and fields, and 8-unit gaps
in the avatar grid. Center button labels by the admitted font's line metrics.
Keep Cancel secondary and the confirming action at the right.

## Headings and navigation

The lounge header reads **< OTriOS • The Brass Relay**. OTriOS is the back action;
its type size and baseline match the room title. The small brass separator ties
them together. Wide headings use text scale 4; compact headings use scale 3.
The member/profile controls sit on the same header row. Keep Home's compact
icon, title and subheading cards.

## Icons

Room avatars are 64 distinct copper-line drawings, rather than small variations
on a repeated emblem. Animals, navigation, optical instruments, tools, tea and
writing objects, automata, natural curiosities and clocks share a 32-unit grid
and a 1.6-unit stroke. Keep silhouettes recognizable at transcript size. Avoid
using color as the only identity difference.

Each picker page mixes the families. Six columns give the tiles breathing room;
the selected identity determines the opening page. The same primitive drawings
serve native GLES and browser WebGL, without image or font-icon dependencies.

## Available space

A software keyboard changes the document's available height. Reflow the chat
viewport at its existing horizontal scale; do not shrink an entire portrait
scene into a shorter rectangle or switch its design just because the keyboard
reduced the height. Preserve editor identity, value, selection and focus through
that reflow. Short views reduce the composer's visible rows and use a compact
calling card. Browser pinch zoom remains available.
