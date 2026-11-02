<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0032: Restore authored OIB instances and UI defaults

Date: 2026-11-03. Status: **implemented; live qualification pending**.

The bounded OIB editor can remove its original controls, but Add only instantiates
the component's first signed factory. It cannot recreate a deleted Brightness
binding. Restoration is an explicit owner command, separate from allocating a new
instance or editing a property.

`InstanceOperation::Restore { defaults }` uses tag 38 and the existing durable
writer sequence/digest receipt. With `defaults=false`, the authority activates the
signed seed instances and deactivates extra allocated instances. It retains all
component/instance properties, allocations and removed identities. Original objects
regain their original identities and signed data bindings; no new identity is
allocated or recycled. Restoration therefore works after the lifetime allocation
budget is exhausted. Ordinary property edits still cannot revive removed objects.

With `defaults=true`, the same command also assigns signed component defaults and
clears every instance override. Only the OIB namespace changes. App content fields,
including current slider values, are preserved. The command observes the current
authority heads when accepted. Later concurrent edits retain the normal causal
conflict/removal behavior; this is not a generic transaction or state rollback.

Only admitted Space Admins can issue restoration. UI controls, `Client` and
`StateAdapter::restore_oib(defaults)`, and Wasm host `oib_restore(defaults)` use the
same operation. **Restore sliders** preserves the live design. **Reset UI defaults**
restores the entire signed UI default, including the original stacked geometry.
Geometry activation continues to wait for held gestures. Local inspection falls
back to an available original when its previous target is deactivated.

State protocol is **v24**, preserving release-control v1. Packages exposing the
new actions require `otrios-oib-restore-v1`. The exact predecessor layout seed is
admitted through the existing explicit compatibility allowlist. No schema fields,
services, identities, trust roots or dependencies are added.

Acceptance includes exhausted-history restoration, stable bindings and app values,
Member rejection, duplicate retries, reset recovery after an unknown outcome and
authority restart, and live use of both controls followed by reapplying the owner's
approved compact design. Retain a compatible v24 signed code recovery kit.
