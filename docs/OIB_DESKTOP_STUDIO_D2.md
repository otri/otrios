<!-- SPDX-License-Identifier: Apache-2.0 -->
# Desktop Studio and bounded canvas authoring

Date: 2026-11-04. Status: **completed, live-qualified and accepted, including Insert/Edit/Delete**.

Desktop Studio presents persistent Layers, runtime canvas and Properties using
Frame Focus's typed hierarchy, drafts, provenance, reset and retained scrolling.
The browser chooses Studio; native Frame chooses Focus. An explicit local
profile control switches presentations. Sidebar sizes and the profile are local
client preferences retained across app mounts; they never enter shared UI data.
Resize keeps authored preview geometry, full property addresses and app bindings.

## Active goal and completion boundary

Deliver Desktop Studio with persistent, resizable hierarchy/canvas/properties
panels, sharing Frame Focus's editing model. Then provide a palette of the
existing Audio/Display panel factories and one simpler reusable Power toggle,
with insertion, property editing, deletion and reordering inside one admitted
canvas container.

This goal is complete when:

- Desktop Studio and Frame Focus use the same selection, typed property drafts,
  provenance, reset and scrolling model; resizing is local and respects input,
  clipping and existing scene/control limits.
- An author can insert, edit and delete admitted components, and move a selected
  root earlier or later using the actual UI. Persistent Edit/Delete controls make
  the lifecycle available after insertion, including from a selected internal part.
  Inspector disclosure controls visibly show their expanded state. The starting Audio/Display document and
  existing app bindings remain intact.
- Focused qualification demonstrates stable overrides and unchanged app values
  after insertion/reorder, explicit rejection and deliberate retry of competing
  structural edits, and durable order/receipts through reconnect and authority
  restart.
- The completed code is built, signed, deployed to the existing Frame/Web
  environment and checked live, with an available compatible code rollback.
  Actual commands, results and deferred checks are documented.

Full nested/slot/definition authoring, connection editing, reusable application
text controls/content sizing, rich typography, complete portable UI transfers,
images/assets and O3D/XR are outside this goal. The next goal below is planning
only and does not expand the current implementation.

## Narrow D2 contract

This increment admits insertion and ordering of whole signed factory instances
in one container: the presented document canvas. It does not admit nested slots,
reparenting, definition structure mutation, new connections, removal repair or
arbitrary primitives. The palette names the signed factory; the inspector exposes its admitted app
binding. These are whole components, not newly composable D3 controls.

The container is derived from signed metadata: every admitted factory must be
owned by the document and visually attached to its presented root node. Each
insert specifies the exact factory prototype (so Audio and Display bindings
cannot be confused), optional `before` sibling and observed active generation.
Each reorder specifies an existing live root, optional `before` sibling and the
observed generation. `None` appends. The authority validates the entire request
before changing allocations, identity counters, property registers or the UI.

Structural operations require the observed generation to equal the authority's
active generation and a coherent draft/active graph. Competing requests against
one generation accept at most one change. A stale or inadmissible request is
acknowledged as rejected, with a bounded diagnostic and durable sequence receipt;
it does not retry automatically or overwrite the accepted graph. Refresh the
confirmed order and deliberately retry to rebase. Replayed receipts never allocate
or move twice, including across restart. No structural sequence CRDT is added.

The existing resident allocation list supplies order. Renderer slots remain
immutable per resident, while instance IDs and full relative property keys keep
all existing overrides attached to their occurrences. Projection assigns visual
order from the list without copying or clearing any surviving property register.
Insertion uses a fresh monotonic identity, signed defaults/authored assignments
and the factory's original app bindings. Existing resource caps still apply.

The opening release-control envelope remains v1. New instance-operation variants
require state protocol v28 in both browser and native/server codecs. The resident
checkpoint shape is unchanged, but the signed palette-only factory
descriptor requires a rollback runtime that understands it. Retain a verified
compatible runtime baseline; do not treat the preceding v27 binary as compatible
after this descriptor has been persisted. Code rollback never restores old
application data.

## Implementation and focused validation

