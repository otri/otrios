<!-- SPDX-License-Identifier: Apache-2.0 -->
# Replayable Relay events and identity recovery — 2026-11-22

Implementation `fb96db8` adds optional validated typed descriptions to immutable
messages: Steam, Boop, Salute and Dice. The authority records targets, random seeds
and dice outcomes before its live effect broadcast. Nonce retries only return the
original receipt. `/dice` aliases `/roll`; `/boop` and `/salute` use separate match
arms with shared target lookup. Publication copies metadata without autoplay.
Existing messages retain their exact bytes when the optional event is absent.

The shared renderer generates shaded perspective d6 cubes and d20 meshes with
face markings, seeded floor/wall bounce and spin, settling onto the recorded face.
Other accepted side counts show exact numbered result blocks. Steam drifts;
Boop springs and Salute tips a hat near a visible target avatar, otherwise near
the event record. No animation frames are replicated. Off/reduced-motion stop
playback; Quiet reduces movement and particles. At most one throw and two other
effects coexist, with a 60,000-vertex replay boundary leaving headroom for focus
and typing sparks. The renderer's observed ten-d20 maximum in the focused
standalone scan was 6,582 vertices; this is geometry evidence, not a Frame FPS claim.

Replay controls use visible document anchors, clipped bounds and a separate
semantic scene routed through InputRouter. Event text and the play mark are
hittable; scrolling does not activate a replay, and captured message revisions
must still match. Keyboard/assistive activation shares first-responder state.
Replays issue no room commands and are available to admitted read-only viewers.
History stays still. Controls/effects change independently of retained UI/text
meshes. `f0f9ede` also suppresses replay overlays behind enclosing shell modals.

Managed browsers previously joined without a durable bearer and allocated a new
anonymous guest on reconnect. `/auth/session` now provisions a separate Secure,
HttpOnly, SameSite=Strict browser cookie, renewed for 400 days. It supplies a
chat-local hashed principal, never a Space grant. Verified browser profiles can
be adopted into an account through an explicit durable journal entry. Existing
account profiles win across browsers; old anonymous sockets are retired during
adoption and cannot inherit account access. Browser-bound profiles survive the
old 30-day expiry; account-bound profiles remain stable. Inactive unbound legacy
aliases can be reused without merging identities or changing transcript authors.

`c06e012` fixes a live cutover race: a new worker that briefly receives an older
status response or a network failure retries cookie readiness with one request
and one timer, exponential pacing from one second capped at 30 seconds. It never
joins anonymously before readiness, and retains the existing authentication epoch
guards. No user passwords or session cookies were copied into acceptance logs.

## Focused validation

The following Rust checks passed, nine tests total:

- `cargo test --locked -p otrios-core --features canvas --lib recorded_events_`: 2; durable recovery, authoritative results, retry deduplication, distinct commands, bounded metadata and unchanged legacy bytes.
- `cargo test --locked -p otrios-core --features canvas --lib canvas::chat::effects::tests`: 2; all d6/d20 settled faces and deterministic finite clipped geometry across phone/Frame sizes and bounded dice counts.
- `cargo test --locked -p otrios-core --features canvas --lib canvas::chat::replay::tests`: 1; quiet history, local replay, stale-revision rejection, drag safety, reduced motion and live deduplication.
- `cargo test --locked -p otrios-core --features canvas --lib browser_identity_restores_adopts_and_recovers_without_alias_or_account_hijacking`: 1; reload/reconnect, first-login adoption, existing-account priority, browser expiry, logout isolation, name protection and journal recovery.
- `cargo test --locked -p otrios-dashboard --features canvas,server,host-sync --bin canvas-server browser_identity_cookie_renews_the_same_private_bearer_without_replacing_auth_cookie`: 1; renewal flags, separate cookies and ambiguous-bearer rejection.
- `cargo test --locked -p otrios-core --features canvas --test canvas brass_relay_particles_do_not_rebuild_the_retained_ui_scene`: 1; live effects retain immutable chrome/text meshes.
- `cargo test --locked -p otrios-core --features canvas --test canvas managed_read_only_chat_is_readable_without_joining_or_enabling_composition`: 1; visible overlay controls, shared focus, local replay and read-only admission; rerun after the enclosing-modal guard.

`node scripts/test-dashboard-session.mjs`: 15 passed.
`node scripts/test-dashboard-monitoring-session.mjs`: 14 passed, including the
actual worker readiness helper under outages and old-server responses. Syntax
and scoped diff checks passed. Mac tooling, browser Wasm, ARMv7 release client and
Linux authority/checker builds passed. The final follow-up changes client overlay
presentation and browser retry behavior; the authority retains the tested v41
identity/event decoder. No broad preflight suite was run.

## Signed deployment and acceptance

Active release:
`03e8d0ba3af67e20fc298b4a5d2543952b61c55ac90852f3eba71b6564188411`.
Complete Mac/Linux signatures and digests verified, with all 85 authority files,
app contracts and catalog bytes unchanged. Both release pointers match. State
protocol is v41 on browser/native/server and publisher metadata; release-control
v1 stays unchanged. The real browser upgraded from v40 despite state negotiation
being unavailable during the codec transition.

Starting v41 against the still-active v40 manifest correctly refused state
recovery, so the import bridge returned 503. The matching old authority checked
the import guard, the verified v41 pointer was promoted, then the v41 authority
was restored. The guard was not bypassed. The cutover order is recorded in
FRAME_HOSTING.md. Final client-only promotion used the warm v41 authority.

New events/adoption journal entries require the new decoder. Signed fallback
`03f1ad175f7bf8fc29097903c54abc32f7e1a536688c3bf4d9deaec9a5b06d16`
uses the same v41 contracts with procedural playback disabled at compile time.
It was built and fully verified, not activated. Its pointer and compatible v41
authority executable remain in the VM's code-rollback directory. Historical v40
code is not a safe recovery target after new record variants are written.

At 390 × 720 CSS pixels, the real public browser loaded the signed code without
an application error. Participant ID, name and avatar stayed identical through
Home → Relay, full reload and the final automatic signed upgrade. Existing three
transcript records remained present. Cookie values were not read or logged.
Account adoption/cross-browser restoration are qualified by the focused fixture;
no real-account password was requested. Public chat commands were not posted.
Event rendering/replay use the focused shared-client/authority tests and inspected
procedural triangle artifacts; physical iPhone animation and Frame hardware/FPS
acceptance remain unrun. Authority, tunnel and Monitoring timer are active and
the disposable builder is stopped. Acceptance and signed-closure records are in
`~/Library/Application Support/OTriOSOrbStack/checks/relay-events-*.json`.
