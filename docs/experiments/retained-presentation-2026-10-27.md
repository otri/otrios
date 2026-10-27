<!-- SPDX-License-Identifier: Apache-2.0 -->
# Retained presentation foundation — 2026-10-27

First delivery of [RFC 0023](../rfcs/0023-retained-presentation-foundation.md).
The preceding lounge implementation is committed as `b2476c6`; no implementation
PR was opened. Stable structured document blocks and room-loop restructuring
remain separate follow-ups.

## Changed ownership

Covered core now owns one composer and immutable presentation frame, including
ordered base/document/overlay/effects layers. Both native and Wasm use it rather
than duplicating scene splitting and text composition. Public layer/revision
metadata lives in the Apache interfaces crate. No new dependency, GPU format,
state protocol or release-control envelope was introduced.

One matching renderer credit permits frame assembly. Dirty input/network turns
coalesce before composing or copying arrays; their processing continues while a
frame is outstanding. Native returns credit after presentation; browser returns
it after an animation-frame submission. The browser acknowledgement does not
prove physical scanout. The former chat-local 16 ms paint throttle is removed.

Native retains the composer's immutable mesh Arcs directly. Browser prepares one
snapshot per credit and reads its resource getters, with independent content,
placement, semantic and effect revisions. Particle-only frames transfer effects
without UI mesh copies, text-view copies or semantic-control enumeration.
Graphics recovery forces resource replay; stale acknowledgements cannot release
newer frame capacity. Scene-level invalidation and host-owned lab animation,
reflow interpolation and diagnostics remain.

## Automated evidence

- `bash scripts/check.sh`: passed, including separate-process Merkle, Canvas,
  chat authority and native-owner tests. The initial sandboxed run could not
  bind process-test sockets; the complete run with socket access passed.
- Direct/server all-target Clippy: passed with warnings denied.
- Focused worker/document/GPU/session JavaScript suite: 38 passed.
- Signed builder's worker/session/cache/GPU/input JavaScript suite: 59 passed.
- Shared composer parity covers Home, Layout Lab and lounge: exact vertices,
  glyph vertices, draw plans, styles, views and layer order against the former
  composition path. Clearing/replaying retains geometry but advances revisions.
- Particle motion/expiry preserves UI mesh identity and content, placement and
  semantic revisions. Held effect snapshots remain immutable.
- Core demand and real native-owner tests withhold renderer capacity, reject
  stale credits, and verify that the next frame contains completed navigation.
  The worker test coalesces 100 dirty requests into one subsequent composition
  and verifies effects-only transfers omit UI and semantic payloads.

## Final artifacts

Signed release:
`0942954970c38be29904d5ed8d5537eebfd8cc4a85c84a4e3ed0768e618eee52`.

Real Chromium/WebGL acceptance passed against the signed candidate using two
independent guest contexts. It covers Home in both orientations, durable messages,
live/private drafts, profile/64-avatar pages, long composer caret visibility,
multiline input, identity after reload, commands, modal closing, responsive text,
and actual `WEBGL_lose_context` loss/recovery followed by another delivered
message. The first local run lacked the test origin's explicit WebSocket grant;
correcting the disposable server configuration resolved that setup failure.

The real Mali-400 cache probe passed all 42 exact pixel cases at rotations 0/90.
The additional old/new whole-screen native capture comparison did not complete:
its fixture selected `home` as an entry path, while the catalog uses the empty
root path. The trial restored the service and console mode. Further capture
trials were deferred when the user requested immediate rollout. CPU geometry
parity and GPU cache pixel cases are separate evidence, not a completed
whole-screen native comparison.

[Machine-readable qualification](assets/retained-presentation-2026-10-27/qualification.json)
records the checks and that limitation. No FPS improvement is claimed by these
work-avoidance and geometry-parity checks.

## Activation

The signed release above is active on both trusted/public release endpoints and
the native supervisor's last-good pointer. The running native executable matches
the signed manifest SHA-256
`2e2505a7861a6266a0dbd6556032935fb31d315dc44d0e98be3aafccb58fc0e2`.
Home is confirmed with zero pending commands, `gles2-direct`, DRM, rotation 90,
800×1280 output and RGB565. Authority, native display and tunnel services are
active. The accepted touch calibration hash is unchanged.

Rollback is saved at
`/var/lib/otrios-authority/rollback/2026-10-27-brass-relay`, including the preceding
signed release, service binaries and a consistent copy of authority state.
The release is available for user testing; no PR was opened.
