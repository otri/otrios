<!-- SPDX-License-Identifier: Apache-2.0 -->
# Dashboard milestone history

These historical milestone notes record qualified increments and their evidence. For current behavior, start with the [dashboard guide](DASHBOARD.md), [app guide](../apps/README.md), and [RFC index](rfcs/README.md). Version references below describe each increment at the time, not necessarily the current protocol.

## UI, rendering and input

Live Controls now includes a [primitive component playground](COMPONENT_PLAYGROUND.md): captured slider/dial/pad gestures, independent shared values and local feedback. The editable layout demo lives in Test Playground.

Animation and 3D experiments: [presentation ownership, load controls and hardware measurements](ANIMATION.md). The [direct KMS display path](DIRECT_DISPLAY.md) bypasses Weston, with [measured renderer improvements](experiments/direct-display-validation.md) and an accepted [SDL-free Linux backend design](rfcs/0001-direct-linux-gles2.md).

[Retained text viewports and culling](rfcs/0017-retained-text-viewports.md)
and [adaptive alpha coverage storage](experiments/native-retained-simd-2026-10-27.md)
reach 56.4–56.6 kernel-presented fps for dense scrolling text on the Mali-400 frame.
The full Layout Lab demo measures about 42 fps; these averages do not imply a
60 fps lock. Its **Text scroll demo** opens a full-page, two-axis momentum workload
at `/test-playground/text-panning`, with retained geometry and viewport culling.

The [fixed point/SIMD evaluation](rfcs/0018-fixed-point-layout-simd.md)
found a faster isolated integer SIMD kernel, but no complete scrolling gain over
the optimized float path and slower rebuilds. The pilot and unsuccessful GPU
position formats are retired. [RFC 0019](rfcs/0019-native-fixed-simd-pipeline.md)
records the outcome and evidence required for any future float SIMD batches.

[The chunked document reader](rfcs/0020-chunked-text-documents.md) adds
paragraph reflow and bounded retained chunks through shared Rust layout and
native/WebGL resource adapters. Test Playground’s **Long Document** experiment opens
a 1 MiB momentum reader at `/test-playground/long-document`. Source anchors survive width
changes; rich text and editing remain the second delivery. See the
[qualification report](experiments/chunked-documents-2026-10-27.md).

[Timestamped touch history and momentum](rfcs/0021-touch-history-and-momentum.md)
share lift-off filtering across scrolling and captured controls. Repeated scroll
strokes can add remaining momentum; bounded capture/replay tools qualify the same
paths through sliders, the dial and the touch pad.

## Apps and collections

A [public module host with private Monitoring](PUBLIC_ACCESS.md) serves a verified Wasm/UI replica through the single Rust authority. Public demo controls share the photo frame's state; Monitoring uses a separate authorized subscription. Code becomes active only after the configured replica is ready.

The [Canvas app packages](../apps/README.md) are the dashboard runtime on both hosts. Home opens The Brass Relay, Test Playground and private Monitoring. All apps share the brass palette, outlined surfaces and clickable breadcrumb header. Test Playground groups Components, Layouts, Long Document, Text Panning, Text Input, 2D Animations and 3D Animations through independently admitted UI/State references, with portrait and landscape layouts. Each app owns its source dependencies and scoped state; animation clocks, editing focus and ephemeral trails stay in RAM. See [RFC 0004](rfcs/0004-canvas-behavior-and-app-packages.md) for scope and execution limits.

[The Brass Relay](BRASS_RELAY.md) adds a shared native/browser lounge with
retained history, live drafts, clockwork avatars, playful commands and
Monitoring-backed moderation. [RFC 0022](rfcs/0022-playful-shared-chat-room.md)
records the design and qualification requirements.

[Viewport-scoped collections](rfcs/0024-viewport-scoped-collections.md) now
synchronize bounded coherent message windows over the shared session. Stable
message blocks retain text/avatar placement and source anchors through window
replacement and reflow. [Browser/frame and rollback evidence](experiments/collection-windows-2026-10-28.md)
records the deployed migration and fixed-window measurements through 25,000
messages.

The [modern app pattern and retirement map](MODERN_APP_PATTERN.md) defines the target ownership model, current implementation limits and concrete gates for deleting the earlier dashboard paths.

## Merkle foundation and transport

The [bounded Merkle hierarchy and subtree transport](rfcs/0005-merkle-subtree-prototype.md)
now provide immutable incremental hashing, causal multi-value registers, scoped
replication/resync and partial-subtree joins. The live [Canvas state adapter](rfcs/0006-durable-canvas-merkle-adapter.md) uses app-scoped Merkle replication,
server-side field policy and durable intent receipts; pending edits stay in RAM.
Browser/frame [deployment acceptance](experiments/canvas-merkle-adapter-validation.md) records the cutover. The pre-Merkle baseline is tagged
`2026-10-19-canvas-apps-milestone`; performance results distinguish actual tree
updates from network, durable storage and display latency.

The [photo-frame Canvas adapter comparison](experiments/canvas-adapter-frame-benchmark.md) measures durable latency, native presentation, application/device CPU and actual replication traffic against the dated pre-Merkle milestone. It also isolates a binary-envelope encoding cost; measured results distinguish application CPU savings from total system cost.

The [practical Canvas foundation](rfcs/0010-runtime-foundation.md) now deploys
one bounded RAM intent queue and coherent UI-derived Merkle coverage, including
only the writer's own receipt. The global dashboard replica and Automerge dependency
are retired; [resource diagnostics](ARCHITECTURE-RESOURCES.md) are shared by both hosts.
[Browser/frame qualification and measurements](experiments/canvas-foundation.md)
record lower replication traffic and mixed latency results. The final
[retirement and browser/frame acceptance](experiments/runtime-retirement.md)
closes this bounded foundation milestone, including explicit validation costs.

The [bounded replication increments](rfcs/0007-bounded-replication-pipeline.md)
fix opaque-byte expansion, make signal history opt-in and reuse bounded encoding
workspace, with a separately measured interactive-socket change. Their
[frame measurements](experiments/canvas-replication-pipeline.md) compare each
increment against its immediate control using fresh disposable authorities.
These increments are [deployed on browser and frame](experiments/release-discovery-deployment.md). Release discovery uses a stable [hello/upgrade channel](rfcs/0008-release-control.md) inside the [unified server session](rfcs/0009-unified-server-session.md), independently of state decoding ([deployment acceptance](experiments/release-hello-deployment.md)). [Sync v4 packed envelopes and measured optional compression](rfcs/0011-packed-canvas-transport.md) are deployed. Normal state traffic stays raw after frame measurements; the optional bulk compressor is bounded.

[Public UI reuse and optional iPad navigation timings](rfcs/0016-ui-cache-and-navigation-timings.md) extend the packed transport with sync v5. Repeat mounts reuse admitted public UI by hash while requesting fresh State. Open `/?timings=1` for local startup and navigation phase reports.
