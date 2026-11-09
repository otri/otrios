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
Edit/Test mode are local. Desktop Studio is the browser default and Focus is the
native default.

Current state protocol is v30 with release-control v1. App features include
`otrios-oib-v2`, `otrios-oib-designer-v2` and `otrios-oib-library-v1`. The initial
format cutover reset only Designer and Live UI, with unchanged v2 business
schemas. Compatible library-only update, rollback and return retained collection,
epoch, app values and an explicit width override without recompiling hosts.

See [the current guide](../../docs/OIB.md) and [L1 qualification](../../docs/OIB_LIBRARY_L1.md).
[D2](../../docs/OIB_D2_COMPLETION.md), [Frame Focus](../../docs/OIB_FRAME_FOCUS_D1.md)
and [Desktop Studio](../../docs/OIB_DESKTOP_STUDIO_D2.md) are dated earlier records.
D3/D4, complete authored UI transfer, images, component Wasm ABI L2 and XR remain
separate work. Existing Wasmi executes app ABI 1.
