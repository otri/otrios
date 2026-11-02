# Portable app and data transfers v1

Status: combined RFC 0028 delivery implementation contract, not deployed acceptance evidence. RFC 0028
defines the experience; this document pins its payload boundary. The deployed
admin/moves milestone does not yet implement these transfers.

Development checkpoint, 2026-11-02: `feat/spaces-portable-transfers` is the single
continuation branch. It includes the retained-data and admin/move histories;
their milestone commits remain preserved by dated tags. The root checkout's
earlier combined-delivery planning edits are incorporated here with the newer
deployment facts. Shared transfer panels, the browser adapter, native private
Transfers-folder adapter and fixed codec fixtures now have local checks. The
integrated release qualification and browser/physical Frame acceptance remain
unfinished. This
checkpoint is development work, not a qualified release or a final review PR.
The continuing [combined qualification record](experiments/spaces-portable-transfers-2026-11-02.md)
distinguishes passing local checks from the remaining signed/deployed gates.

## Containers and delivery

| Payload | Filename | Media type | Expanded budget | Entry budget |
| --- | --- | --- | --- | --- |
| App | `BUNDLE_ID.otapp.zip` | `application/vnd.otrios.app+zip` | 16 MiB | 64 |
| Data | `BUNDLE_ID.otdata` | `application/vnd.otrios.app-data+zip` | 80 MiB | 64 |

App ZIP bytes are bounded at 17 MiB; data ZIP bytes at 81 MiB. Individual app
files retain RFC 0025's 512 KiB limit and 64 KiB metadata limit. Data Info and
integrity metadata each fit 64 KiB; its checkpoint fits 64 MiB. Data assets fit
the current owned-asset contract: at most 32 distinct hashes, 256 KiB per asset.
Budgets are checked before allocation and again against actual decoded bytes.
An over-budget dataset reports export unavailable; it is never truncated.

The ZIP profile uses ordinary single-disk ZIP records, Stored or raw Deflate
regular files and implicit directories. Paths are portable case-sensitive ASCII,
at most 240 bytes and eight segments, with RFC 0025's path validation and
case-alias rejection. No wrapper folder is added. Export uses sorted paths,
Stored files and fixed timestamps so fixtures are reproducible.

Read every local and central record before publishing files. Require matching
names, method, flags, CRC32, sizes and non-overlapping offsets. All local file
records must be represented exactly once in the central directory, with no
executable prefix, trailing bytes or unexplained gaps. Validate Deflate end of
stream, exact compressed-byte consumption, decoded length and CRC32. Reject
encryption, multi-disk/ZIP64 archives, links, special files, path traversal,
duplicate/case-alias paths and unsupported metadata. Version-needed values are
10 or 20; Deflate and data descriptors require 20. Allowed flags are UTF-8,
Deflate compression hints and the descriptor bit (hints are zero for Stored).
Descriptors have matching CRC/sizes, with or without their optional signature.
Local CRC/sizes either match the central record or are all zero with a descriptor.
This initial profile rejects nonempty extra fields and file/archive comments.

Independent Python-produced archive vectors live in
[`transfer-vectors.json`](../crates/otrios-core/tests/fixtures/transfer-vectors.json).
They cover Stored, Deflate, both descriptor forms and hostile aliases, traversal,
symlinks and surplus Deflate bytes. These are archive-layer fixtures, not trusted
app bundles or qualified data-checkpoint fixtures.

