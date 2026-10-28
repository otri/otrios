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
