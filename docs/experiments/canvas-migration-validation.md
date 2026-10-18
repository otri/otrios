<!-- SPDX-License-Identifier: Apache-2.0 -->
# Canvas app migration validation

Status: the initial signed app cutover is deployed on the public module host and
photo frame. Browser Home and Input Lab were inspected; frame Home, 3D, Animation
and private Monitoring were captured. Automated input/lease tests passed. Physical
finger interaction on the migrated build remains a user acceptance check.

## Scope

Home, Component Playground, Layout Lab, Input Lab, Animation Lab, 3D Lab and
Monitoring each supply an app-owned UI description and state schema under
`apps/<id>/`. Home navigates with admitted UI/State pairs. Playground loads its
hash-qualified Wasm reducer; ordinary controls use declared operations.

The browser and direct Linux host consume the shared Canvas client. The production
and development authorities use the same implementation. A mount qualifies the
catalog identity, UI bytes, behavior dependency and complete authoritative state
before replacing the active app. Candidate failure preserves the prior activation.
Clients retain state and pending edits in RAM; accepted authority edits persist in
separate app files. No legacy shared dashboard document is loaded or migrated.

Monitoring uses an authenticated app lease. Its collector still publishes a
standalone binary Layout aggregate, including the existing chart gap and time
semantics. The authority persists the latest aggregate separately and sends it as
a replaceable View, outside CRDT history. Anonymous clients may fetch the public
catalog metadata, but private UI, code, state and aggregate payloads require
admission. Existing sessions are checked before packets and once per second.

Touch trails remain an ephemeral stream with five-second fading. Animation phase,
focus, capture, navigation, rendering resources and diagnostic load are local.
The diagnostic CRDT workload uses two disposable documents and rolls them before
its budget is exhausted; it never writes the shared app's durable fields.

## Evidence collected

| Check | Result | Evidence |
| --- | --- | --- |
| Typed state, merge and lifecycle unit tests | 7 passed | `cargo test -p otrios-core --features canvas --lib canvas::state --offline` |
| Public policy, sign-in, expiry and ink process tests | 5 passed | `cargo test -p otrios-dashboard --features server --test public_access --offline` |
| Catalog/scope, atomic mount/persistence and static payload policy process tests | 5 passed | `cargo test -p otrios-dashboard --features server --test sync_server --offline` |
| Shared authority binaries and policy test lint | Passed, warnings denied | `cargo clippy -p otrios-dashboard --features canvas,server --bin dashboard-server --bin canvas-server --test sync_server --test public_access --offline -- -D warnings` |
| App scoping, persistence, reload and native byte-stream process proof | 1 passed in the parent validation run | `canvas_process::real_process_scoping_persistence_reload_and_native_probe` (requires `canvas,server,host-sync`) |
| Migrated browser on the actual public module host | Passed | Home renders the admitted six-app launcher; Input Lab keyboard and a literal text edit were inspected locally; 73 deployed HTTP/resource/auth checks pass |
| Migrated direct GLES host on the photo frame | Passed | Same signed catalog; acknowledged Home and actual framebuffer captures of Home, Animation, 3D and private Monitoring |

Additional checks: 22 Canvas integration tests, 4 native worker tests, 31 Node
resource/GPU/session/lifecycle tests and 20 signed-release tests pass. The full
workspace check and strict all-feature Clippy pass; the actual ARMv7 direct host
also passes strict release Clippy.

Loopback process tests ran with local networking enabled. They launch disposable
servers in temporary folders, preserving the deployed services.

The policy suite exercises a catalog that falsely marks Monitoring public. The
authority still requires sign-in before sending its UI or state. Logout and expiry
revoke the existing lease; public apps remain usable on that connection. It also
checks rejected Origins, retired global writers, forged UI/State pairs, unknown
catalog hashes and the lack of a static authority payload route.

## Explicit limits

- Authority: eight admitted catalog versions, at most sixteen apps per catalog and
  128 retained state roots. Public and trusted connections have separate peer
  limits. Incoming binary app messages are limited to 64 per second.
