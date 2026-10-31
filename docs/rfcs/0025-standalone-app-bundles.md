<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0025: Standalone app bundles

Date: 2026-10-29. Status: B1 packaging and bounded B2/B3 signed Canvas app
installation are implemented. Independent app discovery, native/browser execution,
app-only upgrade and state-preserving rollback are qualified on live Frame.
General dependency resolution, capability imports and multi-publisher policy remain
proposed. See the [Frame qualification](../experiments/app-bundle-frame-2026-10-29.md).
Builds on [RFC 0004](0004-canvas-behavior-and-app-packages.md),
[UI replacement](0013-app-ui-updates.md), [behavior replacement](0014-app-behavior-updates.md)
and [release control](0009-unified-server-session.md).
[RFC 0026](0026-target-wasm-compilation.md) scopes target compilation separately.

## Problem and outcome

An app should be an identifiable, inspectable unit that can be transferred,
verified and installed without rebuilding the host. Today the packer imports
an explicit Rust source registry, the signed release carries all admitted app
packages, and some authority operations are app-specific Rust. Moving files into
a directory does not remove those dependencies. This RFC defines the destination
and implements installation for the bounded existing Canvas contract.

## Bundle layout

```text
Brass Relay.otapp/
  Info.flex
  Code/behavior.wasm           # optional portable executable
  UI/main.flex
  Schemas/state.flex
  Resources/                  # images, fonts, locales, etc., when present
  Dependencies/               # pinned private content, when present
  Integrity/files.flex
  Notices/                    # required distribution notices, when applicable
```

Use a flat root, without Contents/. Apple's extra level distinguishes its modern
macOS layout from historical bundles; iOS uses a flat root. We have no legacy
layout to distinguish. [Apple bundle guide](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html).

The tree defines the logical package. A later archive/content transport must
reproduce it exactly; timestamps, archive order and compression do not change
bundle identity. Omit empty directories. Keep mutable app data, credentials,
grants, host binaries and compiled caches outside the bundle. Private UI/code
stays behind admission checks; installation must never expose it as static assets.
Folder names and Home navigation paths do not define app identity. A future
resource API resolves logical locale/scale alternatives without install paths.

### Designated application roots

The proposed installed-bundle namespace is `/Applications`. The earlier proposal
for a separate `/PrivateApplications` root is replaced by
[RFC 0027: Spaces, membership and app data](0027-spaces-and-app-data.md).
App attachments, Space membership and audience policy determine access; bundle
placement does not. Monitoring is intended for a closed Operations Space.

The deployed signed release still stores public and private bundles under
`authority/bundles`; it has not created `/Applications`, Spaces or a filesystem
watcher. The signed `Info.flex.private` value and admitted catalog retain their
current bounded delivery classification until an explicit Space-aware cutover.
A future folder-based installer must reject duplicate app identities and publish
complete verified registry changes atomically. Moving or renaming a folder must
never grant access. Protected UI, code and state stay behind admission.

Home omits the generic "Installed app" subtitle. Private app cards carry a closed
padlock and "Sign-in required" when access is absent, or an open padlock and
"Access available" when access is present. The browser uses its validated cookie
session; the installed Frame host declares its trusted authority session. Access
indicators are local to the viewing session and never persisted in shared app
state. Observed cookie expiration/revocation relocks the browser card; a disconnected Frame
relocks its indicator. UI format `otrios-canvas-v7` introduces the host-local
`SessionAccess` selector, independent of state protocol v8 and release-control v1.
Changing this indicator does not authorize a mount or create sign-out controls.

## FlexBuffers and evolution

Info.flex starts with eight bytes: ASCII OTAPP, NUL, then little-endian u16 major
version 1 (`4f 54 41 50 50 00 01 00`). The rest is a FlexBuffers map. This fixed
prefix permits version rejection before metadata decoding, independent of state
protocol v8 and opening release-control v1.

