<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer

OIB Designer is a Test Playground companion with its own
`app:oib-designer/ui` and `app:oib-designer/state`, using
`oib-designer-state-v2`. Volume and brightness are independent numeric slider
fields; audio and display enabled are independent numeric toggle fields. Live UI
has its own collection and app values.

The canonical signed seed contains a format and graph, with reusable definitions,
true presets and real initial instances. Audio (root 20) and Display (root 30)
are SettingsPanel instances with linked slider and toggle definitions. Power is a
preset, not a phantom instance. SettingsPanel exposes an Accessories slot.
Renderer identities, cells, selectors and layouts are derived from the graph;
they are not durable aliases. Full typed property addresses identify shared and
occurrence properties, including nested reference ancestry.

The editor supports component and preset creation, shapes, text, linked references,
deletion, renaming, literal and numeric design/placement edits, slot placement and
admitted app-field choices including unbound endpoints. The SDK also exposes
bounded hierarchy, forwarding and connection operations. One generation-guarded
structural batch applies definitions, presets and hierarchy in that order; literal
properties use separate causal contexts. An unset shared register inherits its
authored default, while observed reset preserves the correct causal override
semantics. Selection, scrolling and Edit/Test mode are local. Desktop Studio is
the browser default and Frame Focus the native default.

The app requires `otrios-oib-v2` and `otrios-oib-designer-v2`. The deployed
signed release `f642416df9b7754d39aef8b62cbb592783ea6df5989f965a45c7fd6e392ac18b`
uses state protocol v29 and release-control v1. Fresh Live UI and Designer
collections were initialized at registry generations 74 and 75, respectively.
Browser and physical Frame acceptance, including preserved app values, is recorded in the
[current OIB guide](../../docs/OIB.md). The packer signs the seed as part of the
app package; this source does not itself sign, install or deploy it.

The [D2 work record](../../docs/OIB_D2_COMPLETION.md) describes the current format,
implementation and qualification. The earlier
[Frame Focus](../../docs/OIB_FRAME_FOCUS_D1.md) and
[Desktop Studio](../../docs/OIB_DESKTOP_STUDIO_D2.md) records are dated evidence,
not current wire or geometry specifications. D3/D4, images, complete UI transfer
and XR remain deferred. [RFC 0036](../../docs/rfcs/0036-upgradable-oib-component-libraries.md)
selects signed data libraries before a component Wasm ABI; neither is implemented.
Existing Wasmi is app ABI 1 and classes/actions remain bounded host contracts.
