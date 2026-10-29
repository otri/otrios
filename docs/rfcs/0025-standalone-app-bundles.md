<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0025: Standalone app bundles

Date: 2026-10-29. Status: proposed architecture; B1 offline packaging is
implemented and qualified locally. Production installation is not implemented.
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
and an offline first increment without presenting it as a generic installer.

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
Hash exact published bytes, never reconstructed metadata.
Bundle digest = `SHA256("otrios-app-bundle-v1\0" || files.flex bytes)`.

B1 is unsigned: report publisher authentication as absent and refuse any claim of
production installation. B2 specifies domain-separated Ed25519 signatures,
authenticated publisher/key IDs, trust roots, revocation and rollback policy.
Signature bytes sit outside their own hash closure; the exact signed envelope
needs conformance vectors before admission. B1 rejects signature extensions.
Existing whole-release signing stays authoritative.

## Limits and filesystem rules

At most 64 files including the manifest, 512 KiB per file, 64 KiB per metadata
document and 16 MiB total file bytes. Paths: at most 240 ASCII bytes and eight
components. Reject absolute paths, empty/dot/dot-dot components, backslashes,
colons, spaces, NULs, non-ASCII names, trailing periods, Windows reserved device
basenames, case aliases (including directories), file/directory collisions,
symlinks, special files and empty directories. Payloads are Info.flex or descend from Code, UI,
Schemas, Resources, Dependencies or Notices. State, Cache and Contents are invalid.

The CLI assumes caller-controlled, quiescent trees. Metadata checks plus ordinary
opens are not race-free against hostile concurrent mutation. Read limits still
hold if a file grows; depth and total directory entries are bounded. Export uses
a fresh sibling staging directory, re-inspects it, then renames to an absent
destination. This is not a crash-durable installation transaction. B2 requires
descriptor-relative no-follow reads, immutable staging, fsync ordering and a
durable activation pointer before handling hostile inputs.

## Installation and lifecycle (B2/B3 proposal)

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
| B2 | Signature/trust contract and safe immutable installation; interrupted staging/activation, publisher/key/revision conflicts and rollback tests; no execution before trust |
| B3 | Multi-screen/dependency closure, generic authority admission, Home discovery without host rebuild; public/private denial and revocation; real browser/Frame lifecycle and signed-release rollback qualification |

B1 exports one current package, potentially a screen in a larger app. External
navigation, fonts, declared dependencies and authority services are not silently
included or resolved. It checks identity/byte integrity, not complete launch
semantics or a general sandbox. General archive extraction, signing, installation,
SDK stability and target compilation are outside B1.

Legacy dependency strings mix source files, SDK hints and runtime requirements.
B1 retains them as descriptive data, not a complete typed dependency or permission
plan. In particular, exporting Monitoring with an empty capabilities list does
not remove its existing admission requirement: every B1 artifact is noninstallable.
B2/B3 must translate host requirements into explicit checked contracts before
generic installation, rather than treating the legacy strings as grants.

Runtime validation/tests stay LGPL in otrios-core; Apache public contracts stay
in otrios-interfaces and filesystem/build tooling in the thin CLI. No new
dependency or unsafe code is needed. B1 changes no live payload, protocol, catalog
or release path, so it needs no deployment.

## Commands

```sh
cargo run -p otrios-dashboard --features canvas --bin canvas-pack -- apps
cargo run -p otrios-dashboard --features canvas --bin canvas-bundle -- \
  export apps component-playground /tmp/component-playground.otapp --revision 1
cargo run -p otrios-dashboard --features canvas --bin canvas-bundle -- \
  inspect /tmp/component-playground.otapp
bash scripts/check.sh
```

These check authoring/inspection, not installation, Linux IPC, publisher trust,
native compilation or physical display acceptance. See the
[B1 qualification record](../experiments/app-bundles-2026-10-29.md) for actual results.