Implementation commit: `3689fd1199bb028e5c561a483f921bf6737818a4`.
The initially qualified signed release was
`f92ac1a0afd6ac3d2d41545fd1e47dfef784eed7d6c0b402f048fa7cef2fe3ae`.
The full control-management follow-up below is the current active release.
The final documentation commit has identical build inputs.

Runtime implementation and tests remain LGPL-3.0-only in core. Public contracts,
app authoring, tooling and documentation remain Apache-2.0. No dependencies,
unsafe code, service, identity or trust root were added.

Actual focused commands used pinned Rust 1.90.0 with locked offline dependencies:

- `cargo test -p otrios-core --features canvas --lib oib_designer --locked
  --offline`: 33 passed on the integrated implementation. The final persistent
  rejection-heading follow-up passed its focused
  `desktop_studio_structural_rejection` test separately.
- `cargo test -p otrios-core --features canvas --test oib_designer_fixture
  --test oib_designer_runtime --locked --offline`: 14 fixture and 17 runtime
  tests passed. A subsequent focused `d2_captured_stale_client_actions` regression
  passed for both stale Insert and Reorder after a rival update was received.
- `cargo test -p otrios-core --features canvas --test oib --locked --offline
  -- --nocapture`: 35 legacy Live UI tests passed (63.03s).
- `OTRIOS_SPACES_SERVER=.../dashboard-server cargo test -p otrios-core
  --features canvas --test spaces_process d2_canvas_order --locked --offline
  -- --nocapture --test-threads=1`: one actual-authority-process test passed,
  zero ignored (10.07s on the final runtime). It covers accepted and rejected
  lost-reply receipts, replay after kill/restart, explicit retry, fresh Power
  identities, unchanged causal property bytes, app values and a separate app.
- `cargo test -p otrios-dashboard --features server,canvas,host-sync --test
  unified_session code_and_hello_survive_failed_state_recovery_and_obsolete_codecs
  --locked --offline -- --nocapture --test-threads=1`: one passed (1.06s).
- `node --test scripts/test-dashboard-session.mjs scripts/test-ui-update.mjs
  scripts/test-text-input.mjs`: 36 passed.
- Focused strict core-library Clippy, workspace formatting and whitespace checks
  passed. Browser Wasm, ARMv7 authority/native/helpers and complete signed bundle
  builds passed. Wasm compilation reported three existing dead-code warnings.

The initial Studio joint-pane test covers six roots, expanded hierarchy, selected
multi-property inspector, software keyboard, three viewport sizes and sidebar
extrema: 300 combinations, maximum 489 nodes / 92 controls under the unchanged
512 / 128 limits. Studio uses smaller retained overscan slabs; Frame Focus keeps
its qualified scrolling window. Wrong-pointer release, cancel/global cancel,
local profile switching, stale semantic IDs and typed palette actions are covered.

Broad `bash scripts/check.sh`, full platform qualification and new Frame pan/FPS
campaigns were intentionally not repeated. These tests do not establish Linux
shared-memory IPC.

## Live acceptance and rollback

The existing publisher verified complete outer and app-bundle signatures before
activation. The first candidate
`42d419ba6522bf2162348c915f3ccfa298af2532af37182d5327451a86aea62a`
activated on Frame and the signed-in browser, upgrading v27 through the unchanged
control-v1 bootstrap. The signed allowlisted palette append preserved all 18
original property/source displays and actual app values `[289,544,1000,0]`.

Actual signed-in browser controls inserted Power as fresh instance 65558;
calibrated software evdev contacts on the real Frame inserted instance 65559.
Frame applied 50% opacity to the first Power occurrence (8-bit alpha 128,
50.19608%) as generation 82, then its visible Reset restored inheritance at 83.
Browser typing `Studio power` stayed local at 83 and Enter confirmed the new
occurrence label at 84. Browser Move earlier and Frame Move later produced 85/86,
retaining that override and every original property/app value.

A live two-client probe queued one move, committed a rival move first, then sent
the stale captured intent. Exactly one order published as generation 87; the stale
intent was durably acknowledged as rejected with no additional generation or
property change. An actual authority-service restart recovered the accepted order,
Power label, rejection and app values at 87. Native renderer PID 24389 remained
running and both hosts reconnected. Deliberate retry then published 88.

