<!-- SPDX-License-Identifier: Apache-2.0 -->
# Repository maintenance

## Local artifact cleanup

Run from any directory with Python 3.11+ and Git on macOS or Linux:

```sh
python3 scripts/clean-artifacts.py                    # inventory only
python3 scripts/clean-artifacts.py --apply            # remove eligible caches
python3 scripts/clean-artifacts.py --only incremental --apply
python3 scripts/clean-artifacts.py --json             # machine-readable dry run
```

The script defaults to its own checkout. For another checkout or worktree, use
`--repo /ABS/otrios`. Repeat `--only` to select several groups.

| Group | Fixed path | Recreated by |
| --- | --- | --- |
| `incremental` | `target/debug/incremental` | Cargo debug builds |
| `python` | `scripts/__pycache__` | Python imports without a separate cache prefix |
| `legacy-web` | `web/dashboard/pkg` | Retired dashboard builds; current Wasm output is `web/canvas/pkg` |

Stop Cargo/Rust/Wasm builds and automatic dashboard builders before applying.
The process guard is system-wide, so a build in another checkout can block the
run. It refuses cleanup if process inspection fails; it does not stop processes.
The guard is a best-effort snapshot, and automated builders must remain stopped
throughout cleanup. Do not change mounts or artifact contents during the run.

All selected paths are inventoried before deletion. A tracked file, unignored
path, symlink, special file, intersecting mount or invalid checkout blocks the run.
The selected directory must be ignored as a whole; all its regular contents are
treated as disposable. Mount-table inspection includes Linux bind mounts and
must succeed. Directory access
and removal use file descriptors and symlink-resistant Python operations. A
directory replaced after inventory is rejected. The script rechecks builders
and mount boundaries before each deletion. An I/O error can stop a run after
earlier paths were removed. A deletion that fails partway through is marked
`incomplete`, with its deleted byte count unknown; completed-removal totals exclude
that path. The result lists each path's status and exits nonzero when blocked.

Dry runs create no report or lock files. Missing caches are reported as `absent`,
so repeated runs are harmless. `--json` includes file counts, logical/allocated
bytes, deletion status and the filesystem's free-space change. Allocated bytes
are an inventory estimate; filesystem accounting, snapshots and shared blocks
can make the immediate free-space change differ.

The fixed list preserves `target/debug/deps`, release builds, qualification
directories, current Wasm/app packages, benchmark results, state and rollback
material. `CARGO_TARGET_DIR` does not expand the deletion scope. There is no
arbitrary path or force option. Release-store pruning uses the separate,
pointer-aware [frame hosting workflow](FRAME_HOSTING.md#build-and-publication).

Run the deletion-boundary tests with `python3 scripts/test-clean-artifacts.py`;
they also run in `bash scripts/check.sh`.

## Dashboard builder after acceptance

Once work is accepted, run the small builder cleanup command with Python 3.11+:

```sh
python3 scripts/maintain-dashboard-builder.py          # dry run
python3 scripts/maintain-dashboard-builder.py --apply  # builders must be stopped
```

The default folder is `~/Library/Application Support/OTriOSDashboard`.
Use `--runtime /ABS/PATH` for another builder and `--json` for a machine-readable
report. Review the dry run before applying. Stop the existing automatic watcher
and other builds/deployments first; restore the same watcher configuration even
if cleanup fails. The script takes the existing watcher/feed locks, checks for
build processes and refuses an unresolved local app-import receipt. It does not
stop services, schedule jobs, build or deploy releases.

The command removes a fixed list: Cargo incremental/dependency/build/fingerprint/
example caches, release/Wasm target caches, `build/previous-source`, generated
`build/source-before-*` copies, and `python-cache`. It reuses the existing release
pruner to remove inactive signed releases. After acceptance only the active
release needs to remain; no rollback pin, age policy or cache-size threshold is
involved. During implementation, keep the compatible rollback until acceptance.

Top-level checker/packer/server executables in `build/mac-target/debug`, the
current build source and source inbox, downloaded dependencies, toolchains,
signing keys, configuration, state, logs and historical backups are preserved.
The next build recompiles the cleared caches. Task-specific recordings,
worktrees and historical backup retirement remain explicit closeout work rather
than guessed deletion rules.

The command rejects symlink/mount boundaries and changed cache directories before
removal. A filesystem error may leave an incomplete removal; the report identifies
it and stops further cleanup. Allocated byte estimates are not a promise of the
same immediate free-space gain on filesystems with shared blocks.

Run its focused checks with:

```sh
python3 scripts/test-maintain-dashboard-builder.py
```

## Experiment retention

The [experiment index](experiments/README.md) separates current qualification from
historical conclusions. Older raw measurements and screenshots were retired from
the current tree at the user's request; exact pinned Git links preserve their
provenance. Early dashboard and app cutover reports are consolidated into one
summary. Current qualification records and RFC identities remain available.

Further experiment retirement should preserve measured outcomes, conditions and
limits in a concise summary, update incoming links, and identify the exact Git
revision containing the original evidence. Cache cleanup and documentation
retirement are separate operations.
