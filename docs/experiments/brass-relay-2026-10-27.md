<!-- SPDX-License-Identifier: Apache-2.0 -->
# Brass Relay qualification — 2026-10-27

The shared lounge and matching Home styling are implemented on `feat/brass-relay`.
[The room guide](../BRASS_RELAY.md) describes identity, commands, moderation and
resource limits. The earlier Lunar mockup remains preserved; its browser-only
checks are separate from the implemented room evidence below.

## Functional evidence

`bash scripts/check.sh` passes: workspace formatting, warning-free Clippy,
workspace tests and release/direct/frame/publication conformance checks. The
private signed-release builder also runs the core and host suites and 58 browser
worker/session/cache/GPU/input tests before building ARMv7 and Wasm. No new crate
dependency was introduced.

The core tests cover durable receipts after a lost ACK and restart, nonce
conflicts/expiry, stale draft generations and revisions, draft expiry, immutable
profile snapshots, name/text bounds, all-tab kicks and persisted cooldown,
connection-bound permissions, and torn-tail versus complete-corruption recovery.
Appending reader paragraphs preserves existing chunk Arcs and source anchors;
invalid appended text leaves the admitted source unchanged. Crowded-roster
qualification exercises member/profile/avatar dialogs and the native keyboard
at both 1280×800 and 800×1280.

A real authority-process test uses HTTP and WebSocket clients to verify:

- Exact-origin identity provisioning, Secure/HttpOnly/SameSite cookies and stable
  public profile recovery.
- Shared identity across two tabs, separate native guest credentials, and no
  moderation privilege from trusted transport or a copied guest token.
- Native Monitoring authentication and a live draft crossing connections.
- A kick reaching both target tabs while the same shared socket still mounts
  another app.
- Durable send retry and cooldown after authority restart.
- Corrupt chat storage closing the room while release/control negotiation and
  app-state recovery continue to work.

Real Chromium/WebGL acceptance used two independent browser contexts against an
earlier staged signed release. It covered matching Home cards in both orientations,
bidirectional messages, live draft disclosure and private clearing, multiline
paste, profile rename and all four avatar pages, persistent identity/history on
reload, commands, particles, member-dialog closing and responsive reflow.
Messages are rendered as text, including in the accessible message log. These
checks caught and fixed focus timing after activating chat controls.

## Native rendering

The disposable renderer probe loads the shipped chat UI/catalog and covered
client runtime. It supplies 64 dense multiline messages, 16 roster entries and,
in the effects case, three replayed draft writers updating every 120 ms. Timed
drag/release strokes traverse older transcript content. It uses the production
retained text composer, document GPU adapter and actual KMS/GLES2 presentation.
It does not insert fixture dispatches into the live room.

This is a rendering workload with sixteen roster entries, rather than a claim
of sixteen real network clients. Socket behavior is established separately by
the process and two-browser tests. It does not establish a 60 fps lock, full
screen-reader acceptance, physical typing/touch acceptance, Unicode shaping, or
long-term simultaneous-client capacity.

Initial rounded particles missed the target: 32.07 kernel-presented fps with
particles off, 28.12 with three replayed typists and particles on. Every tiny
rounded particle generated 52 triangles and repeated trigonometry. The selected
implementation uses two-triangle quads while retaining the count and lifetime.
Particles subsequently moved into an independent geometry layer so their motion
does not rebuild the main UI scene. The final synthetic probe, sampling touch at
60 Hz and reusing scenes as the native host does, measured 42.3474 presented fps
with effects off and 29.6982 fps with effects on over twenty seconds each. Effects
on still misses the 30 fps target. Its p95 presentation interval was 35.317 ms;
process CPU was 45.84% of one core. The probe executable SHA-256 was
`9e2a9242f7721c449fdc510e9250a4eff6d83cf902bdda6f98868a9980f3c26e`.
The harness changed during development, so these are not controlled before/after
speedup measurements. The remaining presentation limit has not been isolated.

The final signed build staged release
`24c0b9d15254492bf7807ee61f430456e805b9bb550da043ffb489adea0cbd9d`.
The required suite and signed builder passed. Browser acceptance predates the
final independent effects layer and long-composer regression; those changes
still require browser acceptance. No new release was activated on the frame.

All display trials restore the native service. The authority and calibration
remain independent; the accepted touch-rule SHA-256 is
`44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

## Scope and remaining acceptance

The reader's single-font LTR and whole-grapheme fallback restrictions remain.
Names use trimmed lowercase collision checks, without NFC normalization. The
local transcript window is bounded at 1,024 messages; reaching that bound while
reading older content reports incoming messages until the user returns to the
tail. Drafts, particle state, pending sends and native admin grants remain RAM
state. This is a PoC guest-session kick with cooldown, not a persistent ban.

Device interaction and user feedback remain part of release testing. PR creation
is deferred until the deployed work is ready, following the user's request.
