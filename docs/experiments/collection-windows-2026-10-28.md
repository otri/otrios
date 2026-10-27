<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0024 implementation and qualification ledger

RFC 0024 is in progress on `feat/0024-viewport-collections`. Delivery 1 has
synthetic-source library, process/scaling and shared-WebSocket qualification on
macOS and Linux. Chat's authoritative store now uses that collection owner, with
streaming journal recovery and admission lifetimes. Both host adapters now consume
bounded message windows and the legacy history read/wire paths are retired.
Stable message blocks now extend the existing document reader and pass branch
checks. Delivery 2 still requires large-thread browser and physical native
acceptance, signed deployment and the final PR. The RFC is not
complete and these branch changes have not been deployed.

## Reusable library

`otrios_core::collection` is covered runtime code, enabled by `merkle`. The
Apache-licensed `otrios_interfaces::collection` defines compiled selections and
commands. No dependencies or unsafe code were added. A feature-gating correction
in the existing presentation contracts allows the `merkle`-only build without
enabling the optional serde dependency.

The immutable ordered index has up to 32 entries per page. Leaf pages contain
stable commit keys and references to independently bounded causal record DAGs;
branch pages contain child bounds, counts, byte totals and hashes. Appends copy
the right spine. Seeking uses page bounds/counts and reads only the candidate
window, including for sparse keys. Retention drops old pages without changing
held publications or reusing the high-water identity. This is a single-authority
append model; it does not implement concurrent distributed insertion, record
editing or tombstones.

A trusted schema declares at most 32 flat field IDs and their register schemas.
Server grants are independent of the client's plan. A selection requests Tail,
Before(epoch/collection/key), or Around(stable key), at most 64 records and at
most 512 KiB of canonical selected record bytes. Each full record is bounded to
64 KiB. The current maximum authority configuration is one million records and
512 MiB of canonical index/record bytes; a host should configure a lower bound
for its memory envelope. These are rejection limits, not a capacity guarantee.

Selected membership and the projected causal records form one bounded Merkle
view. The manifest includes collection/schema identity, ordered field IDs,
history-boundary flags, ordered stable keys and projected row hashes. A replica checks the manifest,
every referenced row, every selected field/schema, byte limits and exact scope
before publishing. Reassembling the view rejects hidden extra paths and mixed
membership/record versions. Collection revision, authority epoch, request
generation and lease ID travel with the canonical Merkle transfer. A digest
identifies bytes; server admission remains authoritative.

One session holds at most one active and one replacement lease. Superseding a
request cancels the obsolete replacement. Only qualification/ACK retires the
active lease. Each lease has an ACK baseline and one in-flight view; publication
does not queue intermediate revisions. Polling a blocked session reports
`Backpressure`; explicit resync produces a fresh full offer. Revocation cancels
both leases and the client clears its active RAM view. An expired or wrong-epoch
anchor is an explicit result. Release discovery is outside this library.

The window snapshot uses the existing bounded Merkle transfer and partial-DAG
materialization, with a 13-digit base-32 path for each u64 record key. Record
field leaves fit within the existing depth limit. Selected view canonical bytes
and transfer bytes are each limited to 1 MiB; transfer/control envelopes have
separate fixed overhead bounds. A selected snapshot and its causal workspace
are separate from future row-layout, transferable-frame and GPU budgets. Hosts
must also bound externally held snapshots; arbitrary application-held clones
are not counted as free memory by the library.

### Canonical bytes

All index integers are unsigned little endian. A page starts with
`OTriOS\0collection-index\0v1\0`, then a u8 kind (0 leaf, 1 branch) and u8 entry
count. A leaf entry is `(u64 key, SHA-256 record hash, u64 record canonical bytes)`.
A branch entry is `(u64 first, u64 last, u64 count, u64 reachable canonical bytes,
SHA-256 page hash)`. Bodies are independently hashed Merkle records, never inline
index payloads. Index page size is bounded independently of collection size.

The root starts with `OTriOS\0collection-root\0v1\0`, then u64 collection ID,
schema hash, u8 field count, sorted `(u64 field, register schema hash)` pairs,
u64 high-water key, and a 0/1 root-page presence byte followed by its hash when
present. SHA-256 includes causal record metadata through the record hashes.
The empty-root digest test has a separately generated Python hashlib vector.

