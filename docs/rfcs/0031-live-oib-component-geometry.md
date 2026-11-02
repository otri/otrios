<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0031: Live OIB component geometry

Date: 2026-11-03. Status: **implemented and live qualified**.

Extends [O2](0030-live-oib-instances.md) with declared component-owned layout
properties. The initial use is a compact LabeledSlider row: name on the left,
fixed nominal track width, percentage pinned to the right and a smaller row pitch.
This does not introduce arbitrary tree editing, reparenting, mutable bindings,
new guest imports or executable main-class APIs.

## Data and compilation

The signed layout surface declares a component numeric `preview_pitch_cell` and
an exact predecessor UI hash allowlist. Component cells map to admitted preview
nodes using X, Y, Height, FrameWidth, OffsetY and Visible targets. FrameWidth permits
small positive widths; the existing Width contract retains its 240 px container
minimum. Visible consumes a boolean. Absolute geometry is applied before additive
Y offsets, independently of cell order. All values remain finite and bounded.

Component targets are remapped onto every allocated instance's signed prototype.
No per-instance geometry cells are allocated. Removed prototype targets are omitted
from the derived scene, while allocation/property history remains retained.
Row pitch also sets each preview root's height and the scrolling content extent.
It must be at least 48 px. Painted content must fit its row, and dragging targets
must remain at least 48×32 px. Both logical orientations qualify before activation.
Existing scene, layout-byte, handler, property and allocation limits still apply.

Declared geometry edits use the existing typed `Client::edit_oib` batch and tag 37
CRDT property operation. The Wasm host exposes the same bounded batch as
`oib_edit`, accepting JSON pairs of cell ID and optional typed Value. A batch has
at most 16 cells. The component geometry is intentionally edited through code in
this increment; the graphical inspector still exposes its original property rows.
Admin authorization, durable receipts, conflict retention and viewer projection
are unchanged. Invalid geometry retains the exact previous active root.

The Live UI component's added cells are:

| ID | Property | Signed default | Compact value |
| --- | --- | --- | --- |
| 104 | label y | 0 | 10 |
| 105 | label width | 360 | 128 |
| 106 | track x | 0 | 144 |
| 107 | track y | 64 | 0 |
| 108 | track width | 520 | 288 |
| 109 | percentage y | 0 | 10 |
| 110 | percentage width | 104 | 76 |
| 111 | content y | 48 | 0 |
| 112 | row pitch | 216 | 64 |
| 113 | heading visible | true | false |

The old per-instance label cells select the displayed names. Existing percentage
selectors continue to reflect app content; applying the compact design does not
set volume or brightness values. The track retains responsive spring behavior
when an explicit instance width override narrows its container. At the normal
520 px row width its track remains 288 px. Tapping the inline name selects its
local inspector after the redundant heading is hidden.

## Admission and compatibility

Bundles require `otrios-oib-layout-v1`. State protocol is **v23** because admitted
state metadata now includes the layout surface and new target variants. Browser,
native/server codecs and publisher declarations agree. The release-control v1
opening envelope and upgrade discovery remain independent of state decoding.

The explicit layout upgrade admits the exact previously signed Live UI versions.
It preserves the main OIB/class, component/instance identities and every existing
cell's owner, name, type/default, suggestions and inheritance. Only component-owned
cells may be appended. Signed node mappings may change. New component cells are
initialized in draft and active state; old property registers, conflicts, presence,
allocations, tombstones and app values remain retained. The original exact O1
upgrade and identical-seed signed presentation updates also remain supported.
This is a bounded explicit migration, not general seed rebasing.

## Required live acceptance

Deploy and verify the layout-capable signed host/component while retaining the
stacked defaults. Then apply the compact values as one CRDT batch on the existing
Space. Record unchanged code release, native PID and mount epoch across that data
edit, agreement with a second client, retained local inspection/scroll, current
percentages and working inline-name selection. Keep a v23/layout-compatible signed
code recovery kit. Older state codecs are not compatible rollback targets.

Tests cover exact predecessor admission/journal replay, compact layout geometry,
new/removed instances, held-gesture activation, slider endpoints, geometry rejection,
unchanged app values, and process recovery. Runtime implementation/tests remain
LGPL-3.0-only; contracts, authoring, SDK/host glue and tooling remain Apache-2.0.
No dependencies, services, identities or trust roots are added.
