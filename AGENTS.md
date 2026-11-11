# OTriOS development instructions

Read README.md, docs/POC.md, docs/ARCHITECTURE.md, and LICENSE.md before changing code.

- The first target is a Linux userspace PoC. Preserve the hardware and driver base supplied by Linux.
- Implement only the active milestone. Keep module execution, portable compilation, general transactions, and CRDT acceleration behind later RFCs.
- Runtime implementation and its tests belong in crates/otrios-core under LGPL-3.0-only. Public contracts, schemas, SDK code, docs, and build tooling use Apache-2.0. Do not copy runtime implementation into permissive packages.
- Rust 1.90.0 is pinned. Keep Cargo.lock committed and justify new dependencies. Use `bash scripts/check.sh` for broad repository validation when warranted by the change or explicitly requested, rather than as a gate on every iteration.
- Prefer safe Rust. Shared-memory FFI may require a narrowly scoped unsafe module or separate crate; document invariants and amend the lint locally with a reviewed RFC before introducing it.
- Hash canonical bytes with a standard SHA-256 implementation. Never substitute process-dependent hashing or treat a content hash as a signature or capability.
- Test snapshot isolation, publication conflicts, slow-reader backpressure, and process boundaries. Do not claim Linux IPC works based on in-process tests.
- Use a feature branch for follow-up work.
- Do not open a pull request until all scoped work and required validation are complete, final commits are ready, and the work is ready to submit to the user for final review. Do not open draft or progress PRs, or use an open PR to track ongoing implementation.
- For deployable changes, complete the authorized deployment and focused live acceptance before opening the PR.
- Finish and validate review follow-ups before pushing them to an existing PR. Batch each completed review update into one push; do not push incremental work-in-progress updates that generate repeated PR notification emails.
- Report actual commands and results; distinguish scaffold checks from PoC acceptance tests.
- Do not assign work to Tess, install integrations, add collaborators, change visibility, publish packages, or deploy services without user authorization for that action.
- The user has given standing authorization to deploy completed, validated OTriOS changes to the existing frame/web environment as part of finishing each task, unless they explicitly defer deployment. Follow the established signed-release verification and rollback workflow.

## Shared UI and input

- Prefer scrolling panels to Previous/Next controls. Keep bounded paging only when rendering the full admitted list would exceed existing scene, control, layout-byte or coordinate limits; test and document that constraint.
- Decorative overlays should use explicit input pass-through when they must leave underlying controls interactive. Preserve modal input barriers and ancestor bubbling. Keep the visible portion of clipped controls clickable.

## Default development loop: live first

- Bias toward live coding: make a scoped change, compile it, deploy to the existing authorized Frame/Web environment, and test the changed behavior on the live system. Repeat this short loop as needed.
- Test essentials only. Default to one focused existing regression or one short live journey for the changed behavior, plus the build needed to run it. Do not routinely stack filtered tests, the full core suite, workspace/all-target checks, Clippy, formatting checks and browser/native repetitions for each slice. Format changed files directly as part of editing.
- Do not run full preflight suites, disposable browser qualification campaigns or broad re-validation by default. A shared resolver change or public type change alone is not a reason to run a broad suite. Broaden only for a specific observed failure, a concrete unresolved integration risk, a named acceptance requirement or an explicit user request; state what the additional check will resolve.
- Reuse existing tests. Add a regression only for a demonstrated bug or a meaningful new invariant; avoid tests that mirror implementation, repeat equivalent cases or cover reversible copy/layout polish. Preserve useful suites for deliberate qualification without making them routine development gates.
- Do not create routine pre-deployment data inventories, preservation audits or paired backups for ordinary PoC iteration. The user prioritizes fast live feedback over protecting the current PoC data. Use additional data safeguards when specifically requested or when testing migration, recovery or rollback is itself the task.
- Keep signed-release verification and a known compatible code rollback available. This faster loop does not authorize new services, trust roots, identities, privilege changes or unrelated destructive actions.
- Batch completed review follow-ups into one push to an existing PR. Report what was actually built, deployed and tested, including any checks intentionally deferred; do not present unrun checks as passing.

## Maintenance after acceptance

- Once the user has accepted the work, run `python3 scripts/maintain-dashboard-builder.py` to review disposable builder artifacts, then apply with `--apply` after builds, deployments and app imports have finished.
- Stop the existing automatic builder before applying, and restore the same builder configuration afterward even if cleanup fails. The script never manages services itself. See [maintenance](docs/MAINTENANCE.md#dashboard-builder-after-acceptance).
- Accepted-work cleanup retains only the active signed release; a rollback pin is not required after acceptance. During development, continue to keep a compatible rollback available.
- Close finished worktrees and delete merged task branches when authorized. Keep the original repository on `main`, preserve unfinished work, and remove temporary task captures/source copies once useful evidence is committed. Do not delete signing keys, configuration, app state or toolchains.

## Release control and client state

- Keep release discovery event driven and independent of CRDT decoding/state recovery. Use one server and a stable opening hello on the shared client WebSocket (RFC 0009); protocol independence does not require separate processes or connections. Preserve the small release-control v1 hello/upgrade envelope as the bootstrap compatibility boundary, including when CRDT protocols change. On every reconnect send the current release hash; publish upgrade events only after the complete signed code replica is active.
- Breaking app/CRDT protocol changes are acceptable during this PoC. Do not let obsolete state-protocol handshakes prevent a client from discovering or loading the current release.
- For now, a disconnected client may discard unconfirmed RAM edits when upgrading. Do not require preservation, migration or durable client storage for those edits. While connected, continue to wait for confirmed edits and finished gestures. Revisit preservation of offline edits as an explicit future requirement before adding it.

- Bump the declared state protocol whenever its wire format changes. Browser and native/server codec identifiers must agree; preserve the v1 opening control envelope and reject state bytes before compatible negotiation.