Window command/offer domains are `OTriOS\0collection-command\0v1\0` and
`OTriOS\0collection-offer\0v1\0`. The codecs reject truncated, extra, oversized
and invalid-tag frames before publication. The shared-session integration below
routes these bytes on the existing WebSocket. The synthetic gate used sync v7;
the consumer cutover below advances the branch consistently to v8.
The deployed environment remains on its earlier release until the full RFC
cutover is qualified. The stable v1 release-control opening envelope is unchanged.

## Local evidence

Host: Apple M2 Max, macOS 27.0.1 (26A434), arm64. Rust 1.90.0, locked dependencies,
release profile for the cost probe, debug profile for tests. Starting revision:
`23dad8f6039a06f75d868a16eecd3cf09a1555cf`; collection implementation is the
subsequent branch change. There is no hardware/display measurement in this
library report.

Commands use the existing session toolchain/cache:

```sh
export PATH=/tmp/otrios-rustup/toolchains/1.90.0-aarch64-apple-darwin/bin:$PATH
export CARGO_HOME=/tmp/otrios-cargo
export RUSTUP_HOME=/tmp/otrios-rustup
bash scripts/check.sh
cargo test -p otrios-core --features merkle --test collection --locked --offline
cargo clippy -p otrios-core --features merkle --all-targets --locked --offline -- -D warnings
cargo test -p otrios-core --features merkle --test collection_process --locked --offline -- --nocapture
cargo run -p otrios-core --features merkle --example collection_cost --release --locked --offline
```

The repository check passed with all 17 collection tests and the two-test
process suite (including its child entry point). The independent `merkle`-only
collection build also passed its 17 tests. The suite covers sparse seeks,
selected-field isolation, byte/record/authority limits, canonical convergence,
snapshot pins, competing publication, bounded slow-reader backpressure,
superseded requests, revocation, retention expiry, reconnect/resync, malformed
wire bytes and fixed-window scaling. Specific cases construct valid Merkle bytes
with inconsistent membership and record revisions, and check retention-boundary
metadata advances without changing selected row hashes.

Two real child processes qualify a tail and anchored selection from 10,002
records over bounded TCP frames. Initial plus two append updates transfer 8,257
bytes to the tail peer. The anchored peer receives 2,735 initial bytes and no
further payload for the unrelated appends. Both processes reject private-field
canaries and check stable held snapshots. Loopback tests require sandbox
escalation; they passed after correcting the test listener's accepted sockets to
blocking mode. These are process-boundary tests on macOS, not Linux shared-memory
IPC or deployed WebSocket acceptance.

### History scaling with fixed selected records

The cost probe builds distinct record bodies with deterministic sparse keys and
no warm-up. It selects the same 16 records around key 192 at each history size
and sends each qualified view to a separate RAM-only replica child process. The
child checks the canonical view hash is identical across all three sizes. RSS
comes from `ps`, includes allocator/code/process overhead, and is sampled once
per size. Three fresh-process repetitions use the same release binary.

| Total records | Index pages visited | Records considered | Authority canonical bytes | Selected record bytes | Selected view bytes | Offer wire bytes | Authority RSS KiB range | Replica RSS KiB range |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 128 | 5 | 16 | 45,329 | 4,848 | 7,151 | 7,514 | 2,784–2,864 | 2,288–2,304 |
| 4,096 | 7 | 16 | 1,450,001 | 4,848 | 7,151 | 7,514 | 9,760–9,824 | 2,352–2,384 |
| 65,536 | 9 | 16 | 23,199,767 | 4,848 | 7,151 | 7,514 | 112,384–112,432 | 2,400–2,416 |

[Raw repetitions](data/collection-windows-2026-10-28/trial-1.csv),
[second](data/collection-windows-2026-10-28/trial-2.csv),
[third](data/collection-windows-2026-10-28/trial-3.csv).
Traversal counters exclude within-page comparisons (fanout <=32), encoding,
allocator and I/O work. Authority memory grows with stored history; selected
records, selected canonical bytes and transferred bytes stay fixed. Small child
RSS changes include allocator warm-up. This is not a long-duration memory test,
network latency benchmark, claimed speedup or frame capacity measurement.

## Shared-session qualification

