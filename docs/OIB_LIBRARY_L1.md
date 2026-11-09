<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB library L1 implementation

Date: 2026-11-09. Status: **in development; not deployed or qualified**.
Branch: `feat/oib-library-l1`, based on canonical D2 commit `7b88756`.

This implements the data-only L1 selected in
[RFC 0036](rfcs/0036-upgradable-oib-component-libraries.md). Component Wasm ABI L2,
new renderer/input primitives, typography/images and XR remain separate work.

## Scope and responsibilities

The package role owns distinct library metadata, canonical integrity/signature
admission and rejection tests. The graph role owns exact import bindings,
resolution, compatibility checks and read-only imported definitions. The state
role owns expected-generation baseline adoption, causal-state preservation and
recovery. Integration owns authoring/publication tooling, authority activation,
protocol coordination and live qualification. Terra supplied the read-only
release/adoption inventory.

The signed app contains independently signed library trees under
`Dependencies/<revision>.otlib/`. This reuses the established complete signed
app/release closure and unchanged-host install workflow. An app manifest pins
exact revisions. Canonical local definitions and import bindings remain separate
from verified immutable library bodies transported to clients; the compiler
resolves them into disposable renderer output.

## Required checks

- Reject package kind/version confusion, invalid signatures/hashes, missing or
  mismatched exports, changed class contracts and aggregate budget violations.
- Preserve document-local definition IDs, node/property IDs, occurrence ancestry,
  semantic epoch, collection identity, business values and causal overrides.
- Reject edits to imported definitions while allowing local wrappers, instance
  overrides and admitted app connections.
- Adopt through a separate expected-generation/expected-lock operation. Validate
  and compile the candidate before one durable publication. Rejected/stale or
  failed persistence retains the prior complete baseline.
- Recover the exact accepted lock after authority restart; retain coherent pinned
  readers and bound slow-reader/retired-closure retention.
- Publish a signed library-only style/default revision and compatible rollback
  with unchanged host binary/module hashes on the existing Frame/Web system.
  Activation waits for confirmed edits and completed gestures/composition.

State protocol advances to v30 for the new baseline representation; the opening
release-control envelope remains v1. Existing D2 source equality remains strict
outside the explicit compatible-library adoption path.

## Results

Local implementation checks now include 19 signed library/app package cases,
three unsigned-library cases, ten import/resolver cases, seven adoption cases and
sixteen existing structural-batch cases. A real authority process passed the
focused upgrade/restart/rollback case with synthetic already-verified bodies;
this is separate from signature/publication acceptance. The actual Client held
slider and final receipt while a new baseline arrived, then activated it after
confirmation. Nineteen ordinary state tests, nine server tests and nineteen
shared-session/update Node tests passed. CLI source/pack/sign/bind/verify passed
using a production library, as did ordinary-build pin inheritance and explicit
newer-pin preservation. The final live result will be recorded after deployment.

The packer reuses the already locked Serde and serde_json packages for closed,
typed JSON authoring. Serde becomes an explicit optional dashboard dependency;
Cargo.lock adds only that dependency edge, with no new resolved package/version.

## Library authoring workflow

`canvas-bundle` provides `library-source`, `library-pack`, `library-inspect`,
`library-sign`, `library-verify` and `library-bind`. Library JSON is authoring
input; the package contains canonical bounded FlexBuffers, exact integrity and
an independent publisher signature.

To extract reusable local definitions from a verified app, write a source spec:

```json
{
  "library": "<publisher-public-key-sha256>:oib-2d",
  "version": "0.1.0",
  "exports": [{"id": 1, "name": "SettingsPanel", "definition": 2}]
}
```

The actual definition IDs come from the app source, not these illustrative
numbers. `library-source APP.otapp SPEC.json SOURCE.json --key PUBLIC --openssl
PATH` includes each export's complete reference closure. Edit the resulting
source's appearance/defaults, then pack and sign:

```sh
canvas-bundle library-pack SOURCE.json candidate.otlib
canvas-bundle library-sign candidate.otlib signed.otlib --key PRIVATE --openssl PATH
canvas-bundle library-verify signed.otlib --key PUBLIC --openssl PATH
```

`library-bind APP.otapp signed.otlib BINDINGS.json candidate.otapp --key PUBLIC
--openssl PATH` creates an unsigned app candidate with the exact dependency pin.
Bindings explicitly map library definition IDs into the app's stable definition
namespace. For a later revision, keep the same binding file; the tool rejects an
implicit rebind. Sign the app candidate with the ordinary `sign` command and
publish it through `scripts/install-app.py`. That workflow reuses the qualified
host assets and verifies their hashes before deployment.

Exact pins select reproducible bytes; they do not define an update policy.
A future compatible-update policy can select a newly published revision and
record the accepted exact pin through the same validation/adoption path. L1
requires an explicit app dependency update and has no floating `latest` resolver.


For repeated library publication, `scripts/publish-oib-library.py --runtime RUNTIME
--source libraries/oib-2d/source.json --bindings libraries/oib-2d/designer-bindings.json
--app oib-designer --checker CHECKER --openssl PATH` stages a complete signed
app/library release without compiling a host. Add the existing deployment config
with `--deploy-config CONFIG` to activate it. The script checks the reviewed
baseline, freezes the requested source/bindings, verifies both package signatures,
and reuses the ordinary installer's unchanged-host closure checks.
