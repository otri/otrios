<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB incremental presentation — 2026-11-10

This follows the [measured Apply hotspots](oib-hotspots-2026-11-09.md).
The implementation removes redundant preparation before adding worker concurrency.

## Architecture

- Retain admitted typed Blueprints through source, graph, instance and placement
  transforms. Serialized artifacts retain the same byte admission limits and
  exact canonical output; internal transforms avoid deserializing their own output.
- Share authored and expanded metadata by exact source/binding leaf hashes and
  admitted UI identity. Cache canonical baseline digests with their binding.
  Equivalent active/draft compilation also shares the result regardless of selection.
- Classify effective property dependencies after inheritance and terminal-output
  precedence. Proven Color-only edits retain expanded graph, qualified layout,
  responder mappings and input identities. Numeric, text, structural and composed
  color dependencies without a complete tracked path use full compilation.
- Keep connected confirmation, held-input, focus and composition activation gates.
  Inspector value/status updates can retain hierarchy/tool models and compatible
  row identities. The first override can add a Reset row and change inspector shape.
- Separate per-draw straight RGBA from immutable vertex and glyph buffers in GLES2
  and WebGL. Color updates preserve coverage resources and need no vertex upload.
  Retained preparation admits visible primitives plus 64 logical pixels of overscan;
  an uncached primitive entering the visible clip refreshes the bounded working set.
  Exact clips, painter order and input semantics remain authoritative.

There is no new dependency, state wire format, executable ABI, worker pool or
unconfirmed speculative activation. GL ownership remains on the renderer thread.

## Validation

Live qualification and final measurements are pending while the signed candidate
is prepared. Historical D2/L1 qualification does not qualify this cleanup.
