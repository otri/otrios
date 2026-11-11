<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer

OIB Designer is a Test Playground companion with `app:oib-designer/ui`,
`app:oib-designer/state` and business schema `oib-designer-state-v2`. Its volume,
brightness, audio-enabled and display-enabled fields are independent of Live UI's
collection and values.

The deployed document imports SettingsPanel, LabeledSlider and LabeledToggle from
the signed [OIB 2D library](../../libraries/oib-2d/README.md). Shell and the app's
presets, real instances, connections and grants stay local. Audio (20) and Display
(30) are real SettingsPanel instances; Power is a true preset without a phantom
object. SettingsPanel declares an Accessories slot. The Rust component fixture
bootstraps app builds/tests; revision tooling retains installed library pins.

Imported definitions are read-only in the app. Edit their source and publish a
signed library revision, or create a local wrapper. Shared and occurrence literal
overrides remain editable; absent assignments inherit the newly pinned default.
Renderer/cell IDs are compiler output. Full typed property addresses carry
semantic identity, including nested occurrence ancestry.

The editor supports local components/presets, shapes, text, linked references,
removal, names, literal design/placement values, slots and admitted app fields.
One generation-guarded batch validates and publishes structural changes;
properties retain their independent causal contexts. Selection, scrolling and
Edit/Test mode are local. Native and browser hosts share one responsive canvas
and task dock with Library, Layers and Inspect tabs. Test uses local sample values;
its gestures do not write the shared app state. Dark/Light currently changes editor
chrome; authored component modes arrive with named style resolution.

Current source declares state protocol v32 with release-control v1. App features include
`otrios-oib-v2`, `otrios-oib-designer-v2`, `otrios-oib-library-v1` and
`otrios-component-behavior-v1` and `otrios-auto-layout-v1`. L2 supplies a signed library-owned Slider through
bounded component Wasm ABI 1. Its initial export-contract cutover initialized a
fresh Designer collection once, with the same v2 business schema. Subsequent
compatible graph/guest updates and rollback preserve collection, epoch, app
values and an explicit width override without recompiling hosts.

See [the current guide](../../docs/OIB.md), [L1 qualification](../../docs/OIB_LIBRARY_L1.md)
and [L2 qualification](../../docs/OIB_LIBRARY_L2.md).
[D2](../../docs/OIB_D2_COMPLETION.md), [Frame Focus](../../docs/OIB_FRAME_FOCUS_D1.md)
and [Desktop Studio](../../docs/OIB_DESKTOP_STUDIO_D2.md) are dated earlier records.
D3/D4, complete authored UI transfer, images and XR remain separate work.
Wasmi executes the independent app ABI 1 and component ABI 1 contracts.