The covered `collection::Channel` owns a bounded registry (at most 16 sources),
server-authored field policy and per-peer collection sessions. One lane binds one
collection until Cancel; changing its window retains the active/replacement
lease contract. Authentication/admission inputs come from trusted host policy.
The thin host adapter creates a worker only after the first negotiated collection
command. Publication sends a coalesced watch hint; unchanged data is not
reselected on an app timer. A 100 ms admission check revokes expired coverage
even with an ACK pending. Private queued views are checked again before sending.

Channel 4 carries generic collection commands/events alongside Canvas fields,
ink and ephemeral room commands on the same socket. Negotiation gates all state
bytes. At this synthetic gate, v7 was declared in authority admission, browser and shared
Rust native bootstrap, release manifests, catalog verification and host tooling.
Old state negotiation still receives the v1 code hello/upgrade. Browser transport
has collection callbacks and a bounded worker bridge. Chat/native app consumers
are delivery-2 work after this synthetic-source gate.

`collection-event` v1 wraps a View, explicit generation/status rejection, or
epoch/generation/collection revocation. Outcomes include AnchorExpired, Stale,
Forbidden, Backpressure, ResyncRequired and Unavailable. Generation zero denotes
lane-level errors such as a lost mailbox baseline; it is not an ACK of a valid
selection. Truncated, extra and oversized envelopes are rejected. A failed data
lane reports resync/unavailable while retaining release and unrelated lanes.

Transport budgets are separate: eight commands of at most 1,024 bytes in the
server input mailbox; one queued response plus one being handed off, each at
most 1 MiB + 512 bytes; two browser worker response credits; 192 collection
commands/second per socket. Adjacent queued Selects coalesce before transfer
assembly without reordering ACK/Cancel. Core retains at most two leases, each
with an ACK baseline and one in-flight view. Layout, transferable presentation
snapshots and GPU budgets remain delivery-2 work.

`--collection-fixture FILE` is an explicit local qualification option, disabled
by default. It admits no network writes. A <=1,024-byte JSON file, for example
`{"records":65536,"retain_from":1}`, controls deterministic source 24. Distinct
records use sparse keys `3 * ordinal`, a public field and a private canary field.
Trusted fixture counts/retention advance monotonically; source construction and
index work remain in covered core. The file observer publishes complete prepared
revisions. Normal servers install no synthetic data.

Actual commands, in addition to the library/scaling commands above:

```sh
bash scripts/check.sh
cargo test -p otrios-dashboard --features server,canvas,host-sync \
  --test collection_session --locked --offline -- --nocapture
node --test scripts/test-dashboard-session.mjs \
  scripts/test-app-catalog.mjs scripts/test-ui-update.mjs
```

The repository check passed, as did the three real authority-process collection
tests on macOS and all 18 scoped browser session/catalog/update tests. The large
source starts with 65,536 records and grows to 65,540. An independent fast reader
confirms four separate publications while the slow reader withholds its first
ACK. The fixed 16-record sparse window around key 192 uses a 7,514-byte **offer**;
the event envelope and multiplexing tag add constant overhead. After ACK, the
slow tail reader receives the latest four appends with a 3,590-byte offer. The
anchored reader receives no outside message bodies. These are byte counts, not
latency or rendering performance claims.

The process suite also checks superseded generations, retention expiry,
fresh-RAM reconnect with canonical view convergence, and authority restart with
an explicit old-epoch cursor rejection after a new hello. A private view is
revoked when Monitoring admission expires with its offer still unacknowledged;
the same socket remains usable for public selections/control. Malformed bytes
receive explicit rejection. Core channel tests revoke policy without publishing
new data and check bounded event codecs.

The same tests ran in an isolated source directory in the existing pinned
Linux builder, leaving its live `/src` build tree untouched:

```sh
docker exec -w /tmp/otrios-rfc0024/src otrios-dashboard-live-builder \
  cargo test --target aarch64-unknown-linux-gnu -p otrios-core --features merkle \
  --test collection --test collection_channel --test collection_process --locked --offline
docker exec -w /tmp/otrios-rfc0024/src otrios-dashboard-live-builder \
  cargo test --target aarch64-unknown-linux-gnu -p otrios-dashboard \
  --features server,canvas,host-sync --test collection_session \
  --locked --offline -- --nocapture
```