- State: at most 64 declared typed fields. Numbers are finite integer registers normalized to
  0–1000 (fractional state writes are rejected atomically); the current reducer ABI uses integral numeric fields 1–5. Text is limited
  to 4096 UTF-8 bytes. Series contain at most 2016 finite points; their floating-point domain and layout
  projection arithmetic are independent of the normalized integer register ABI.
- CRDT documents and transport packets are bounded to 512 KiB. New history is
  validated incrementally, including overwritten forbidden writes. History
  checkpoint/compaction is future work; budget exhaustion is an explicit error.
- UI/code: typed binary descriptions and code hashes are admitted from the
  release-bound catalog. Wasm behavior is at most 64 KiB, has no imports, and uses
  bounded fuel, stack and memory. This is not a general capability-granting ABI.
- Subscription/invalidation currently works at app-root and selected-field level.
  It is not yet Merkle subtree replication, stable array element addressing or a
  general CRDT collection schema.

The module replica verifies publisher signatures before exposing a release. The
installed authority verifies manifest identity and asset digests against that
trusted installed release. A content hash identifies bytes; it does not supply
sign-in or capability authority.

## Actual frame acceptance

Initial signed release:
`8210317c51e0ed85bcae5c57bbd04cb9bd3f67e59bd31fb6613d81b4ed70d961`.
The authority reported seven app roots; the frame readiness reported this release,
`app:home/ui`, `app:home/state`, acknowledged state, zero pending commands,
DRM/GLES2, RGB565, rotation 90 and calibrated touch. Exactly one dashboard process
owned the display, with Weston/browser absent.

A bounded on-device trial stopped the normal service before each display owner
and restored it automatically afterward. Its isolated 64-tetrahedron/30Hz RAM
CRDT workload rendered at 53.80 kernel page flips/sec after a two-second warmup;
median frame interval 17.654ms, p95 17.667ms. It performed 431 actual load updates
without writing public durable state. Composition/submission median 2.720ms,
p95 4.309ms. The fifteen-second run included a framebuffer capture that stalled
one sample for 282 ms. The panel is about 56.65 Hz; this is not a locked 60 FPS claim.
The native object-count CLI override affects presentation only, so its screenshot
retains the default 12-object app label while drawing 64 benchmark objects.

The independent Animation app mounted its own root and rendered twelve circles;
its short five-second run also included a 282 ms readback, so 49.98 measured flips/sec
is not a steady-state performance comparison. Private Monitoring mounted through
the trusted listener and rendered the existing seven-day chart projection with
correct gaps. It rendered three changed frames then idled; its 0 FPS interval field
is not an animation benchmark. Private metric screenshots are kept outside Git.
Raw public timing results are in `canvas-app-frame-results.json`.

Inspection caught a content-layer overlap at Monitoring's header. The follow-up
places aggregate content below app chrome and reserves browser sign-out space;
a regression checks both painter order and Home hit testing.

Final signed release:
`8e3466df4e54dfbb390e426a479fbc25133b922e56f0c0b48b42a489e5c60d82`.
The browser automatically adopted this release and rendered 3D Lab and the six-app
Home launcher from the public host. All 73 deployed public-resource, cookie-session
and private-payload policy checks passed again against this release. A five-second
native Monitoring run confirmed the corrected header and Home button above the
actual seven-day aggregate; its private capture remains outside Git.

After that check, the normal frame service resumed this final release on Home.
Readiness confirmed `app:home/ui`, `app:home/state`, acknowledged state, zero pending
commands, calibrated touch and direct DRM/GLES2 RGB565 output. The process inventory
showed exactly one native renderer and no Weston/browser. The normal signed release
watcher is running again. Fresh app state was used as requested; no legacy state
migration or preservation was required.

All independent client audit findings have focused passing regressions: retained
coalesced edits across reconnect, frozen late keyboard input during staged mounts,
bounded gesture arity/type validation, mandatory Monitoring privacy, two-phase
Accept/Cancel on the wire, and private expiry while newer public navigation is
pending. No permanent legacy-room writer or second prototype authority remains.

Still to check physically: migrated touch keyboard repeat/selection, drags outside
control bounds and visual panel color. Desktop/process checks do not establish
iPad interaction or physical panel acceptance. General document compaction,
Merkle replication and stable collection selectors remain future work.
