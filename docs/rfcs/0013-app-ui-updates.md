<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0013: Independent app UI documents

Status: implementation and qualification. Extends the Canvas mount contract and
RFC 0012 composition. This increment applies to all seven apps.

## One package architecture

Every app owns its manifest, UI authoring, explicit state schema and optional
behavior. Each app exposes the same `manifest()` and `ui(code)` factory API.
`canvas-pack` uses one registry and one package writer. A conventional optional
`behavior/main.wat` produces that app's `behavior.wasm`; there is no
Playground-specific behavior compiler path. Stateless Home and Monitoring also
own explicit schema files. Host presentation implementations remain shared.

The artifacts retain their existing names: `app.flex` describes the app,
`ui.flex` is its UI document and optional `behavior.wasm` is its guest code.
A UI document includes layout, styling, typed selectors, action bindings and
references to behavior. State remains behind its stable authority-owned reference.

Idempotence concerns packaging and activation: identical input produces identical
canonical bytes, unchanged generated files retain their timestamps, an unchanged
publish skips deployment, and duplicate catalog admission does not remount.
It does not change the semantics of toggles, navigation or guest actions.

## A compatible data update

The signed release manifest declares `ui_abi: otrios-app-ui-v1`. Public runtime,
native executable, behavior bytes, app manifests and all other metadata must
match the running release. Only UI documents and their catalog hashes may differ.
A change outside that boundary uses the existing complete code upgrade.

The shared Rust client validates the catalog and stages a new mount only if its
active UI hash changed. Inactive app changes update future admission without
replacing the current mount. The candidate must retain the same UI identity,
state schema, behavior reference, handlers, navigation and presentation contract.
New selectors acquire their required state coverage through the existing
Mounted/AcceptMount/CancelMount handshake. The state wire protocol remains v4;
there is no new wire message or release-control envelope.

While connected, qualification waits for idle input and confirmed edits. The
candidate is resolved and validated before activation. Its new lease generation
rejects stale input; the compatible state, guest instance, local editors,
keyboard, ink and presentation ownership survive. Invalid candidates do not
replace the usable mount. Duplicate catalog bytes are a no-op even while a
previous activation awaits acknowledgment.

Browser frames use the existing logical viewport. Compatible semantic DOM focus
is restored when node ID and label still match; no stable cross-build component
identity is claimed. The page, worker and shared WebSocket remain loaded.
The browser retains its verified manifest in RAM so it can compare future
updates after the frame prunes old release folders. Reconnect hello reports the
latest admitted release. Browser history advances the address without reloading,
so a later explicit refresh opens that admitted release.

The native supervisor verifies the signed release closure, then signals the
same renderer through a private Unix datagram socket. Only the verified local
catalog path, release ID and expected hash cross this handoff. The app owner
performs the same staged admission and reports its admitted UI release after
presentation. Unchanged native code stays in the same process. Installed older
supervisors are restarted once with the initial runtime deployment.

## Publishing

A qualified full build is recorded once with `scripts/publish-app-ui.py record`.
The record binds its signed release, authority executable hashes and source
inputs, including the pinned toolchain/lockfile. UI authoring files
`apps/*/ui.rs` and `apps/*/components.rs` are the permitted data inputs. Changes to
runtime, schemas, handlers, authoring/compiler tooling or other build inputs
require a new qualified full build. The record is build provenance, not an
alternative trust root or permission grant.

`publish-app-ui.py publish --app APP` copies the baseline package closure,
runs `canvas-pack OUTPUT --app-ui APP BASELINE`, qualifies the candidate and
changes only the selected UI document and catalog. It reuses all runtime and
behavior artifacts. The native authoring tool may rebuild to incorporate edited
Rust UI source; browser Wasm and native host builds are skipped.

Only the changed UI/catalog bytes and signed metadata cross the frame deployment
connection. The installer reconstructs unchanged files from the verified installed
baseline, checks that baseline again under the publication lock, and verifies the
complete new closure before activation. All bytes remain covered by the signed
manifest. A publication
identical to the baseline skips network activation. The command shares the
watcher's publisher lock and detects source changes during packing.

```sh
python3 scripts/publish-app-ui.py record --runtime RUNTIME \
  --baseline RUNTIME/build/app-current.json --record RUNTIME/build/ui-runtime.json \
  --server ARM/dashboard-server --monitoring ARM/dashboard-monitoring \
  --bootstrap ARM/session-bootstrap

python3 scripts/publish-app-ui.py publish --app home --runtime RUNTIME \
  --baseline RUNTIME/build/app-current.json --record RUNTIME/build/ui-runtime.json \
  --deploy-config RUNTIME/deploy-config.json
```

Initialize `app-current.json` with the verified full-build pointer. Successful
remote deployment advances that local baseline. Omit `--deploy-config` for local
staging. Source the configured Rust/tooling environment before these commands.

## Boundaries and acceptance

Behavior-module replacement, broader typed compute, per-component replacement,
general capabilities and schema migration remain separate increments. No new
rollback tooling or external backup infrastructure is required for this hobby
frame deployment.

Acceptance covers every app's UI replacement and duplicate no-op, state retention,
private policy, invalid candidate rejection, stale input, content/provenance checks
and two real browser clients. The browser experiment keeps Home and Playground
open, changes Home's introduction, checks unchanged page/worker/session ownership,
retained focus and Playground values, then navigates to the updated Home.
Native acceptance checks the same renderer PID and its new admitted UI release.
