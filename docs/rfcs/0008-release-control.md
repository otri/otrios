<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0008: stable release hello and upgrade events

Deployment/process separation is superseded by [RFC 0009](0009-unified-server-session.md). The stable v1 envelope below remains the compatibility boundary; current operation uses one server/session.


Status: implemented; deployment evidence is recorded separately. Scope: code
bootstrap only. App-state sync, authentication and Merkle transport retain their
own versions and policy. The present state protocol remains `otrios-canvas-sync-v2`;
tests deliberately use unknown future state protocols to exercise recovery.

## Contract

Connect to `/release/events` on the code host over TLS and send this UTF-8 JSON:

```json
{"type":"hello","version":1,"release":null,"state_protocol":"otrios-canvas-sync-v2"}
```

`release` is the currently running 64-digit lowercase SHA-256 manifest digest,
or null on first boot. The server sends the same stable version-1 envelope even
when the client's hello version or CRDT protocol is unfamiliar:

```json
{"type":"hello","version":1,"release":{"id":"<64 lowercase hex digits>","web_entry":"/releases/<same digest>/web/index.html","sync_protocol":"otrios-canvas-sync-v2"},"refresh":false}
```

`refresh` is true when the client's control version or state protocol differs.
A different release hash also causes loading of the current release. Publication
uses the same envelope with `type: "upgrade"`. The server advertises only its
validated local pointer, not any client-provided address. Unknown optional fields
are ignored. Preserve this v1 bootstrap route and fields indefinitely; changing
the CRDT protocol must not require decoding that protocol to discover new code.
A future control extension must retain a v1 reply that identifies current code.

## Publication, races and bounds

The module publisher verifies every signed asset and transfers the complete
replica before activating its pointer. It activates the private origin pointer
only after that transfer, then POSTs the loopback-only `/publish` hook. The hook
rereads the manifest, checks its SHA-256 and matching protocol metadata, updates
a latest-value watch channel and acknowledges activation. The immutable release
must be complete before this step. The local publisher/signing boundary is
trusted; hashes alone do not authenticate code. The frame continues to verify
Ed25519 signatures, target, bounded sizes and every artifact hash before execution.

A reconnect rereads the active pointer, subscribes, and captures the initial watch
value without an await between subscription and capture. Publication racing the
hello is therefore included in the initial snapshot or a subsequent event.
Latest-value coalescing may skip intermediate releases; clients need the newest
complete one. A lost local notification is retried, and a later reconnect also
recovers it. Invalid pointer bytes keep the last valid release.

Envelopes and WebSocket frames are capped at 8 KiB. The service permits 64 peers,
requires hello within five seconds, sends a heartbeat every twenty seconds and
expires silent peers after sixty seconds. Writes have five-second deadlines.
Browser Origins use an exact allowlist; native peers may omit Origin because
this channel exposes public code metadata only. No credentials or authority
state are sent here. Caddy exposes only the event route; the administrative hook
is bound to loopback and has no public proxy route. Reconnect delay backs off
from 250 ms to ten seconds. Healthy connections make no periodic release fetches.

## Client replacement policy

Code-control runs separately from the state authority and starts before browser
Wasm initialization. The native TLS companion also has no dependency on decoding
app-state messages. The signed manifest's state-protocol name is informative to
bootstrap, not an exact-version execution gate: the newly signed host owns its
protocol implementation. Existing target/signature/asset gates remain required.

Finish active gestures before replacing code. Connected clients also wait for
confirmed edits. For now, disconnected clients can discard unconfirmed RAM
intents during refresh. Offline intent persistence, schema migration and replay
are explicit future work, as recorded in AGENTS.md. They must not strand current
clients on obsolete code. Cache size bounds and server-owned durable state remain
unchanged. The actual CRDT protocol need not change in this increment.

## Evidence and limitations

Process tests run the actual control server and native companion without an app
state authority. They exercise hello/upgrade, unknown protocol names, racing
publication, missed-notification recovery, corrupt pointers and denied Origins.
Browser tests cover bounded envelopes, no healthy polling, timeouts/reconnect,
stale sockets, connected edit/gesture gates and discarded disconnected intents.
Signing tests accept valid future protocol metadata and reject tampered artifacts.
Live activation and browser/frame acceptance belong in the deployment report.
This is a PoC bootstrap contract, not general module compatibility negotiation,
production rollout authorization, offline edit preservation or a new state ABI.
