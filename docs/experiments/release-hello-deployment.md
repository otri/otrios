<!-- SPDX-License-Identifier: Apache-2.0 -->
# Stable hello and event-driven upgrades — 2026-10-19

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/release-hello-deployment.md) remain available at the pinned historical revision.

Historical deployment record. Current topology and operations follow [RFC 0009](../rfcs/0009-unified-server-session.md).


The [v1 release-control contract](../rfcs/0008-release-control.md) supersedes the
[interim five-second polling rollout](release-discovery-deployment.md).
Signed app release: `520e5d8f4430fab9d2a75ca41ee1a925d823cdf08c45b4df4559f1a804407af6`,
built from `b0779ca137ba60e6e430582f21b956432fe92cfc`. Bootstrap/supervisor and
control-service deployment source: `508e7fa3bea2dd67374d9b41aa988ff7d4437167`.
Later changes affect the standalone control service, host supervisor, checks and
service configuration; the signed app assets retain their built release hash.
The existing publisher is resumed and pinned to that deployment source.

## Behavior

Browser and frame send their current release hash and state-protocol name in a
stable v1 hello on the code host's `/release/events` WebSocket. The reply returns
the active hash even for obsolete CRDT protocols. Publication pushes an upgrade
event after the complete verified module replica and origin pointer are active.
Reconnect rereads the pointer and recovers missed notifications. Healthy clients
perform no periodic HTTP release requests. Heartbeats only detect dead channels;
all control writes have a bounded timeout. The retained HTTP pointer lets older
polling clients discover this transition.

The Mac control process runs independently of the state authority. Caddy proxies
only its public metadata event route; `/publish` stays a loopback administrative
hook. The native TLS companion also runs independently of the renderer and data
protocol. Public code downloads use an explicit `OTriOS-Release/1` identifier;
Python's default identifier returned Cloudflare 1010 on the frame. No edge access
rule or certificate check was weakened.

Disconnected clients may lose unconfirmed RAM edits during an upgrade. Connected
clients still wait for confirmed edits; active gestures always block replacement.
This is explicit in AGENTS.md, with offline edit preservation deferred. Future
protocol names in a trusted signed manifest do not prevent bootstrap execution;
signature, target, cache budgets and every asset digest remain enforced. The
present state ABI stays `otrios-canvas-sync-v2`.

## Acceptance

- Required `bash scripts/check.sh` passes with serial process tests, including the
  actual independent server and native companion. Strict all-feature/all-target
  Clippy passes. The signed browser/native build passes all 34 Node tests; all
  27 signing/supervisor tests pass. CI 113 passed for the app build commit.
- The real process test has no app-state authority. It covers current/unknown
  hello versions, future CRDT protocol names, racing publication, upgrade events,
  reconnect after a missed local notification, corrupt pointers and bad Origins.
- An isolated browser ran a signed copy of the new app while every app-state
  WebSocket returned HTTP 410. Updating only the code pointer and notifying the
  independent control process moved it automatically to the final signed release.
  Its request log contains zero HTTP `/release` requests. The test used an isolated
  signed variant differing only by an inert JavaScript comment; it was never
  published to production. Both loopback listeners and the browser tab were closed.
- The production TLS channel delivered a hello for `b7dd4d…`, then an upgrade for
  `520e5d…` immediately after activation. A final hello after restarting only the
  control service still returned the current release with `refresh: true` to an
  obsolete-state-protocol probe.
- Safari upgraded automatically, retained Component Playground navigation and
  reported `Live shared space`. The native frame reports the same signed release,
  admitted Home UI, confirmed state, no pending commands and exactly one native
  renderer (PID 28000) plus one control companion. Direct GLES2/RGB565, 800×1280,
  rotation 90 and accepted touch calibration remain qualified. Physical finger
  interaction was not re-tested; this is not a latency or GPU benchmark.
- The rollout exposed a cold-start race: a temporary `locked` presentation status
  was passed as an app route, and a transport handshake could qualify before an
  admitted app mounted. The supervisor now omits that synthetic route and requires
  an admitted, confirmed app for connected-candidate acceptance. Regression tests
  exercise both cases; the final frame is confirmed on Home.
- All 77 live public-host checks pass, including immutable code/catalog digests,
  exact control Origins, v1 hello with obsolete data protocol, denied administrative
  routes, private artifact denial and Monitoring login/logout/cookie revocation.

## Recovery and scope

Private backups retain the prior host files and configuration. Signed cached
rollback and failed-candidate retries remain available. The authority was not
restarted or its durable store modified by this deployment. DNS, tunnel, kernel,
display mode and touch calibration were not changed. Main stays at `9de7618`;
`bench/canvas-adapter-frame` stays at `abf122ab2dbd2c37c9b66d6c183a1cf860698df6`.
The existing feature PR remains unmerged.

The v1 companion/control route is now a bootstrap compatibility boundary that
must survive future app-state protocol changes. Very old clients whose state
protocol was already retired still need one launcher-root visit to acquire the
independent updater. See the [acceptance record](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/release-hello-2026-10-19.json)
and [operational contract](../FRAME_HOSTING.md#protocol-independent-release-discovery).
