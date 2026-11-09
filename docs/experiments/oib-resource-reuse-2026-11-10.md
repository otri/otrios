<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB resource reuse — 2026-11-10

This follows [incremental presentation](oib-incremental-presentation-2026-11-10.md).
Signed deployment and final measurements are pending.

The normalized binding now retains its resolved baseline graph and canonical
library byte count under its exact binding leaf and admitted UI identity. Current
property addresses skip retired-baseline resolution. Property and source changes
continue to run raw causal, schema, closure, presence and descriptor validation;
immutable resident expansion is shared through the existing exact source cache.
Authority journal sync remains required before durable publication.

Retained rendering caches exact normalized primitives and groups consecutive runs
into immutable resources of at most 16 draws. Changed labels reshape/tessellate
only their affected primitive and replace its resource; unaffected resources retain
geometry and GPU ownership. A compatibility mesh remains available to exporters.
Native and Web hosts preserve painter order and move per-frame clips/paints
separately. Resources are namespaced by layer and composer reset epoch, and cleared
on font/mount/privacy/graphics resets. Existing scene, vertex and glyph limits
remain aggregate limits, with at most 32 resources per layer and a conservative
16 MiB combined UI bound. Document, reflow and oversized candidates use their
existing compatible path. Native UI and document slot ownership is exclusive.

There is no new dependency, state wire format, executable ABI or worker pool.