The first live check found rejection detail could be offscreen in a scrolled
inspector. The final follow-up reuses the persistent Properties heading for
`Change rejected`, without adding scene nodes or input targets. Detailed notice
and explicit retry remain in the shared editing model.

Real headless Chrome screenshots were inspected after the shared preview snapshot
API timed out and the computer-use browser was unavailable. A focused
browser mouse-input check dragged the two sidebar dividers by +60 and -64 units:
left handle 336→396, right 896→832, with unchanged shared generation 85 and no
browser exception. Signed-in property and authoring checks used the existing
browser session; this was not a disposable full qualification campaign.

A compatible signed Focus-default rollback is retained privately at
`1ae32472ac09fc62ef671d7d096204f1f9ed614634ea1956c4af5a434514424c`,
with its matching authority, checker and bootstrap. Actual code rollback to this
release passed Frame readiness and fresh admitted-client checks at generation 88,
with the inserted instances, `Studio power` override, order, original overrides
and all app values intact. No application data was backed up, restored or reset.

On the final release, a second live race published generation 89. The signed-in
browser showed `Change rejected` while the detailed notice remained offscreen;
the selected `Studio power` value remained intact. Deliberate retry produced 90
and restored the normal Properties heading. Only temporary instances 65558 and
65559 were removed, and Audio/Display were returned to their original order.
Final confirmed generation 93 contains exactly the 18 original property/source
displays and unchanged `[289,544,1000,0]` app values.

The final combined `scripts/check-frame-release.py --resume-monitoring` check
passed: native/authority release hashes match, state is confirmed, input is idle,
pending commands are zero, GLES2/RGB565/rotation/touch calibration are unchanged,
and authority/native/tunnel/Monitoring services are healthy. The public
`scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only`
smoke check passed 185 checks, including signed code and session negotiation;
that smoke check does not exercise login. The signed-in authoring checks above
used the existing browser session.

[Bounded acceptance evidence](experiments/data/oib-studio-d2-2026-11-04/acceptance.json)
records generation/order/value checks, sidebar geometry, final readiness and the
actual compatible rollback. It contains only the named public fixture and
technical release facts. Raw probe scenes and screenshots are temporary local
artifacts. Temporary Frame helper binaries are removed after qualification.

The existing Git-backed watcher is promoted with
`scripts/promote-frame-release.py` only after this source is committed and
remotely reachable. Its original build environment, publisher and service
configuration are retained; small previous release-control files and the
compatible signed code baseline remain private. No app data rollback is used.


## Accepted full control-management follow-up

User acceptance required the complete Insert/Edit/Delete lifecycle before this
phase was complete. The initial runtime had causal removal but the designer did
not expose Delete. The completed follow-up adds a persistent Properties toolbar:
Edit opens the owning admitted canvas component's properties even when an
internal part is selected; Delete removes that component instance. The toolbar
stays available while Properties scrolls. Document/fixed-shell/shared-definition,
Test-mode and read-only contexts do not offer component deletion.

Info is a visible disclosure: `> Info` / `v Info`, with distinct expanded fill,
accent and checked state and Show/Hide property-info semantic labels. The open
insert palette also has a checked/selected surface. No wire format or protocol
change was needed; state remains v28.

Delete retains the existing causal presence operation and stable occurrence ID.
It respects active capture and pending confirmation, retires stale controls,
cleans only the deleted occurrence's unpublished drafts, and preserves unrelated
dirty drafts and confirmed data. Releasing an old Delete after a rival has
removed/replaced that occurrence safely refreshes local selection without touching
the replacement. Inherited child/definition structure editing remains deferred.

Actual additional checks:

- `cargo test -p otrios-core --features canvas --lib desktop_studio_ --locked
  --offline -- --nocapture`: eight passed (47.51s), including visible
  insert/edit/delete, protected contexts, stale controls and disclosure state.
- The focused `desktop_studio_delete_` filter then passed two additional
  remote-removal/dirty-draft regressions (9.26s).
