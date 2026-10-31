# OTriOS development instructions

Read README.md, docs/POC.md, docs/ARCHITECTURE.md, and LICENSE.md before changing code.

- The first target is a Linux userspace PoC. Preserve the hardware and driver base supplied by Linux.
- Implement only the active milestone. Keep module execution, portable compilation, general transactions, and CRDT acceleration behind later RFCs.
- Runtime implementation and its tests belong in crates/otrios-core under LGPL-3.0-only. Public contracts, schemas, SDK code, docs, and build tooling use Apache-2.0. Do not copy runtime implementation into permissive packages.
- Rust 1.90.0 is pinned. Run `bash scripts/check.sh`. Keep Cargo.lock committed and justify new dependencies.
- Prefer safe Rust. Shared-memory FFI may require a narrowly scoped unsafe module or separate crate; document invariants and amend the lint locally with a reviewed RFC before introducing it.
- Hash canonical bytes with a standard SHA-256 implementation. Never substitute process-dependent hashing or treat a content hash as a signature or capability.
- Test snapshot isolation, publication conflicts, slow-reader backpressure, and process boundaries. Do not claim Linux IPC works based on in-process tests.
- Use a feature branch for follow-up work.
- Do not open a pull request until all scoped work and required validation are complete, final commits are ready, and the work is ready to submit to the user for final review. Do not open draft or progress PRs, or use an open PR to track ongoing implementation.
- For deployable changes, complete the authorized deployment and make it available for testing before opening the PR.
- Finish and validate review follow-ups before pushing them to an existing PR. Batch each completed review update into one push; do not push incremental work-in-progress updates that generate repeated PR notification emails.
- Report actual commands and results; distinguish scaffold checks from PoC acceptance tests.
- Do not assign work to Tess, install integrations, add collaborators, change visibility, publish packages, or deploy services without user authorization for that action.
- The user has given standing authorization to deploy completed, validated OTriOS changes to the existing frame/web environment as part of finishing each task, unless they explicitly defer deployment. Follow the established signed-release verification and rollback workflow.

## Release control and client state

- Keep release discovery event driven and independent of CRDT decoding/state recovery. Use one server and a stable opening hello on the shared client WebSocket (RFC 0009); protocol independence does not require separate processes or connections. Preserve the small release-control v1 hello/upgrade envelope as the bootstrap compatibility boundary, including when CRDT protocols change. On every reconnect send the current release hash; publish upgrade events only after the complete signed code replica is active.
- Breaking app/CRDT protocol changes are acceptable during this PoC. Do not let obsolete state-protocol handshakes prevent a client from discovering or loading the current release.
- For now, a disconnected client may discard unconfirmed RAM edits when upgrading. Do not require preservation, migration or durable client storage for those edits. While connected, continue to wait for confirmed edits and finished gestures. Revisit preservation of offline edits as an explicit future requirement before adding it.

- Bump the declared state protocol whenever its wire format changes. Browser and native/server codec identifiers must agree; preserve the v1 opening control envelope and reject state bytes before compatible negotiation.
