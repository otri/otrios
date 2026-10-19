<!-- SPDX-License-Identifier: Apache-2.0 -->
# Component Playground

The app composes primitive panels, a linear slider and percentage label, rotary
dial, touch pad and accent toggle. Named source controls compile to an admitted
FlexBuffers UI with selectors, handler declarations and a bounded loadable WASM
behavior module. Slider and indicator select the same underlying field.

Pointer capture tracks a gesture outside its initial bounds. Local pending values
appear immediately; one bounded RAM queue coalesces unissued assignments and a
single immutable write waits for durable confirmation. Final release/cancel flushes
the latest value. Remote selected changes update the same bindings without making
clients materialize another app's state.

The accent toggle changes presentation color; it is not a hardware-output switch.
Pad trails are five-second ephemeral ink, allowing visitors to leave short ghost
messages. Only final pad position is durable. Coordinates use one top-left normalized
mapping in both hosts. Public visitors share the frame's demo state; Monitoring
has a separate private admission boundary.

Source, behavior and local components live in `apps/component-playground/`.
See [app packages](../apps/README.md), [ink](TOUCH_PAD_INK.md),
[dispatch](rfcs/0007-bounded-replication-pipeline.md) and
[resource ownership](ARCHITECTURE-RESOURCES.md). The former Live Controls global
room and its independent drag/presentation queues are retired.
