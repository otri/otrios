<!-- SPDX-License-Identifier: Apache-2.0 -->
# Repository cleanup — 2026-10-28

This completes the bounded cleanup from [issue 31](https://github.com/otri/otrios/issues/31).
The [machine-readable results](data/repository-cleanup-2026-10-28.json) record
validation, release identities, rollback retention and the local deletion inventory.

## Documentation and tooling

Current guides now distinguish completed dashboard work from the Linux IPC
backlog, use `/session`, describe the app inventory and collection limits, and
separate UI schema versions from state sync v8 and release-control v1. The native
agent's 2 GiB default is distinguished from the checked-in frame service's
128 MiB override. The font resource path is corrected; the older public release
hash is explicitly historical.

The README's milestone narrative moved to [the history](../HISTORY.md), preserving
its evidence links. The [RFC index](../rfcs/README.md) disambiguates the duplicate
0005/0006 identifiers by title, preserves historical filenames and requires
unique numbers for new RFCs.

The current benchmark runner rejects `--legacy-reference` before creating output
or invoking external tools. Its remote child also clears the obsolete inherited
encoding override. The historical report identifies the compatible runner and
client at `ed3c86b`; its archived results are unchanged. Server/browser admission
reuse their existing protocol constants, and publisher output is checked against
the same contract. Catalog tests exercise the production release loader and
reject incompatible manifests before fetching catalog bytes. Server usage text
lists the four previously omitted options. No dependency or wire format changed.

## Validation

Commands used the existing Rust 1.90.0, Node and OpenSSL environment.

| Command or check | Result |
| --- | --- |
| `bash scripts/check.sh` | Passed: 308 Rust tests and 46 Python tests, formatting and strict workspace Clippy |
| Node session/worker/cache/catalog/document/input/viewport/UI-update suite | Passed: 62 tests |
| Signed builder's Node suite | Passed: 74 tests; overlaps the separate Node suite |
| `cargo test -p otrios-dashboard --features direct --bin dashboard-direct --locked --offline` | Passed: 11 native worker/input/network tests |
| `cargo clippy --workspace --all-targets --all-features --target armv7-unknown-linux-gnueabihf --locked --offline -- -D warnings` in the existing Linux builder | Passed, including Linux-only examples |
| `python3 scripts/test-canvas-pipeline-cli.py` | Passed: early rejection with no output/tool invocation, and CLI help |
| Server usage invocation | All four previously omitted options present; existing usage-error exit behavior retained |
| Relative Markdown target scan and `git diff --check` | Passed |
| `CARGO_INCREMENTAL=0 cargo check -p otrios-dashboard --features server,canvas,host-sync --bins --locked --offline` after cache removal | Passed |

The macOS all-feature/all-target Clippy attempt failed on existing Linux-only
display examples whose types are unavailable on macOS. The Linux ARM command
above passed. An additional temporary frame harness initially expected the raw
slider input of 730; the authored control confirms the stepped value 700. The
assertion was corrected, and the final frame process qualification passed without
a runtime change.

`watch-dashboard.py --once --stage-only` froze the source and built/signed the
browser and ARM artifacts in an isolated staging runtime. The complete signature
and asset graph verified. A disposable Mac authority serving that exact release
passed 123 public-only host checks and the real Chrome acceptance suite, including
durable messages, reload, viewport reflow and WebGL loss/recovery. The Home capture
was also visually inspected. All message-writing browser tests used disposable state.

On the frame, a disposable loopback-only ARM authority verified that signed
release and served two concurrent `canvas-native` client processes. Both confirmed
the stepped slider value 700. The authority restarted against its disposable
durable state, and another client confirmed successfully. Production state and
the display service were not used for this qualification.

These are Canvas application/process and release checks. They do not complete
the separate Linux shared-memory PoC acceptance gates. No new physical touch,
color or FPS qualification was performed.

## Deployment and rollback

Deployed source commit: `fcd180d9b8150e4e98ee803f2d5db285bdd91cd4`.
It was pushed and fetched from the configured remote before stopping or activating
production. The stopped watcher was pinned to that commit, its earlier config
was saved, and its exported source matched the signed candidate's input fingerprint.
The later documentation/evidence edits do not change the build input fingerprint.

Previous release:
`810de336ecad70e3c8692f6bc907ba27ae8693bd6c8aa4a56ca08bf1c542970b`.
Activated release:
`292e09a536befd5be2ccc9d53c8e8f1e64312d89e59cfe9dd6343334a69a3f54`.

Before deployment, the previous complete signed release, ten installed files and
32 stopped-state files were retained at
`/var/lib/otrios-authority/rollback/2026-10-28-repository-cleanup`.
The authority restarted after the snapshot. The backup's signature and complete
asset graph verified; its `rollback-code.py` was syntax-checked. The rollback
script restores code/services and retains current canonical app state. It was
not executed during this cleanup.

The established `deploy-dashboard-frame.py` command transferred 25,927,680 bytes,
verified the complete signed replica and activated it. The authority and native
cache retained the new active release; the operational rollback copy remains.
The authority, native display, tunnel and Monitoring timer are active. Native
Brass Relay reached connected, confirmed state with zero pending commands on the
same new release. Rotation remains 90 degrees, and the accepted calibration hash
is unchanged:
`44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

```sh
python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only
node scripts/probe-chat-release-browser.mjs --url https://otrios.otri.ca \
  --release 292e09a536befd5be2ccc9d53c8e8f1e64312d89e59cfe9dd6343334a69a3f54 \
  --chromium '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' \
  --output target/cleanup-2026-10-28/live-browser
```

The deployed public checker passed 120 checks. The real Chrome probe verified
control v1/state v8, recovered 25 selected message records and sent zero message
commands. Public-only checks exclude authenticated Monitoring login; private
admission/revocation remains covered separately by the process tests.

## Local artifacts and retained boundaries

After validation, with no active Cargo process, only these ignored generated
directories were removed:

| Path | Allocated bytes removed |
| --- | ---: |
| `target/debug/incremental` | 6,290,911,232 |
| `scripts/__pycache__` | 192,512 |
| `web/dashboard/pkg` | 3,338,240 |
| Total | 6,294,441,984 (5.86 GiB) |

The immediate filesystem free-space counter showed zero change, so this is an
allocation inventory of removed files, not a measurement of physical space reclaimed.
The post-cleanup Cargo check passed without restoring incremental cache files.
The file/path, size and modification-time inventories of 24 retained qualification
directories (5,203 files) matched before and after deletion.

Cargo dependencies/executables, current Wasm/app packages, benchmark results,
qualification/deployment outputs, durable state, signing/auth material and rollback
copies remain. Raw task logs and captures are retained locally under
`target/cleanup-2026-10-28`; the tracked JSON above summarizes the results.

Historical RFCs and experiment evidence, stable release control and retired-route
tombstones, OS research scaffolding, the small `canvas-server` and `loadAppCatalog`
wrappers, hardware regression probes and old state ignore rules retain documented
purposes. Their removal was deferred by the approved plan.