FlexBuffers supplies self-describing binary values, not compatibility policy.
Apply bounded structural validation before library access, reject duplicate or
unsorted map keys, and use typed deserialization rather than coercing accessors.
[FlexBuffers reference](https://flatbuffers.dev/flexbuffers/).

All B1 fields are required, except behavior can be null or omitted by a reader:

| Field | Meaning |
| --- | --- |
| app | Existing Canvas App contract: id, title, ui_ref, state_ref, status, dependencies |
| revision | Nonzero u64 ordinal; not rollback authorization |
| private | Catalog delivery classification; not a grant |
| required_features | Contracts the loader must understand |
| capabilities | Requested capabilities; B1 accepts only an empty list |
| entry_ui | UI/main.flex in B1 |
| state_schema | Schemas/state.flex in B1 |
| behavior | Code/behavior.wasm, or null |

The nested existing App contract is named by required feature
`otrios-canvas-package-v1`. UI features name the actual `otrios-canvas-v2` through
`v6`; reducers also require `otrios-canvas-behavior-v1`. Unknown required features,
capabilities or bundle majors fail closed. App requirements never create grants.

Unknown optional top-level Info keys may be ignored, while their original bytes
remain in the hash closure. New execution/security requirements MUST be named in
required_features/capabilities, never solely in optional keys. Preserve existing
field types/meanings; new optional fields need documented defaults. The integrity
manifest has a closed schema. Bundle, app contract, guest ABI, UI format, state
schema and transport have independent versions. Schema migration/new state refs,
publisher ownership and rollback policy remain explicit later gates.

## Integrity and deterministic authoring

Integrity/files.flex contains `format` = `otrios-bundle-files-v1` and `files`.
Each entry has exactly `path` (string), `bytes` (u64) and `sha256` (64 lowercase
hex digits). Entries are strictly ASCII-path sorted and cover every regular file
except files.flex itself. Unlisted, missing and mismatched files fail inspection.

B1's publisher profile uses the locked FlexBuffers 25.12.19 Rust serializer with
default options: manifest fields in declaration order `format, files`, file fields
`path, bytes, sha256`, sorted file entries, unsigned sizes, and no omitted fields.
Inspection re-encodes the closed manifest and requires exact equality. A fixed
fixture digest gates encoder changes. This is not a claim that all equivalent
FlexBuffers maps have identical bytes. Independent publishers require
cross-language conformance vectors before the format is declared stable.

Info uses that same locked authoring encoder, but inspection accepts additive
optional metadata without normalization. UI/schema keep their existing codecs.
Revision editing refuses extended/noncanonical Info metadata it cannot preserve,
rather than silently discarding unknown fields. Installation copies original
metadata bytes unchanged.
Hash exact published bytes, never reconstructed metadata.
Bundle digest = `SHA256("otrios-app-bundle-v1\0" || files.flex bytes)`.

B1's original artifacts are unsigned. Current inspection still reports publisher
authentication as absent; only verification against an independently supplied key
produces an authenticated byte snapshot. B2's receipt is specified below.
Existing whole-release signing remains the installation authority.

### B2 signature envelope

`Integrity/publisher.flex` is a closed FlexBuffers map with `format` equal to
`otrios-app-signature-v1`, `key_sha256` (SHA-256 of the raw 32-byte Ed25519 public
key as lowercase hex) and `signature` (a vector of exactly 64 byte values).
It is excluded from files.flex and from the bundle digest, and counted in file
and byte budgets. All payloads and files.flex retain their B1 identity.

Sign the concatenation of ASCII `otrios-app-bundle-signature-v1` plus NUL, the raw
32-byte public key, and the binary 32-byte bundle digest. The verifier rejects a
key ID different from its externally configured key and verifies Ed25519 before
returning an owned immutable snapshot. No caller-supplied trust flag is accepted.
The unit fixture pins a public key, payload hash and signature independently
generated using OpenSSL; the process test signs with OpenSSL and verifies with ring.

The first trust scope is the existing single publisher. Frame uses its existing
`/etc/otrios-authority/publisher.pem`, configurable through `--bundle-key` for
disposable tests. A package cannot add a trust root. The private key stays on the
builder. Removing/changing trusted keys is a trusted administrative operation;
multi-publisher grants, automated key rotation and revocation distribution remain
future work. Neither package requirements nor signatures grant service access.

### B2/B3 installation transaction

Use the existing signed immutable release snapshot as the registry transaction.
The release contains `bundle_abi: otrios-app-bundle-v1`, public apps/catalog.flex,
and private `authority/bundles/<app-id>.otapp/` trees. The outer signature covers
the complete trees including their detached receipts. Before advancing the pointer,
the activation tool verifies every outer digest and every inner signature against
the pinned publisher, then fsyncs files/directories before the atomic pointer write.
The authority independently validates those trees/signatures before guest loading.
No bundle path is added to public HTTP asset allowlists.

`install-app.py` verifies a candidate, preserves all other bundles, composes the
catalog/Home and reuses the exact qualified browser/native assets. It stages a
complete newly signed release, and the existing deployment lock checks the expected
baseline during transfer/activation. Failures leave the active pointer unchanged.
Existing app updates must retain ID, UI/State references, private classification
and state schema. Changed content requires a strictly higher revision; equal
revision/equal content is idempotent. Explicit operational rollback selects a
previous complete verified release and never restores old authority state.

The current Canvas contract needs no source registry entry to admit a new schema,
primitive UI and pure bounded Wasm reducer. The launcher is generated from the
installed catalog's root entries; child routes stay under their existing parents.
Composition checks every navigation destination and qualifies every signed package
through the real Canvas runtime before publication. Unknown effects/capabilities
and arbitrary module linking remain outside this contract. Existing app-specific
Brass Relay and Monitoring services remain explicit host requirements.

Registry changes use a safe full browser/native host handoff after confirmed edits
and idle gestures; host bytes are reused without rebuilding. They are not sent
through the legacy fixed-catalog UI-only fast path. A removed native screen returns
to Home on handoff; its mutable state is retained. An old browser URL for a removed
app returns the normal 404. Already pinned versions may finish before upgrading.

Full host builds preserve independently installed bundles. Scoped UI/behavior
authoring remains available through publish-app.py: it materializes a temporary
verified build-input bridge, rebuilds/signs only the changed package, then uses
the same registry installer. Legacy package directories are not production input
when bundle_abi is present. Home is owned by registry composition.

## Limits and filesystem rules

At most 64 files including the manifest, 512 KiB per file, 64 KiB per metadata
document and 16 MiB total file bytes. Paths: at most 240 ASCII bytes and eight
components. Reject absolute paths, empty/dot/dot-dot components, backslashes,
colons, spaces, NULs, non-ASCII names, trailing periods, Windows reserved device
basenames, case aliases (including directories), file/directory collisions,
symlinks, special files and empty directories. Payloads are Info.flex or descend from Code, UI,
Schemas, Resources, Dependencies or Notices. State, Cache and Contents are invalid.

The CLI assumes caller-controlled, quiescent trees. Bounded no-follow/nonblocking
leaf opens and metadata checks do not provide a descriptor-relative filesystem
against hostile same-UID directory mutation. Read limits still hold if a file grows;
depth and total directory entries are bounded. Export uses
a fresh sibling staging directory, re-inspects it, then renames to an absent
destination. Export alone is not a crash-durable installation transaction. The
installer verifies owned byte snapshots, stages in the trusted private release
store, then uses the fsync and atomic activation sequence above. General hostile
archive extraction and adversarial same-UID filesystem writers remain out of scope.

## Installation and lifecycle

1. Receive bounded staging; verify the entire closure and trusted publisher
   signature before executable loading or compilation.
2. Resolve pinned dependencies and host interfaces, producing an explicit
   dependency/permission plan. Missing services remain unmet requirements.
3. Host policy grants access. Store immutable content by digest, then atomically
   register entry descriptors. Home discovers installed descriptors without a
   source registry edit or host rebuild.
4. Mount through Canvas and the authority. Mutable state stays at its stable
   authority reference, outside the bundle. Local/offline authority mode requires
   a separate design, not implicit durable client state.
5. Stage upgrades while the active bundle stays pinned. Connected clients wait
   for confirmed edits and completed gestures. Disconnected upgrading clients may
   discard unconfirmed RAM edits under current policy. Failed candidates retain
   the old mount; keep a verified last-good bundle.
6. Uninstall removes registration and unpinned content/cache references. Deleting
   authority data is separate and explicit. Rollback must never overwrite newly
   confirmed state with an old backup.

Generic authority admission must interpret packaged schemas/operations. Brass
Relay commands and Monitoring services remain app-specific host dependencies
today. Bundling their UI must not install privileged server code. Effects and
general capability imports require later review.

Keep one session and stable release-control v1 independent of state/install
failures. Publish upgrades only after complete signed content is active. State
wire changes require matching codec identifiers and a protocol bump.

## Milestones and acceptance

| Milestone | Required evidence |
| --- | --- |
| B1 (complete locally) | Offline export/inspection of one catalog screen/package; pinned deterministic digest; malformed header/type/feature/path/budget rejection; complete hash closure; catalog/UI/schema/behavior binding; private classification; real CLI export/inspect with tamper/link/no-overwrite checks; required repository checks |
| B2 (bounded implementation qualified) | Pinned publisher signature and immutable release installation; interrupted staging/activation, key/revision conflicts and rollback tests; no execution before trust |
| B3 (bounded Canvas admission qualified on Frame) | Generic schema/UI/pure-Wasm app admission, generated Home discovery, unchanged host bytes, private denial, real browser/native execution, app upgrade and signed-release rollback with confirmed state preserved |

General dependency resolution, multi-publisher revocation distribution, capability
imports and independently linked modules require later milestones. Existing
multi-screen catalog routes are preserved; the current independent installer adds
one Canvas package at a root route rather than resolving a new app's screen graph.

B1 exports one current package, potentially a screen in a larger app. External
navigation, fonts, declared dependencies and authority services are not silently
included or resolved. It checks identity/byte integrity, not complete launch
semantics or a general sandbox. General archive extraction, signing, installation,
SDK stability and target compilation are outside B1.

Legacy dependency strings mix source files, SDK hints and runtime requirements.
B1 retains them as descriptive data, not a complete typed dependency or permission
plan. In particular, exporting Monitoring with an empty capabilities list does
not remove its existing admission requirement. Unsigned exports alone remain
noninstallable. The bounded installer checks Canvas features and rejects capability
imports; legacy dependency hints never become grants. Rich host dependency plans
remain future work.

Runtime validation/tests stay LGPL in otrios-core; Apache public contracts stay
in otrios-interfaces and filesystem/build tooling in the thin CLI. B1 needed no
new dependencies or deployment. B2 adds direct uses of already locked ring 0.17.14
(Apache-2.0 AND ISC) for standard Ed25519 verification, base64 0.22.1
(MIT OR Apache-2.0) for bounded public-key PEM decoding, and libc 0.2.191
(MIT OR Apache-2.0) for safe no-follow/nonblocking open flags. No new crate version,
hand-written cryptography, unsafe block or workspace lint exception is introduced.
Native signature support is feature-gated and does not add ring to browser Wasm.
The repository exporter includes its Apache license under Notices; registry
composition and scoped revision tooling preserve existing resource/dependency/
notice payloads. Other publishers remain responsible for their own notices.

The installer operates on authenticated owned byte snapshots and private staging;
the release store is writable only by trusted management/service code, not guests.
No-follow/nonblocking file opens reject swapped links/devices without blocking on
a FIFO. Bounded traversal plus signature verification is not a general capability
filesystem for hostile same-UID concurrent writers; that threat remains out of
scope. Deployment and failure tests must qualify the actual owned-store workflow.

## Commands

```sh
cargo run -p otrios-dashboard --features canvas --bin canvas-pack -- apps
cargo run -p otrios-dashboard --features canvas --bin canvas-bundle -- \
  export apps component-playground /tmp/component-playground.otapp --revision 1
cargo run -p otrios-dashboard --features canvas --bin canvas-bundle -- \
  inspect /tmp/component-playground.otapp
bash scripts/check.sh
```

The export/inspect commands check unsigned authoring and byte integrity. See
[installation commands](../FRAME_HOSTING.md), the historical
[B1 record](../experiments/app-bundles-2026-10-29.md), and the
[signed Frame qualification](../experiments/app-bundle-frame-2026-10-29.md).
Neither milestone claims Linux shared-memory IPC or native AOT compilation.