Linux environment: aarch64, kernel
`7.2.12-orbstack-00394-gbecdd6130068`, Rust 1.90.0. All 17 collection tests,
two channel tests, the two-test real child-process suite and all three actual
server/driver process tests passed. Linux and macOS produce the same 7,514- and
3,590-byte offers. This demonstrates independent Linux processes and scoped
WebSocket synchronization; it does not implement the separate shared-memory
IPC PoC. No native page-flip or live-site acceptance is claimed by delivery 1.

## Chat authoritative collection: intermediate cutover

The room's writable `VecDeque<Message>` and estimated history byte counter are
retired. Source 22 now owns sent history in the shared collection registry.
Each stable sequence identifies one independently hashed causal record containing
the immutable sent message/profile snapshot. The schema is SHA-256 of
`otrios-brass-relay-message-v1`; field 1 contains its bounded FlexBuffers message.
Retention is explicit: at most 50,000 messages and 32 MiB canonical index/record
bytes. Eviction prepares a new immutable root while held snapshots remain valid.

Journal replay streams one checksummed entry at a time into a private staging
collection. It installs the source only after the entire replay qualifies; a
later corrupt record exposes no partial collection. A send or moderation notice
prepares its bounded index update before journal append/fsync, publishes after
durability, and emits a receipt afterward. Failed append leaves the active root
unchanged. Unexpected publication failure after durability stops subsequent
writes for journal recovery. Compaction streams the retained ordered records,
guests and receipts through the existing atomic replacement workflow instead
of collecting a second full transcript.

The `OTCHAT01` framing, checksums and entry shapes remain unchanged. Recovery of
a valid journal leaves its bytes unchanged; receipts survive retry/restart, and
renaming a guest leaves original sent-profile records and their hashes intact.
The live-data rollback point and signed-release rollback qualification remain
deployment work. No live journal has been rewritten or migrated by this branch.

