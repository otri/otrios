<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer D1 sample

This separate Test Playground companion owns `app:oib-designer/ui` and
`app:oib-designer/state`, using `oib-designer-state-v1`. Volume (field 1, initial
400) and brightness (2, 700) are normalized numeric sliders. Audio enabled (3,
1000) and display enabled (4, 0) are independent numeric 0/1000 toggle fields.
`Projection::Nonzero` supplies dynamic Boolean checked semantics. The existing
Live UI app, schema, factories and values are unchanged.

`app.rs` requires `otrios-oib-designer-v1` alongside OIB and instance features.
The packer signs `Seed.designer` with the rest of the UI; this source does not
sign, install or deploy a package. Parent integration owns feature discovery and
the coordinated v27 host/publisher activation. Release control remains v1.

The signed document is 1, with fixed Shell definition 11 / instance 40, and its
nonvisual Configuration node 2. The presented shell root is O2DView node 1.
It has no factory, allocation, occurrence cells or mutable selection. SettingsPanel
10 has Audio 20 and Display 30 factory roots visually attached to that shell.
Reference wrapper 7 mounts LabeledSlider 12; wrapper 8 mounts LabeledToggle 13.
Wrappers and inherited roots map to distinct existing canvases. Definition/node
names and paint order are labels and ordering, never identity inputs.

All editable properties are on component roots. Property-free text and shape
children supply the one actual paint tree; the root cell targets those descendants.
Panel accent forwards to both referenced root accents. Literal inner overrides
may mask that source, and reset reveals its current value. The two factories keep
their signed Volume/Brightness labels. Slider value and toggle checked connections
are signed local selector bindings, read-only to UI literal edits. The ordinary
text button toggles the same enable field as its neighboring toggle. The seed has
no live override; acceptance creates one through admitted causal operations.

| Relative property path | Immutable key | Shared cell | Audio alias | Display alias |
| --- | ---: | ---: | ---: | ---: |
| `[]/1/accent(1)` | 2 | 100 | 200 | 300 |
| `[7]/1/accent(1)` | 9 | 101 | 201 | 301 |
| `[7]/1/width(2)` | 17 | 102 | 202 | 302 |
| `[7]/1/label(3)` | 28 | 103 | 203 | 303 |
| `[8]/1/accent(1)` | 41 | 104 | 204 | 304 |
| `[8]/1/label(3)` | 55 | 105 | 205 | 305 |

These aliases and keys are explicit for both factories; vector order does not
allocate them. Six shared cells plus six per resident root use 18 initially, 42
at six roots, and 60 at nine residents. Ten residents require 66 and must be
rejected by the runtime's 64-cell union preflight. Descriptor capacity does not
override cell capacity. No resource ceiling is increased.

The named tooling viewports are `hierarchy-scroll`, `preview-scroll` and
`inspector-scroll`. Preview is also the fixed shell's layout root. Hierarchy is
empty in the signed layout; inspector contains only empty 1px factory placeholders
needed by the existing instance surface. Core generates local hierarchy rows and
one selected inspector, with local selection, Edit/Test mode and scroll state.
No app handler or causal cell is allocated per hierarchy row. Wide composition
places three scrolling panels side by side; portrait composition puts hierarchy
and inspector over the runtime preview, without resizing authored panel internals.

The focused covered tests live at
`crates/otrios-core/tests/oib_designer_fixture.rs`. Run after integrating the
runtime/client lanes:

```sh
cargo test -p otrios-core --features canvas --test oib_designer_fixture --locked --offline -- --nocapture
```

This exercises signed metadata, source/checked bindings, budget admission at the
highest renderer slot, shared forwarding, nested override/reset, factory labels,
unchanged app values, and checkpoint recovery. It is in-process evidence, not
Linux IPC, authority-process restart or physical/browser acceptance.