ZIP record interpretation follows [PKWARE APPNOTE 6.3.10](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT).
The implementation will use bounded safe Rust parsing plus pinned
`flate2 = 1.1.10`, default features disabled and `miniz_oxide` selected explicitly
for raw Deflate. Its [decoder API](https://docs.rs/flate2/1.1.10/flate2/struct.Decompress.html)
reports consumed and produced byte counts. This dependency is justified by
interoperable Deflate decoding; no ZIP encryption, native compression backend
or general-purpose archive extraction is needed. Commit its exact lock closure
and qualify Rust 1.90, Wasm and ARM builds before deployment.

## App payload

The expanded tree is the exact RFC 0025 signed bundle: Info, UI, schema, optional
behavior/resources/dependencies, notices, integrity and publisher receipt. App
bundle digest and signature rules are unchanged. Import preserves every signed
byte. A source URL, digest or signature from an unknown key cannot enroll trust.
No mutable data, host binary, credentials or Space grants belong in the archive.

Only an explicit host-authorized Share App publication creates these public
routes:

```text
GET /apps/BUNDLE_ID/BUNDLE_SHA256
GET /apps/BUNDLE_ID/BUNDLE_SHA256/download
```

The first returns HTML with the bundle's canonical name/ID, Download App, brief
import instructions and project/hosting-guide links. The second is an attachment.
Unknown or unpublished combinations expose no protected metadata. A public share
is an independent immutable object; Space audience/removal does not recall it.
Withdrawal can stop future delivery without claiming that downloaded copies vanish.

Upload accepts the attachment file or the HTTPS page URL. URL import resolves
the `/download` endpoint directly, with no HTML scraping, source cookies or host
credentials. Fetching is bounded by time, bytes, redirects and host network policy;
every resolved destination and redirect is checked. Private/local destinations
need explicit operator policy. Receive into isolated staging and verify closure,
named digest, publisher trust, compatibility and dependencies before installation.

Same ID/digest reuses installed code. A different installed digest requires an
explicit version-change review naming its baseline and the host-wide impact.
It cannot silently update all Spaces. Use the existing signed immutable-release
installation/activation workflow; the Mac signer remains on the Mac. Space Admin
can attach installed code but does not acquire installation/publishing rights.

## Signed app publication and installer intake

The completion branch implements `ShareApp` on the existing management channel.
The covered runtime requires a configured host operator and current Admin access
to the attached app's Space, plus the reviewed registry generation, catalog and
exact installed bundle digest. Publication writes only a verified code archive in
`state.apps/published-apps`; it does not alter Space labels, audience or collection
bindings. Unknown/unpublished page and download references return 404. The HTML
page uses the canonical bundle title, escapes it, provides Download App and project
links, and requires no account. Its existence is independent of later detachment
or Space access changes. Public archive responses have a separate bounded credit
held through streaming; source release directories remain protected.

`InstalledApp.bundle_sha256` supplies the verified bundle identity for review.
Each launch entry now carries only its UI and State references in `LaunchApp`;
Admin catalog facts are carried once. The maximum-name/member/Space fixture also
populates maximal revision, size and digest fields and still fits the existing
128 KiB discovery envelope. These remain v20/v5 changes within the same unreleased
combined candidate, with the v1 opening control envelope preserved.

The existing installer accepts a portable ZIP through the covered strict decoder:

```sh
canvas-bundle unpack candidate.otapp.zip candidate.otapp --key PUBLIC.pem --openssl OPENSSL
python3 scripts/install-app.py --runtime PRIVATE_RUNTIME --checker CANVAS_BUNDLE \
  --payload candidate.otapp.zip --expected-release REVIEWED_RELEASE \
  --expected-bundle absent --expected-candidate REVIEWED_CANDIDATE \
  --expected-app BUNDLE_ID --openssl OPENSSL
```

For an update, replace `absent` with the reviewed installed bundle digest. The
candidate digest is the existing bundle-manifest digest, not the ZIP hash. Intake
preserves original signed bytes and uses isolated staging. Host-ineligible launch
metadata is rejected before composing a release. Payload imports require the exact
release, installed-bundle and candidate baselines; a named source/review can also
pin its app ID. Composition and activation continue to verify publisher trust,
compatibility, dependencies and the full immutable release. No new trust root or
signing key is enrolled, and no host executable is rebuilt. Without deployment
configuration this command stages a release only; the established verified
activation/deployment step is still required.

A real authority test using independently generated OpenSSL fixture signatures
passes operator-only/Admin-only denials, reviewed-digest rejection, escaped
canonical HTML, unauthenticated download, exact signed-file closure, retained
private classification, protected source paths, unchanged confirmed checkpoints,
and continued delivery after detach and restart. Both installation process flows
pass: the existing install/update/uninstall/rollback path and the portable ZIP
path. The latter rejects foreign-key input without changing trust, rejects stale
release/bundle/candidate/app reviews, and preserves confirmed counter state across
an accepted version update. These are local integration results; native/browser
controls and integrated deployment qualification are still unfinished.

### Bounded file and URL intake

`POST /transfers/apps/SPACE` accepts `application/vnd.otrios.app+zip` or an exact
page/download URL with `text/plain; charset=utf-8`. It requires an admitted Space
Admin, the configured Origin and `X-OTriOS-Request: transfers`, before consuming
bytes or resolving a source. The single private transfer credit covers input,
network fetching, verification and the outstanding review; cancellation of HTTP
does not release the credit while detached verification still owns memory.

The receiving host currently permits public HTTPS on implicit port 443 only.
The URL is at most 512 ASCII bytes and must name `/apps/ID/DIGEST`, optionally
ending in `/download`. Credentials, query strings, fragments, explicit ports,
escaped paths and arbitrary HTML URLs are rejected. DNS names and bracketed IPv6
literals are accepted. All of each hop's at-most-eight DNS answers must pass the
public-address policy before connecting to a pinned IP; mixed public/private
answers fail. A 30-second absolute deadline covers DNS, connection, TLS and body
reads, with at most three redirects. Redirects may change host but must preserve
the app ID and digest. Each hop gets fresh destination checks and a fresh,
credential-free request. Proxy environment variables, source cookies and host
credentials are never used. TLS verifies the original hostname against the same
pinned Mozilla CA-root dependency already used by native WebSockets. Publisher
trust remains independently pinned by the host's existing publisher key.

The address policy conservatively excludes private, loopback, link-local,
multicast, documentation, benchmarking and transition ranges based on the
[IANA IPv4](https://www.iana.org/assignments/iana-ipv4-special-registry/) and
[IPv6](https://www.iana.org/assignments/iana-ipv6-special-registry/) registries.
IPv6 is limited to ordinary global unicast in `2000::/3`, excluding the special
`2001::/23`, documentation and 6to4 ranges. This is deliberately narrower than
every potentially global special-purpose address. There is no local-address
override in the covered UI or transfer protocol. The HTTPS adapter uses Rustls
and makes the already locked `webpki-roots = 1.0.9` a direct server dependency;
no new HTTP client dependency or alternate trust-root enrollment is added.

HTTP response headers are bounded to 16 KiB. Stored/chunked/close-delimited bodies
remain under the 17 MiB app ZIP budget; conflicting lengths, unsupported transfer
or content encodings, oversized chunks and trailers fail. Chunk framing has a
separate 256 KiB budget. The strict archive and signed-bundle verifier then check
complete file closure and the URL's named bundle identity, without executing it.

Successful intake returns an actor-bound ten-minute `AppReview` naming the exact
Space, audience, generation, catalog, release, installed/candidate digests,
canonical title/revision/size and host-wide update impact count. Already-installed
code can be reviewed by its Space Admin without host-operator rights; new/different
code additionally requires the configured host operator. Incompatible updates and
closed-only apps in public Spaces fail. Admission and baseline are checked again
after the delay. Staging changes no installation, attachment or data. `CancelApp`
on the existing shared session disposes only the caller's stage, including after
that caller loses Admin rights. Expiry/restart also discard uncommitted stages.

Focused checks pass for URL/IP/redirect/framing budgets and trust/review policy.
A real authority process case confirms unauthenticated/private-source denials,
unchanged confirmed data/release pointer, Admin-only reuse, actor-bound cancellation
after role revocation and nonactivation across restart. A separate local TLS
fixture confirms hostname verification before sending HTTP and absence of
ambient Cookie/Authorization headers. This does not yet qualify a complete
cross-host URL installation or the native/browser Upload interaction.

### Durable install decision and Mac signer bridge

`ImportApp` approves one actor-bound staged review on the existing Spaces channel;
`ImportStatus` reads the operation's durable outcome. The operation ID is the
review's random stage ID. A lost response/reconnect sends status reads only, never
another installation command. Responses distinguish `queued`, `armed`, `applied`
and `rejected`; only applied is installation success. Client observation checks
request ID and operation, waits for authority cursor changes between intermediate
status reads, and forgets its watch on account changes. It does not busy-poll or
mistake an unrelated management reply for install completion.

An exact installed-code reuse publishes the attachment and its applied receipt
together, preserving the existing dataset. New/different code records a queued
decision in the existing OTSP9 latest-decision ledger and writes one bounded,
private ZIP beside that ledger. Intake's RAM/work credit is released after this
durable queue operation. There is at most one active decision and 64 bounded
outcomes. Old terminal outcomes can become unknown after eviction; their IDs
are never replay authority. Queued cancellation/rejection does not alter code or
data. The original actor can cancel its queued decision after losing Admin.

The Mac's existing watcher calls `process-app-imports.py`; no new Frame signer,
signing key, service or trust root is introduced. The standalone command takes
the existing watcher lock and accepts the existing private deployment config:

```sh
python3 scripts/process-app-imports.py --config PRIVATE_DEPLOYMENT_CONFIG
```

The bridge uses the existing pinned SSH argument list to reach trusted-loopback
`/admin/app-import` metadata, outcome and payload routes. Those routes require the
signer request header, reject browser Origins and are absent from the public
listener. Each payload read re-verifies the original publisher and candidate
digest. The Mac invokes the existing installer with the reviewed release,
installed bundle, incoming bundle and app baselines, composes/signs a complete
immutable release, and writes a durable local prepared-release receipt before
deployment. Host executables and the declared protocol remain unchanged by an
app installation.

After transferring, verifying and flushing the complete candidate, the deployer
revalidates and arms the original decision **inside `.activation.lock`**, immediately
before advancing the signed pointer. Current Space Admin, host installation
policy, audience, generation, catalog, release and review expiry must still agree.
Delayed conflicts become rejected without a pointer change. Once armed, the
latest ledger fences conflicting management, portable replacement and payload
restore. Ordinary admitted app data writes continue. Other deployments also check
the fence under this same activation lock and cannot substitute a different code
release. The old pre-transfer host's missing bridge is accepted only for ordinary
one-time host upgrade, never for an app-import operation.

Verified catalog admission completes an armed decision by publishing the prepared
attachment and applied receipt together through the same latest-ledger mechanism.
It accepts only the planned release and candidate digest. Restart recovers either
side of the pointer switch; the Mac reads the outcome before resuming the exact
already-armed release. A process exit after ledger publication recovers from that
ledger before any primary-file rollback. Code updates preserve collection IDs,
confirmed content and other attachments; they never copy an older data snapshot.
Failed or mismatched activation retains the durable fence for operator recovery.

Local checks include 239 core library cases, 31 Spaces cases and 14 transfer cases,
plus the subsequently added bounded-outcome case. The install-specific cases cover
delayed authorization/generation rejection, fencing, exact release/digest checks,
restart, an actual subprocess exit after latest-ledger sync and before primary
publication, and read-only observer recovery. The signed process integration uses
the real shared session, installer and local activation lock: new code approval,
interruption before the pointer, interruption after the pointer/before notification,
restart, confirmed writes while armed, update without changing the dataset CID,
and stale review rejection. Subsequent checks also cover direct reuse/reattachment
of installed code without signer/operator rights, preserved dataset bytes, private
bridge routes, SSH package closure with unchanged host binaries/no private key,
and temporary transport failures that leave a decision queued for retry. Release
control's 35 Python cases and the five Frame-hosting cases pass. These software
checks do not qualify the browser/native
controls, remote SSH transfer on Frame or the final combined release.

## Data Info and integrity

The tree has exactly these regular files, with no code or host configuration:

```text
Info.flex
State/checkpoint.bin
Assets/SHA256                  # zero or more owned assets
Integrity/files.flex
```

`Info.flex` begins with the eight bytes `4f 54 44 41 54 41 01 00`
(`OTDATA`, version 1, reserved zero), followed by a closed FlexBuffers map:

| Field | Logical type | Value |
| --- | --- | --- |
| `format` | UTF-8 string | `otrios-app-data-v1` |
| `bundle_id` | ASCII string | Valid installed bundle ID |
| `schema_sha256` | ASCII string | Exact current Space app-policy schema SHA-256, 64 lowercase hex digits |
| `codec` | ASCII string | One of the qualified codec identifiers below |
| `codec_version` | unsigned integer | `1` |
| `checkpoint` | ASCII string | `State/checkpoint.bin` |

`Integrity/files.flex` is a closed FlexBuffers map with `format` equal to
`otrios-data-files-v1` and `files`, an ascending, strictly unique path-sorted
vector of closed maps: `path` (ASCII string), `bytes` (unsigned integer) and
`sha256` (64 lowercase hex digits). It covers every file except itself, including
the exact Info bytes. There are no optional/unlisted files. Asset filenames are
their exact-byte SHA-256 and their listed digest must agree. Integrity is
corruption detection, not a signature, capability or authorization decision.

Canonical encoding uses the locked FlexBuffers 25.12.19 serializer, unique sorted
map keys, unsigned integers and byte vectors of unsigned values 0–255. Unknown
fields, floats in integer fields, duplicate keys and noncanonical metadata reject.
Publish exact metadata/checkpoint bytes and SHA-256 conformance vectors before
shipping; round-trip equality alone is insufficient qualification.

## Qualified checkpoints

### `otrios-canvas-portable-v1`

Export the confirmed Canvas app-field subtree and retained signal-history
subtree, preserving their causal Merkle records. Exclude the authority root,
writer-admission receipts, resume windows and local optimistic intents. The
checkpoint identifies the installed schema and app-owned state reference; it
does not convey a source Space or collection binding. Import validates every
field and history record against the installed schema, constructs a fresh
authority root and writer identity, and preserves the app-owned causal records.
Do not merge imported authority receipts or grants into the target.

### `otrios-chat-portable-v1`

Export a pinned complete confirmed ordered-history snapshot, including all
retained records and its sequence high-water mark, rather than the selected UI
window. Preserve the causal record bytes, immutable historical sender profiles,
ordering, redaction gaps and exact owned-asset closure. Current history budgets
are 50,000 records and 32 MiB canonical record bytes. The portable encoding may
use up to the checkpoint budget above to include framing and causal metadata.

Exclude live presence, drafts, rate/admission state, guest secrets, private
authority principals and send receipts. Source participant IDs in historical
messages do not grant membership or writing rights. Imported records and future
messages must qualify under the current immutable-message schema; loss of
older history or substitution of a bounded transcript is not allowed.

### Exact checkpoint framing

The data container's app-level codec is `otrios-collection-portable-v1`. It
contains **every** distinct installed root, including Canvas state for chat
screens, with an optional complete chat checkpoint on those roots. A UI screen
sharing another screen's State reference does not duplicate that root. The
installed catalog supplies the primary root, schemas and chat declarations;
payload metadata cannot add, omit or reclassify them. The exact collection
schema digest covers the complete installed root set.

All integers below are unsigned little endian. Lengths count bytes. The outer
checkpoint starts with `4f54434c50010000` (`OTCLP`, version 1, two reserved zeros),
then a `u16` root count (1–16). For each root in ascending UTF-8 State-reference
order, write: `u16` reference length (1–128), reference bytes, `u32` Canvas
checkpoint length, Canvas checkpoint bytes, `u32` chat checkpoint length, and
chat checkpoint bytes. Zero chat length means this root has no chat contract.
The entire checkpoint fits 64 MiB. Duplicate/unknown/missing roots, controls in
references, unsupported chat declarations and trailing bytes reject.

A Canvas checkpoint starts with `4f54435650010000` (`OTCVP`, version 1, two
reserved zeros), then a closed canonical FlexBuffers map with `format` equal
to `otrios-canvas-portable-v1`, `scope` (State reference), `schema` (the installed
StateSchema), `fields` (byte vector), and `signals` (byte vector or null). The
whole encoding fits 1 MiB. Fields and signals use the existing canonical full
Merkle snapshot framing from `merkle/wire.rs` (store cursor epoch 0, lease 0,
sequence 1). Fields are the subtree rooted at branch 1; an app with no fields
uses an empty Merkle snapshot. Signals are branch 3, or null when absent. The
full source root and branch 2 admission receipts are excluded. A fresh authority
root is created, every installed field must exist before defaults are considered,
and all field, bucket, register and signal records qualify against installed
schemas. App-owned causal subtree hashes must remain identical.

A chat checkpoint starts with `4f54434850010000` (`OTCHP`, version 1, two
reserved zeros), followed by the 32 raw bytes of SHA-256 over
`otrios-brass-relay-message-v2`, a `u64` sequence high-water mark, and a `u32`
retained record count. Each ascending record consists of its `u64` sequence,
`u32` causal snapshot length, and exact canonical Merkle snapshot bytes. These
are the complete retained immutable records, including historical profiles and
owned attachment references. The high-water mark can exceed the final retained
key after redaction. Each row must match the current immutable-message causal
form (actor `[22;16]`, one message field), current message validation, the complete
collection budget and its stated key. Import rejects unsupported causal forms
rather than changing them. The private new journal contains only those messages
and the high-water mark; reopening must reproduce every original causal row.

The owned asset files are the exact union referenced by all qualified chat
roots. Missing files, unused files or conflicting lengths reject. The codecs
are implemented with the fixed fixtures below but remain **unqualified for
deployment** until integrated release and end-to-end delivery/replacement acceptance.
Other app codecs remain unavailable rather than exporting runtime directories.

### Fixed portable data fixtures

[`portable-data-fixtures.json`](../crates/otrios-core/tests/fixtures/portable-data-fixtures.json)
publishes exact bytes, SHA-256 hashes and expected logical values. It includes
nondefault numeric/Unicode/Boolean fields, absent and retained signal history,
an empty auxiliary root, two immutable historical sender profiles, redaction gaps
with a sequence high-water mark beyond the final retained message, and an owned
attachment inside a complete two-root `.otdata` archive. Fixtures contain synthetic
data and fixed causal actors; no source authority identity, credentials or grants.

| Frozen payload | SHA-256 |
| --- | --- |
| Canvas fields | `56a89ee11487736fc6e556342d5748d0759820f9e5f909f324acaca105eb2121` |
| Canvas with signals | `92d0084292718672b596347e2dbcde933e07e446b4966b8e2052e1fd71c07fce` |
| Empty auxiliary Canvas | `ec1f6a60b8fcfcd25147fc4261f78815db84eacfaa14be04bfd64790bae466a3` |
| Complete chat checkpoint | `88c910194c553b350bb268ed2e5aab90de3d9342910b1b2a72513547a0255c97` |
| Two-root collection checkpoint | `8f7cc25edc5c385305248ad1aa11e229d4d6a588d1ec75dcfb6b7113a184ce80` |
| Complete data archive | `730ccd65407c0cc6357ae918c1c3358d2b666310438e3066ba1e1cf0c1e4046e` |

The `portable_data_fixtures` Rust tests compare current encoding against these
frozen bytes and hashes, qualify the imported values, then reopen every staged
root, complete chat journal and asset without changing their portable bytes.
Signal history does not enable a new authority's capture policy. Truncated framing,
wrong causal subtrees and invalid history bounds reject. An independent Python
stdlib oracle checks ZIP closure, little-endian framing, causal node ordering,
SHA-256 node closure, clocks and record/high-water boundaries. It treats embedded
FlexBuffers values as opaque; canonical FlexBuffers bytes and typed semantics
are checked by the Rust fixtures, not claimed as a second FlexBuffers implementation.

Maintenance commands use the pinned toolchain, offline dependencies and a reviewed
byte/format decision before replacing any fixture:

```sh
cargo run -p otrios-core --example portable_data_fixtures --features canvas --locked --offline -- --check
cargo test -p otrios-core --features canvas --test portable_data_fixtures --locked --offline
python3 crates/otrios-core/tests/fixtures/verify-portable-data-fixtures.py
```

### Browser and Frame transport adapters

Shared core owns destination selection, current-Admin checks, exact reviews and
the explicit publication/import/replacement decisions. File/URL receipt alone
cannot install code or change a binding. Both adapters return typed, correlated
results to that core; all commits use the existing Spaces session channel.

The browser uses its file picker or a drop on the active upload panel, posts only
to the same authority, and downloads a complete bounded Blob. Supplied app URLs
are sent to the authority's qualified fetch route, never fetched with browser
credentials. Reset aborts old actor I/O, including file selection; stale completions
cannot remove a newer task. Raw JSON review bytes preserve exact `u64` values in
Wasm. Clipboard permission/failure affects Copy link only.

Frame uses `OTRIOS_TRANSFERS`, defaulting to `Transfers` inside its existing cache.
The directory is private (0700), exclusively owned by one adapter and bounded to
64 files/128 MiB. Compatible `.otapp.zip`/`.otdata` files must be private regular
files (0600) without symbolic/hard links; selection exposes single-use opaque IDs
bound to inspected file metadata. A changed file requires selection again. Users
can place payloads in this host folder; app code receives no filesystem path or
general file access. Exports stream to private temporary files, fsync, and promote
without overwriting existing files. Names include the app ID and a random suffix.
Failures/cancelled accounts remove temporary downloads. An operator can remove
unused exports from this folder when its explicit quota is full.

Native I/O runs on one bounded worker thread, independently of input/rendering and
release discovery. It sends the existing private `OTRIOS_IDENTITY_FILE` credential
only to the configured authority, uses verified TLS for remote connections and
allows plain HTTP only for a loopback authority. It accepts no redirects. The
trusted listener's authenticated, no-Origin `GET /transfers/context` supplies only
the configured public HTTPS origin for shared links and POST Origin checks; it is
absent on the public listener. The authority's original transfer checks still
apply on every request. A native account change interrupts socket I/O and invalidates
picker IDs; clipboard is unavailable on Frame and no Copy link control is shown.

Local checks pass 38 shared UI cases, 18 native owner cases (including five transfer
adapter cases), eight browser transfer cases and 15 browser worker cases. macOS
Unix socket tests require a short `TMPDIR` such as `/private/tmp`; the default long
per-user path exceeds the OS socket name limit. These checks prove local software
behavior; they do not establish final browser or physical Frame acceptance.

## Implementation progress — 2026-11-02

The completion branch now serves authenticated confirmed exports at
`GET /transfers/data/SPACE_ID/BUNDLE_ID/COLLECTION_ID` on both authority listeners.
This is a payload download, not a second session or a management mutation.
The covered runtime authorizes the exact active, detached or sealed dataset for
its current Admin and pins every installed root plus the complete owned-asset
closure. Serialization runs outside the admission lock. Delivery rechecks the
identity, current Admin role, installed schema and dataset binding before sending
and between 64 KiB chunks. One shared transfer credit bounds serializers and
outstanding response bodies, including cancelled blocking work and slow readers.
No new state-channel bytes or release-control changes are introduced by this route.

Focused validation uses the pinned Rust 1.90 toolchain. The transfer integration
suite passes eleven cases; the portable codec/replacement library filter passes eight
cases, including its subprocess recovery helper. Two real-authority process cases
pass with the freshly built server: confirmed Canvas state survives export,
detach and restart; unauthenticated/native-loopback/Member access and deleted
bindings reject; a complete chat export retains 66 primary messages, a subsidiary
root and three owned attachments beyond the UI window. Its payload exceeds the
state-message body budget and also qualifies through portable upload before
cancellation, without changing the live dataset. The history fixture
is extended durably while its authority is stopped, using synthetic clock values;
live messaging rate limits are unchanged. These are local software checks, not
browser/physical Frame acceptance or Linux shared-memory IPC evidence.

Portable replacement intake now uses `POST` on that same data path with the
payload media type and `X-OTriOS-Request: transfers`. Authentication, exact target
admission and one transfer credit precede request-body consumption. Upload bytes
and a 30-second absolute deadline are bounded. Codec qualification runs outside
the admission lock, then the authority rechecks identity, generation and catalog
before issuing an opaque, actor-bound review that expires after ten minutes.
The review carries the exact dataset, schema, audience, archive SHA-256 and one
fresh replacement CID. Staging changes no active binding. An expired stage releases
its credit; a current authenticated owner can cancel even after losing Admin.

Apply and Cancel use channel 5 `ReplaceData`/`CancelData` commands; replacement
is not an HTTP mutation. Commit rechecks current Admin, reviewed generation,
catalog, source health and service restrictions, consumes its stage once and
uses the OTSR2 durable decision barrier. Unknown replies are resolved through
`ReplacementStatus` against the fresh CID. The RAM observer preserves this read
across reconnect, never resends ReplaceData, and discards it on account change.
Active or preferred-detached replacement retains the outgoing CID as a sealed
backup. Replacing a sealed ghost preserves the active/preferred app binding and
both inactive datasets; the new inactive dataset has explicit Imported provenance.
OTSP9 fences writers that cannot preserve that extension; recovery accepts OTSP1–8
without changing existing identities. State protocol v20 and Spaces envelope v5
carry these controls; the v1 release-control bootstrap stays unchanged.

Local checks now also cover stage expiry, delayed generation/catalog changes,
reconnect correlation and inactive replacement. A real authority process check
uploads without activating, rejects another Admin's stage use and revoked/stale
reviews, cancels stages, commits with an unread reply, rejects a delayed old writer
and stage replay, and resolves the same committed CID after restart while retaining
source data and outgoing backup bytes. All 30 authority/client process cases pass
with two test threads. All 234 then-current core library cases, 29 Spaces cases
and 33 host UI cases pass; the subsequently added OTSP8-to-OTSP9 migration case
also passes without changing custom layout, sealed data, live bindings or payload
bytes. Targeted strict core Clippy and the browser session/UI-update Node checks
pass. These checks do not qualify deployed transfer UI or constitute the final
repository-wide release qualification.

The host controls and fixed codec fixtures above have local coverage. Final
integrated release qualification and real browser/Frame acceptance are still required.
The deployed release remains unchanged. Run the complete repository and scoped
integration suites on the final combined candidate before signed deployment and
browser/physical Frame migration and rollback acceptance.

## Export and replacement

Export pins a coherent confirmed app-owned snapshot plus its owned asset bytes
or immutable file handles. Serialization runs outside the authority lock where
possible; admission and source identity are checked when pinning and delivering.
Bound staged objects, concurrent work, retention and total storage. Private data
downloads remain authenticated and scoped; a filename or digest is not a grant.

Replace Data stages and validates the complete payload, then reviews the target
Space/app and audience. Recheck current Admin, generation, binding, schema and
codec after any upload delay. Keep the outgoing dataset as a sealed backup,
activate a fresh collection only after its durable decision, and invalidate old
mounts/writers. Recover an interrupted scoped operation before admission.
An invalid payload leaves existing data untouched; uncertainty triggers a
read-only outcome check, never automatic mutation replay. The authority mints
one fresh replacement CID for the review. A committed CID remains observable
in its current binding, sealed dataset history or permanent deletion ledger.
An unknown outcome means unresolved; it is never permission to retry a write.
Replacement uses the existing OTSR2 staged decision/admission barrier, leaves
the outgoing CID sealed as `BeforeReplacement`, and bumps the attachment epoch.
No generic transaction facility or old writer admission state is imported.

## Host authorization and publication storage

Credential configuration may contain an explicit `operators` list of at most
64 configured human/native actor IDs. Omission grants no operator rights;
unknown actors and duplicates reject configuration. These identities are
independent of Space memberships. Aaron and the existing Frame identity are
authorized for this deployment; resolve their actual actor IDs from private
configuration rather than guessing from display names or login addresses.

Published app objects occupy a separate private authority-owned directory,
not the immutable release tree. One process owns its stable lock. Publication
verifies the signed bundle against the host's independently pinned publisher,
keeps every signed file byte, then writes a deterministic ZIP with atomic
rename and directory sync. Repeated publication of the same exact object is
idempotent. At most 64 objects and 128 MiB total are retained. Stages are private
and unreachable; public routes read only explicitly published ID/digest pairs.
Withdrawal removes future delivery, leaving downloaded copies unaffected.