- The focused `frame_focus_outlined_keyboard_and_both_panes_keep_combined_limits`
  check passed (19.37s): 407 nodes/57 controls landscape and 431/73 portrait.
  Studio's 300-case six-root joint-pane matrix peaks at 495 nodes/93 controls,
  below the unchanged 512/128 ceilings.
- Strict core Clippy with `-D warnings`, formatting and whitespace checks passed.
  Signed ARMv7 and browser Wasm builds passed. Broad preflight, repeated transport
  and restart qualification, and new Frame FPS campaigns were not rerun for this
  tooling-only follow-up.

Active signed release:
`e2102140ca214eaacf4ecc17a6acccc5d094f686bb5bd186da98ae32e8bcdf70`.
The previously qualified f92ac1a0 release and its matching host helpers were kept
as the compatible code fallback during this iteration; confirmed app data was
not restored or reset.

Actual browser mouse/keyboard input inserted qualification Power 65561, selected
its internal label, opened the root through Edit, toggled Info both ways, typed
`Studio CRUD QA` locally and confirmed it with Enter, then deleted it using the
persistent toolbar while Properties remained scrolled. Info did not publish shared
edits. The typing buffer stayed local at 101; Enter confirmed at 102 and Delete
at 103. The toolbar remained at y=136 throughout scrolling.

Calibrated software evdev contacts on the real Frame inserted qualification Power
65562 at 101, selected its internal label, used Edit and the Info disclosure,
traversed the software keyboard to Label, and typed `q` locally at 103. Enter
confirmed `Powerq` at 104; visible Delete removed the occurrence at 105. Every
contact check retained the existing calibration. These are actual device software
contacts, not a human touch observation.

[CRUD acceptance evidence](experiments/data/oib-studio-d2-2026-11-04/crud-followup.json)
records the complete lifecycle and final readiness. The user's pre-existing Power
65560 was preserved with Audio and Display in exactly the original order
`[65560,20,30]`; all pre-existing property/source displays and app values
`[289,544,1000,0]` are unchanged. Only qualification instances 65561/65562 were
deleted. Final generation 105 is confirmed and idle; native/authority hashes,
health, RGB565/rotation/calibrated touch and existing services pass the combined
readiness check. The user accepted this follow-up and authorized PR closure and
the next phase.

CI follow-up: the two metadata-only fixed-component hierarchy fixtures reused
instance 50, now reserved by the Power palette prototype. They now choose an
unused instance ID while retaining the visual-parent, ownership and fused-wrapper
assertions. `cargo test -p otrios-core --features canvas,merkle --test
oib_designer_hierarchy --test oib_designer_fixture --locked --offline` passed all
11 hierarchy and 15 fixture checks; formatting and whitespace checks passed.
This changes tests and documentation only; the qualified deployed runtime is
unchanged.

## Next goal: design-agnostic OIB and Components dogfooding

After this active goal is qualified, make OIB independent of any particular app
design and rebuild Test Playground's Components interface as an OIB document.
Use the conversion to expose and close architecture gaps rather than adding
another app-specific renderer or a privileged fixed fixture.

The OIB document should own composition, layout, styling, reusable definitions,
references and instance overrides. Signed contracts admit the available classes,
behaviors and connections; app bindings preserve the app's value ownership.
Choose and freeze a bounded authoring surface for the conversion before coding.
Do not silently fold all of D2, D3 or D4 into that next milestone.

Proposed completion evidence:

- Construct and edit the Components interface through Studio using the ordinary
  document and authoring APIs; no runtime branches keyed to the sample's design.
- Reuse definitions and nested instances with stable overrides, app bindings and
  values through live edits, concurrent edits, reconnect and restart.
- Close the portable-transfer gap before widening OIB into the existing app:
  export/import includes UI definitions, references, overrides and causal/conflict
  metadata as well as confirmed app values.
- Qualify the rebuilt interface on Frame and Web, recording any additional
  authoring/behavior capabilities explicitly selected to support the conversion.

Images/assets and O3D/XR stay separately deferred. Richer application controls,
content sizing and typography remain separately selected stages unless the next
milestone explicitly admits a narrow prerequisite.
