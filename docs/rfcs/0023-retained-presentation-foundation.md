<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0023: Retained presentation and structured documents

Status: first delivery deployed for testing; stable document blocks remain the
second delivery. [Qualification and limits](../experiments/retained-presentation-2026-10-27.md).

## Problem and scope

The Brass Relay exercises retained documents, dynamic UI and particles together.
Its integration duplicated scene splitting/composition between native and Wasm,
packed frames before discovering renderer backpressure, and imposed an app-local
16 ms paint timer in addition to host and display clocks. These boundaries make
work avoidance and performance attribution harder than individual math kernels.

Keep authority policy, durable receipts, the stable release-control envelope,
bounded queues, shared input physics and platform-owned GPU resources. This RFC
does not introduce a module system, general transactions, a new network channel,
or a fixed-point/SIMD pipeline. No dependency or state-protocol change is needed.

## Delivery 1: One retained presentation contract and renderer capacity

Covered core owns a `presentation::Composer`, immutable frame snapshots, ordered
base/document/overlay/effects layers, and explicit content, placement, semantics
and effect revisions. Both hosts consume the same composition. Document chunks
retain their existing immutable version identities and bounded upload protocol.
Backend slot numbers are implementation details, independent of the app.

One `presentation::Demand` permits one outstanding frame. Dirty requests coalesce
before scene composition, buffer copies, resource diagnostics and accessibility
enumeration. Input, physics, ordered network messages and command dispatch keep
running while presentation is blocked. Only a matching frame acknowledgement
returns capacity; stale/duplicate acknowledgements do nothing. Native returns
capacity after KMS presentation. Browser returns capacity after its animation
frame submits the accepted view, rather than immediately on message receipt.
This browser acknowledgement is submission pacing, not proof of physical scanout.

Wasm prepares one snapshot per credit. Getters read that snapshot, so mesh, view,
document and effect data cannot describe different composition turns. Browser
transfers only changed resource domains; an effects-only update does not enumerate
controls or copy UI meshes. Graphics loss resets transfer residency and forces
complete replay, without accepting stale frame acknowledgements. Private-state
revocation clears retained private resources immediately, even while a renderer
credit is outstanding. App-local paint throttling is removed.

Hosts still own animation/reflow interpolation and diagnostic overlays. Scene
identity remains the input to shared composition; fully incremental node layout
is outside this delivery. The first contract has four ordered layer categories,
not a general-purpose render graph or arbitrary material system.

Acceptance: a blocked renderer produces no additional assembled frames while
navigation/network input proceeds; stale credits cannot unblock it; the next
frame contains latest state. Particle motion and expiry leave UI content,
placement and semantic revisions unchanged. Held snapshots stay immutable.
Existing scene geometry/views and GPU pixel-reference cases must agree, browser
context recovery must replay resources, and both signed hosts must qualify.

## Delivery 2: Stable structured document blocks

[RFC 0024](0024-viewport-scoped-collections.md) connects stable message blocks to
the missing reusable viewport-scoped collection synchronization boundary.

Give messages/paragraphs stable source identities with immutable content and
separate order. Preserve existing layout summaries, chunks and viewport anchors
when prepending or appending pages. Cache layout by block revision, width, font
and style; changing one block invalidates its layout, not the transcript. Keep
memory and per-turn layout/upload caps explicit, including pinned snapshots.

Start with current plain text and message headers. Styled spans and inline
objects can later use the same block identity and layout boundary. This delivery
does not promise shaping, editable rich text, or per-character collaboration.
It must replace the current prepend source reconstruction rather than retaining
a parallel document engine.

Acceptance: prepend/append preserve unaffected chunk identities, visible content
and anchors; width changes reflow against source anchors; eviction/reload stays
bounded. Use fixed text, touch replay and typist clocks for performance baselines,
pixel comparisons and actual native page-flip timing. Do not treat measurements
from changing harnesses as controlled speedup comparisons.

## Independent follow-up

Separate chat replica/composition/presentation responsibilities and move room
commands/expiry into one room-owned serial loop. Neither requires coupling the
room journal to Canvas CRDT state or changing release discovery. These are
separate changes after the presentation boundary is established.
