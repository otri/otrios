<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0034: Authored instance defaults

Date: 2026-11-03. Status: **implemented, deployed and live qualified**.
See the [Frame/Wasm acceptance record](../experiments/oib-label-defaults-2026-11-03.md).

Volume and Brightness share a component's styling, but their labels describe
different bindings. Both previously inherited the same component label and used
the same Volume-only inspector suggestions. Clearing the Brightness label could
therefore display Volume even though its identity and field binding were correct.

An inherited `Cell` may declare an optional typed `authored_override`. Effective
values resolve in this order: live instance override, authored instance override,
shared component value, signed component default. The authored layer is explicit,
including when its value equals the component default. It cannot appear on a
component definition and must satisfy the property's existing type/value bounds.

Reset clears the live override and reveals the authored value when present.
Default explicitly resolves to that authored value, otherwise the declared cell
default. Inspector diagnostics distinguish `authored`, `inherited` and live
`override`. Duplicates retain the source factory and its authored layer; restoring
a deleted factory recreates its label without retaining deleted object state.
Reset UI defaults clears live overrides while keeping the signed authored layer.

Live UI declares Volume and Brightness label defaults separately, with matching
per-factory suggestions. Width, color, spacing and geometry continue to inherit
the common component. The runtime contains no Volume/Brightness special cases;
object identity, labels and app data bindings remain separate.

Bundles using this field require `otrios-oib-authored-defaults-v1`. State protocol
is v26 for the new signed seed field; release-control v1 is unchanged. An omitted
field preserves previous canonical bytes. The explicit predecessor allowlist adds
UI SHA-256 `b202dc66dfea4e987448e7c6ae100d1cad0bf703d1a6c7d127953719250801d0`.
Under that existing signed migration path, inherited cells may revise authored
overrides and suggestions. Cell identity, type/default, owner and inheritance stay
fixed. Existing live values, component design, allocation slots/counter and app
content values are preserved. Keep a matching verified v26 code recovery kit.

Acceptance selects a recreated Brightness instance and activates its actual
Reset/Change/Default controls, confirming factory-specific labels, unchanged
Volume overrides and separate field bindings. Migration/replay and whole-UI
default reset are checked as well.