A stable `.journal.owner.lock` sidecar uses the safe standard-library
[`File::try_lock`](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
API. Cooperating room authorities hold it through replay, append and atomic
data-file replacement. An independent process cannot open a second room owner
while it is held. The sidecar stays in place across data-file rename; it is not
unlinked while running. The existing stopped-owner deployment workflow remains
required, including for rollback to an earlier binary.

Room membership now supplies a host-local admission lifetime. Leaving, dropping
the room or kicking a participant clears it before notifications; a kick clears
every tab's lifetime. Collection policy requires this admission even for a
trusted native socket. Monitoring authentication and room admission are checked
separately. Queued views are rechecked before sending. Revocation controls are
idempotent: a send-boundary suppression and lease-worker observation may both
report the same revocation, and revocation takes priority over in-flight work.

The current UI still uses temporary Welcome/History/Message read replies. Their
page adapter reads the one collection index and owns no second writable log.
Those bodies and client paging are removed after the browser/native consumers
move to collection leases. This intermediate state does not establish bounded
chat UI traffic or structured-block/display acceptance.

Validation after this store step:

```sh
bash scripts/check.sh
cargo test -p otrios-core --features canvas --lib canvas::chat --locked --offline
cargo test -p otrios-core --features canvas --test chat_journal_process --test collection --locked --offline
cargo test -p otrios-dashboard --features server,canvas,host-sync \
  --test chat_process --test collection_session --locked --offline -- --nocapture
cargo check -p otrios-web-host --target wasm32-unknown-unknown --locked --offline
```

The required repository check and Wasm compilation passed. All 20 scoped chat
unit tests passed. They cover private staging, unchanged valid-journal recovery,
snapshot/profile preservation, failed durable append, retained-window eviction,
compaction and receipt recovery, exclusive ownership across rename, and
exhausted sequence rejection before append. The authority-side record iterator
also passes sparse ordered traversal/pinned-root tests.

The two-test real journal-owner process suite and two server/driver chat tests
passed on macOS and Linux. Actual collection traffic is denied before Join;
after Join it qualifies the original sent snapshot. Kicking the participant
revokes both an acknowledged tab and a tab withholding its first collection ACK.
Both tabs' later selections are denied. Restart/retry returns the original
receipt without duplication, and the original record hash survives recovery.
The three synthetic shared-session regression tests also pass. Linux additionally
ran the same 20 scoped chat unit tests, using the isolated builder directory
and pinned environment described above. These checks do not replace the pending
native/browser consumer cutover, pixel comparison or physical page-flip evidence.

## Native/browser consumer cutover (branch, not deployed)

The generic RAM client now owns request generations, coherent qualification,
ACK/resync, revocation and cancelled/superseded reply rejection. Its request
counter belongs to the host shell, so reopening an app cannot accept an earlier
activation's response. A rejected replacement retains the old qualified pin and
re-establishes its live coverage using a new monotonic generation. An expired
anchor carries an explicit neighboring key: first retained key at or after it,
or the retained tail if none follows. Neighbor lookup descends the bounded index;
it does not scan history. Empty collections report no neighboring key.

Brass Relay selects source 22 after room admission. It retains at most 64
projected messages and 256 KiB of selected canonical bytes, replacing its local
window rather than appending fetched pages. Visible source anchors and a
16-record edge reserve drive `Around` replacement; the actual tail uses `Tail`.
Resize does not issue a new request for unchanged coverage. An older anchored
view receives no message body or authors/layout invalidation for an outside
append; the Latest control explicitly returns to tail coverage. Optional unread
head metadata is not implemented.

The old `History` command, `History`/`Message` events, welcome message bodies,
temporary authority page adapter, 1,024-message client accumulator and roster
rewrites of sent profiles are removed. Channel 3 retains admission, guest/profile
identity, durable send receipts, moderation and ephemeral drafts/effects. Sends
remain in the separate bounded eight-intent queue. The existing shell waits for
confirmation before navigation or connected code replacement; the end-to-end
consumer test now verifies that gate and navigation resumes after the receipt.

The state wire identifier is now `otrios-canvas-sync-v8` on authority, browser,
native/shared contracts, release publisher and verification tooling. It accounts
for both the removed chat wire variants and the generic explicit expired-anchor
event. The stable v1 opening code/control envelope is unchanged. Collection
events retain the 1 MiB + 512 byte codec cap. The native inbound mailbox has eight
shared events; the browser bridge allows two collection events and one outgoing
worker handoff. These transfer bounds are separate from the selected-view cap.

Collection revocation, room removal and leaving a chat activation clear the owned
replica/projection and composer. Browser clearing also removes the accessibility
transcript and presentation resources without waiting for renderer credit.
Native handed-off frames share a revocable atomic access marker. The display
checks it every turn, drops invalid RAM resources and retires UI/document/effect
GPU slots before drawing again, independently of replacement-frame credit.
The state-worker test holds its only frame credit while the WebSocket authority
revokes access, verifies invalidation, and only then acknowledges that frame.
This tests the actual worker/network boundary; physical display timing remains
a later acceptance gate.

Validation commands (Rust 1.90.0, locked dependencies, offline Cargo):

```sh
bash scripts/check.sh
cargo test -p otrios-core --features canvas --test canvas --test collection_client --locked --offline
cargo clippy -p otrios-dashboard --features direct --bin dashboard-direct --all-targets --locked --offline -- -D warnings
cargo test -p otrios-dashboard --features direct --bin dashboard-direct --locked --offline
bash scripts/build-dashboard-web.sh
node --test scripts/test-dashboard-session.mjs scripts/test-dashboard-monitoring-session.mjs scripts/test-dashboard-artifact-cache.mjs scripts/test_gpu_scene.mjs scripts/test-app-catalog.mjs scripts/test-canvas-worker.mjs scripts/test-document-transfer.mjs scripts/test-touch-input.mjs scripts/test-text-input.mjs scripts/test-viewport.mjs scripts/test-ui-update.mjs scripts/test-canvas-host-exports.mjs scripts/test-canvas-module-sharing.mjs
node scripts/test-chat-browser.mjs --url http://127.0.0.1:18965 --chromium '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' --output target/rfc0024-client/browser
```

Required checks, Wasm release build and all eight direct-worker tests passed.
The 58 Canvas and five collection-client tests passed on macOS and Linux. The
200-message real room/Canvas integration selects 137..200 initially, replaces
older coverage without accumulation, observes no body/invalidation on an outside
append, returns to latest, resizes without a new selection, and receives the
authority's send receipt before permitting navigation. That in-process consumer
fixture has no journal; the disk/restart guarantees are qualified separately by
the process tests. Process tests on both OSes
passed the two chat and three shared-collection suites. The final ARMv7 direct
display host also compiles in the isolated Linux builder. No new dependencies.

Node 24.19.0 passed all 77 host/worker/session/cache/input checks, including actual
Wasm module cloning with separate memories, stale transport epochs, bounded
outgoing collection handoff and clearing while renderer credit is held.
Chrome 156.0.8078.48/WebGL acceptance passed using isolated local state and a test-signed
browser fixture. Its native asset is a prior test fixture; it is not a deployable
release candidate. The run covers two independent guests, durable exchange,
immutable old sender snapshots/new sender profiles on later sends, reload
identity/history, commands, responsive controls, IME/multiline editing, visual
keyboard reflow and WebGL loss/recovery. It also asserts actual channel-4 traffic
on both tabs, v1 hello/v8 negotiation, selected/replica bounds, and zero sent-body
canary leaks on the incoming ephemeral room lane. Final windows each contained
28 records, 14,976 selected canonical bytes and 18,175 replica canonical bytes.
Those small-window figures are acceptance evidence, not large-history scaling
or performance measurements. Screenshots and `collection-transport.json` are in
`target/rfc0024-client/browser`; landscape and phone captures were visually
inspected. These generated artifacts are not committed.

The browser acceptance test now waits for the completed Home activation after
leaving the Monitoring sign-in sheet before issuing its next navigation; merely
observing the retained old Home scene does not establish that the staged
handshake finished. It also checks immutable sent profiles rather than expecting
a roster rename to rewrite history.

## Immutable message blocks and shared inline resources (branch)

The existing chunked document engine accepts bounded `Source::from_blocks`
windows of shared immutable `Block`s. `BlockId { object, part }` names logical
content independently of its array index. Each block has a SHA-256 revision of
its exact text, inset, inline geometry revision and prefix-anchor attachment.
The ID is separate from that revision. Flat resources retain their original
bytes/line endings; `original_text()` explicitly returns no whole-document string
for structured windows. Hard newlines in a message are handled inside its block,
with grapheme-aligned block-local byte positions. No source flattening is used by
the live chat consumer.

Chat maps the immutable sent record to one message block: timestamp/sender label,
body and shared avatar art. Overlapping selections reuse unchanged block Arcs.
Only the current bounded window's record revisions, blocks and position map
remain owned by the chat client. The old header-number table, global byte remap,
prepend reconstruction, append/prepend string queues and scene avatar overlay
are retired. The fixed 64-avatar catalogue is shared as immutable triangle art;
all variants pass its per-resource and aggregate allocation bounds.

The reader reconciles `(BlockId, content revision)` against its existing selected
source. Unchanged height summaries, wrapping checkpoints and chunk Arcs survive
index changes at the same width. Its font/scale/paint and width are the remaining
layout-cache keys; width changes invalidate wrapping/geometry while the source
and width-independent measurements stay shared. Unselected pages are dropped,
and externally pinned old frames continue counting against the existing core
working-set cap. The generic reader's one-screen reverse and two-screen forward
prefetch and bounded CPU/GPU caches remain disposable reserves; evicted content
is reacquired through the stable-key collection plan.

Reading anchors keep logical identity/revision, local source byte and viewport Y.
A resolved reading anchor remains pinned while heights above it become exact,
including when resolution and more layout work happen in the same scheduling
turn. Explicit scrolling releases the pin. Short day labels and the leading
spacer attach to the following message's anchor, so prepending older records does
not replace the reading anchor with a transient paragraph number. Their identity
uses the following message, which also handles backwards clock/day transitions.
Diagnostics report `anchor_identity` and `anchor_revision` alongside the source
position. Existing flat-reader anchor tests continue passing.

Avatar triangles are packed into the first immutable chunk of their message's text.
Both adapters admit its geometry/glyph payload together, with one version and
one shared translation/clip. Nonzero geometry ranges opt into the existing
eight-float placement contract; zero geometry entries retain ordinary UI
placement. Native C checks integral triangle ranges just as it checks glyph
ranges. The Rust FFI ABI, unsafe module and platform/driver setup are unchanged.
Warm scrolling updates only ranges/uniforms. There is no separate avatar update
clock or cached scene overlay. Document movement no longer invalidates the chat
UI scene; direct pointer movement retains its scene Arc while the shared
document frame moves both text and art.

Budgets are distinct: chat still selects <=64 records/256 KiB canonical data;
structured source admission allows <=256 blocks, <=512 KiB text allocation,
<=128 KiB per inline allocation and <=4 MiB shared inline allocations. Allocation
identity is used only to count memory, not as a content hash. Document index
metadata remains <=8 MiB, core layout/chunk workspace <=8 MiB, with another
8 MiB reserved for host staging/transfers. GPU residency stays <=8 MiB and cold
uploads <=512 KiB per display turn. Triangle bytes count alongside glyphs in
GPU/transfer budgets, and draw plans count in GPU residency. Source records, merge workspace,
immutable block source, layout workspace and GPU residency are not one combined
"message count" budget.

Validation on this tree:

```sh
bash scripts/check.sh
cargo test -p otrios-core --features canvas --lib canvas::document --locked --offline
cargo test -p otrios-core --features canvas --test canvas --locked --offline
cargo test -p otrios-core --features canvas --lib every_avatar --locked --offline
bash scripts/build-dashboard-web.sh
node --test scripts/test-dashboard-session.mjs scripts/test-dashboard-monitoring-session.mjs scripts/test-dashboard-artifact-cache.mjs scripts/test_gpu_scene.mjs scripts/test-app-catalog.mjs scripts/test-canvas-worker.mjs scripts/test-document-transfer.mjs scripts/test-touch-input.mjs scripts/test-text-input.mjs scripts/test-viewport.mjs scripts/test-ui-update.mjs scripts/test-canvas-host-exports.mjs scripts/test-canvas-module-sharing.mjs
node scripts/test-chat-browser.mjs --url http://127.0.0.1:18965 --chromium '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' --output target/rfc0024-blocks/browser
```

The required check and release Wasm build passed. All 15 document tests and 58
Canvas tests passed on macOS and Linux; ARMv7 direct-display compilation passed
in the isolated Linux builder. New checks cover source/revision admission bounds,
hard breaks, stable chunks and source anchors across prepend/append/reflow,
prefix-label anchors, targeted revision invalidation and 33 successive 64-block
windows within memory/GPU bounds. The 200-message real room/Canvas test now drags
to the beginning of its 64-record view, acquires older coverage automatically,
and checks that the visible message/inline chunks retain both Arc identity and
placement. It still verifies outside-window silence, resize, receipts and
navigation gates. This fixture is in-process; the earlier process suites remain
the disk/network qualification, not this test's claim.

All 79 Node 24.19.0 checks passed, including shared geometry/glyph admission,
identical projection and clipping in both shader paths, warm no-upload movement,
resource retirement and held-credit privacy clearing. Actual Chrome
156.0.8078.48/WebGL acceptance passed the complete two-guest flow again using an
isolated browser-only test-signed fixture. The native asset in that fixture is a
prior test artifact and is not a deployment candidate. Final windows contained
18 records, 9,720 selected canonical bytes and 12,119 replica canonical bytes,
with zero private sent-body leaks on the ephemeral lane. Generated results and
captures are in `target/rfc0024-blocks/browser`; landscape and phone captures were
visually inspected. An actual 170-pixel transcript drag kept source allocation
at 31,201 bytes, core working storage at 543,208 bytes and resident pages at 19;
the sender revision remained unchanged. `message-block-scroll.json` records
those observations. This is small-thread functional/graphics-recovery evidence,
not the pending independent pixel comparison, large-thread browser scaling or
physical page-flip measurement. No dependencies were added.

## Remaining completion gates

1. Establish the stopped live-data rollback point and qualify the actual
   deployed single-owner journal restart/rollback workflow. The branch consumer
   cutover and old read-path retirement above are not a live deployment.
2. Qualify large-thread browser traversal and fixed-window data/merge/layout/GPU
   working-set measurements with the new block implementation. Add independent
   text/avatar pixel/clipping comparison fixtures for prepend, append and reflow;
   the structural implementation above does not establish their pixel results.
3. Run required checks and browser/native acceptance, pixel/clipping comparisons,
   touch/typist replays and actual native page-flip measurements. Record selected
   data, merge, layout, frame and GPU budgets separately. Verify reconnect,
   revocation and renderer backpressure on both hosts.
4. Build/sign/verify the complete release, deploy to the existing frame/web
   environment with established rollback checks and make it available to test.
   Finish final branch commits, then open a non-draft PR. No PR or deployment has
   occurred for this partial implementation.
