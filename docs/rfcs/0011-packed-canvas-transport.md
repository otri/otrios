<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0011: Bounded packed Canvas transport

Status: implementation under qualification. Not deployed. Builds on RFC 0010;
wire changes require `otrios-canvas-sync-v4`, while release control stays v1.

## Decision

Use one small fixed-schema envelope for the four Canvas message families, with
explicit little-endian integers and typed values. Keep canonical Merkle nodes,
UI/schema FlexBuffers, hashes and stored checkpoints unchanged. No client tree or
clock becomes an authorized write. Admission, policy validation, atomic durable
publication and receipt semantics remain the authority's responsibility.

This is a pragmatic alternative to introducing generated FlatBuffers accessors
for this bounded envelope. It removes repeated field-name maps and legacy byte
vector expansion without adding generated unsafe code to the shared runtime or
another schema build tool. This decision does **not** establish a performance
win over FlatBuffers; that comparison has not been measured. Flexible app UI and
schema data remain FlexBuffers. General collection operations remain future work.

The same safe Rust encoder/decoder runs in WASM, native clients and the server.
Wire enums deliberately have no serde fallback. Old Blob/vector negotiation and
legacy wire encoders are removed. Old state clients still receive a current
release through the stable control hello before their state stream is refused.

## Packet contract

All packets start with ten bytes: `OTC4`, flags u8, reserved u8 (zero), decoded
body length u32. Flags 0 means a raw body; flag 1 means SHA-256 of the decoded
body (32 bytes) followed by one independent LZ4 block. The digest detects block
corruption; it provides neither authentication nor authorization. TLS and server
policy remain required. No dictionaries, concatenated blocks or nested compressed
inner state envelopes are accepted. Other flags/magic fail closed.

Packet and expanded body plus header are each at most 512 KiB. Raw body length
must exactly match the header; decoded output length and digest must match before
materializing typed values. Truncation, trailing bytes and wrong-family tags fail.
Lengths/counts are checked before allocation. There is no unbounded recursive
packet grammar. Unknown fields require a protocol revision rather than skipping.

Body tag is u8. All numeric fields below are little-endian. A blob is u32 length
then bytes. Text is a blob checked for UTF-8. Hash is exactly 32 bytes; writer
nonce exactly 16. Fields are positional in the listed order.

| Family/tag | Payload |
| --- | --- |
| Client 1 Mount | epoch u32, uiRef text, stateRef text, catalog hash, known-code count u8 and hashes, writer, resume u64 |
| Client 2 Accept / 3 Cancel / 5 Leave | epoch u32 |
| Client 4 Sync | epoch u32, inner packet blob |
| Server 17 View / 19 Sync | epoch u32, payload blob |
| Server 18 Mounted | epoch u32, UI blob, UI hash, code blob, state packet blob |
| Server 20 AccessRequired | epoch u32 |
| Server 21 Error | epoch u32, message text |
| State request 33 Hello | writer, resume u64 |
| State request 34 Write | sequence u64, field count u8, repeated field u16 and typed value |
| State request 35 Ack | authority u64, lease u64, sequence u64, root hash |
| State request 36 Resync | empty |
| State response 49 Mount | authority u64, lease u64, canonical frame blob |
| State response 50 Update | canonical frame blob |

Refs are at most 128 UTF-8 bytes; known code at most eight hashes; errors at most
2048 bytes; writes at most 64 fields. Value tags: 0 = f32 number; 1 = text blob
(max 4096 bytes); 2 = boolean u8 exactly 0 or 1; 3 = four color bytes; 4 = u16
point count then f32 series; 5 = u16 count then pairs of f32 timed points. Series
count is at most 2016. Wire shape checks precede the existing schema, finite-value,
range, duplicate-field, capability and receipt validation. NaN is not granted
meaning merely because f32 can represent it.

## Bounded work and ownership

Encoders offer reusable output Vec builders. A packet remains independently owned
until its transport credit is returned; scratch reuse does not recycle an in-flight
packet. Thread-local compression/decode scratch uses bounded size classes, plus
one boxed reusable 16 KiB compression table. Inner state envelopes always remain
raw; only outer messages may compress. Current-thread scratch capacities/growth
are diagnostics, not allocation counts or whole-process memory measurements.

Both runtime clients and server sessions charge declared decoded bytes before
expansion: 8 MiB per one-second monotonic fixed window. Backwards clocks cannot
reset the window. At a boundary two windows can burst; this is not a CPU deadline
or an exact rolling-second limit. Server queue capacity remains eight incoming
and four outgoing state packets, with a bounded peer count and send deadlines.
Budget exhaustion closes/reconnects the session instead of growing a backlog.
Canonical Merkle limits and one-flight subscription retention still apply.

LZ4 is optional per packet. The trial default attempts compression at 4096 body
bytes and retains it only when it saves at least 64 bytes and 12.5%, including
its integrity field. Small continuous edits stay raw. Select the deployed default
from matched frame codec and complete-client measurements; a smaller payload
alone is not acceptance. Reusable scratch does not make owned packet encoding
allocation-free.

Dependency: pinned [lz4_flex 0.14.0](https://github.com/PSeitz/lz4_flex), with default
features disabled and only std, safe-encode, safe-decode and checked-decode enabled.
Its declared Rust minimum 1.81 fits the pinned 1.90 toolchain. No native FFI,
frame/checksum library or unsafe-code exception is introduced by this increment.
The upstream block API supports caller-owned output and a reusable table.

## Qualification

Roundtrip every variant/type, golden endian bytes, wrong direction/version,
truncation at every offset, trailing data, invalid lengths/counts/UTF-8/booleans,
corrupt compressed blocks/digests, no nested expansion, scratch reuse, incompressible
fallback and decoded-rate overload. Preserve canonical hashes, atomic policy,
durable receipts, reconnect/resync, app isolation and local presentation progress.

Measure identical canonical frames through old opaque FlexBuffers, raw packed
and compressed packed envelopes. Record size and encode/decode CPU separately;
old accessor-only timings must not be represented as full old validation cost.
Then compare complete clients using frozen commits/matched signed app resources.
Deploy browser/frame together only after qualified; record live separately.

Source qualification on 2026-10-20: `bash scripts/check.sh` passes (format,
strict workspace lint, Rust unit/process/server integration suites and release/
direct-host Python checks). Browser session/catalog Node tests: 12 pass. The
wire suite includes seven packet/compression/budget tests. Frame and live browser
qualification remain pending; the deployed release continues to use sync v3.
