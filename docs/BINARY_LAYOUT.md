<!-- SPDX-License-Identifier: Apache-2.0 -->
# Binary Canvas layouts

Each admitted Canvas UI contains a typed FlexBuffers layout: component templates,
parameter declarations, instances, child slots, responders and expressions. New
apps use `otrios-layout-v2`, including parent-local pins and proportional struts.
The interpreter also reads bounded v1 static aggregate views already produced by
Monitoring; it supplies no global dashboard state or implicit binding defaults.

The public contract lives in `otrios-interfaces::layout`. Shared authoring in
`tools/canvas/authoring.rs` compiles named controls into this data. App sources and
local dependencies live in `apps/<id>/`; Monitoring owns its read-only aggregate
view templates in its own folder. There is no embedded default dashboard asset.

Numeric frames encode as typed float vectors. Values include numbers, booleans,
text, colors, series and time-positioned series. Expressions read declared
parameters, instance dimensions and explicit local slots supplied by Canvas
selectors. A slider and label can consume the same selected field. Missing slots
are errors; there are no ambient title, enabled, samples or touch-counter bindings.

Panel is a Canvas, Superellipse background and child slot. Controls compose the
same primitives and declare app event/navigation responders; the compositor does
not contain screen-specific widgets. Input routing, painting and semantic bounds
share the resolved parent-local geometry. Primitive node identity survives layout
reconfiguration so captured gestures can continue across immutable scene changes.

Before deserializing, a safe structural guard checks reference offsets, scalar
spans, key/vector lengths and bounded traversal. The layout interpreter then
checks map keys, nesting, expression types, template/instance identity, dimensions,
geometry and responder admission. A rejected candidate leaves the current Canvas
usable. This remains a bounded interpreter, not a general scripting language.

Build current app packages and browser bindings with:

```sh
bash scripts/build-dashboard-web.sh
```

Signed release activation updates the admitted catalog/UI/code atomically; existing
clients discover it through the stable session hello/upgrade event. Durable State
references remain authority-owned. See [the app guide](../apps/README.md),
[modern pattern](MODERN_APP_PATTERN.md) and [live deployment](DASHBOARD_LIVE.md).
A public hash identifies bytes; it does not grant access or authorize execution.

The old global `default.flex`, `dashboard-layout` editor, `/layout` publication API
and automatic Automerge import are retired. Their earlier experiments remain in
Git history and experiment reports. New work should use app packages and scoped
selectors rather than reviving that global mutable room.
