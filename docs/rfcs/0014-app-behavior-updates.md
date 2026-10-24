<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0014: Independent Canvas behavior replacement

Status: implemented and qualified in browser and native frame process tests. Extends RFC 0013's shared package
update path for every app with an existing declared Wasm behavior module.

## Contract

A compatible behavior update changes its Wasm bytes and digest only. The guest
ABI version and declared writable fields, state schema, layout, selectors,
handlers, navigation and presentation contract remain identical. Adding a
behavior to an app, changing permissions or introducing an ABI requires an
initial qualified full release. The bounded Wasmi interpreter, no-import policy,
64 KiB module/memory limits and 10,000 fuel budget remain unchanged.

The signed release advertises `behavior_abi: otrios-app-behavior-v1`, which is
an update capability distinct from the guest's exported `abi_version() == 1`.
Existing hosts discover the initial runtime upgrade through stable control v1.
Subsequent compatible module updates keep their browser worker, WebSocket and
native renderer. Canvas sync v4 and its mount messages are unchanged.

The selected app stages a candidate with the existing mounted state/coverage
handshake. Hash, permissions, imports, limits, ABI and the entire candidate view
are qualified before replacement. The guest instance is replaced; its private
memory starts fresh. Explicit app document values, pending-intent policy,
editors, focus, keyboard, ink and presentation ownership are retained. No guest
input event is dispatched during activation. Duplicate admission does not reset
the guest or advance the mount epoch. Invalid candidates leave the current guest
and scene usable. This does not establish purity or migrate hidden guest memory.

Shared resource diagnostics report the active **instance's** loaded code hash,
not just the UI's declared reference, as `behavior_hash`. Private view clearing
also removes its active behavior diagnostic.

## Packaging and delivery

`publish-app.py` is the common publisher for UI and behavior updates, using one
qualified runtime receipt, publisher lock, signed closure verification,
expected-baseline check and partial transfer. No new rollback tooling is added.

```sh
python3 scripts/publish-app.py record --runtime RUNTIME \
  --baseline RUNTIME/build/app-current.json --record RUNTIME/build/app-runtime.json \
  --server ARM/dashboard-server --monitoring ARM/dashboard-monitoring \
  --bootstrap ARM/session-bootstrap

python3 scripts/publish-app.py publish --kind behavior --app component-playground \
  --runtime RUNTIME --baseline RUNTIME/build/app-current.json \
  --record RUNTIME/build/app-runtime.json --deploy-config RUNTIME/deploy-config.json
```

The qualified record binds host source/toolchain inputs and executable hashes.
App UI authoring and `apps/*/behavior/main.wat` are independent package inputs;
state schemas, event ABI declarations, host code and compiler tooling remain
runtime inputs. Integration-test sources are validation inputs rather than host
executable dependencies.

The behavior path invokes the already-built native `canvas-pack` authoring tool
in `--app-behavior APP BASELINE SOURCE` mode. It compiles only the selected WAT
module, validates it against the existing guest contract, and changes that
package's `behavior.wasm`, the digest in `ui.flex`, and the catalog hash. The UI
factory is not rebuilt or rerun for behavior-only edits. Browser Wasm and native
host artifacts are reused. UI layout bytes remain identical. Unchanged modules
produce the same release and skip transfer/activation.

The frame reconstructs unchanged files from its verified installed closure and
verifies the complete new signature/digest graph before activation. Modules stay
behind authorized app mounts; the partial transfer does not expose private code
through public static routes. Native signature verification remains mandatory;
the browser retains its existing HTTPS and manifest-hash trust model.

## Acceptance and scope

Tests cover state retention, fresh guest memory, duplicate no-ops, stale input,
ABI/permission changes, trapping candidates, active-instance hashes and partial
signed guest transfers. Real browser acceptance leaves Playground and Home open,
replaces only Playground's guest, checks retained values/pages/sessions/focus,
then verifies the changed reducer result and its replication to another client.

The first visible example makes Playground's linear slider snap to ten-percent
steps. This is ordinary app guest code, without a Playground-specific loader.
A native headless process can execute the same signed app bytes through the
frame's loopback authority; native renderer PID/readiness verifies host retention.

Home's animated banner needs a separate local presentation hook. General typed
compute, effect imports, schema migration, component replacement and portable
compiler selection remain separate increments.
