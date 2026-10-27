<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0024 implementation and qualification ledger

RFC 0024 is in progress on `feat/0024-viewport-collections`. Delivery 1 has
synthetic-source library, process/scaling and shared-WebSocket qualification on
macOS and Linux. Delivery 2 remains required: chat cutover, native/browser app
consumption, structured blocks, signed deployment and final PR. The RFC is not
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
routes these bytes on the existing WebSocket. The branch declares sync v7 on
the authority, browser, shared Rust native-client constant and release publisher.
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
bytes. v7 is declared consistently in authority admission, browser and shared
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

## Remaining completion gates

1. Replace the room's authoritative VecDeque/history paging with the ordered
   collection and one journal-recovery publication owner. Preserve sent profile
   snapshots, guest identity, admission, durable send receipts, moderation and
   RAM-only drafts/effects. Establish a stopped-data rollback point and restart
   qualification; retire old history commands/store/client paging after cutover.
2. Map viewport/overscan to bounded stable-key coverage. Keep pending sends
   separate. Implement stable structured message blocks with one placement and
   clip for text/profile/avatar, block-revision layout caches, bounded reverse
   reserve, eviction and explicit expired-anchor adjustment. Preserve source
   anchors across prepend, append and width changes. Replace the prepend source
   reconstruction and separate avatar overlay.
3. Run required checks and browser/native acceptance, pixel/clipping comparisons,
   touch/typist replays and actual native page-flip measurements. Record selected
   data, merge, layout, frame and GPU budgets separately. Verify reconnect,
   revocation and renderer backpressure on both hosts.
4. Build/sign/verify the complete release, deploy to the existing frame/web
   environment with established rollback checks and make it available to test.
   Finish final branch commits, then open a non-draft PR. No PR or deployment has
   occurred for this partial implementation.
