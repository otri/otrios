# OTriOS development instructions

Read README.md, docs/POC.md, docs/ARCHITECTURE.md, and LICENSE.md before changing code.

- The first target is a Linux userspace PoC. Preserve the hardware and driver base supplied by Linux.
- Implement only the active milestone. Keep module execution, portable compilation, general transactions, and CRDT acceleration behind later RFCs.
- Runtime implementation and its tests belong in crates/otrios-core under LGPL-3.0-only. Public contracts, schemas, SDK code, docs, and build tooling use Apache-2.0. Do not copy runtime implementation into permissive packages.
- Rust 1.90.0 is pinned. Run `bash scripts/check.sh`. Keep Cargo.lock committed and justify new dependencies.
- Prefer safe Rust. Shared-memory FFI may require a narrowly scoped unsafe module or separate crate; document invariants and amend the lint locally with a reviewed RFC before introducing it.
- Hash canonical bytes with a standard SHA-256 implementation. Never substitute process-dependent hashing or treat a content hash as a signature or capability.
- Test snapshot isolation, publication conflicts, slow-reader backpressure, and process boundaries. Do not claim Linux IPC works based on in-process tests.
- Use a feature branch and a pull request for follow-up work. Report actual commands and results; distinguish scaffold checks from PoC acceptance tests.
- Do not assign work to Tess, install integrations, add collaborators, change visibility, publish packages, or deploy services without user authorization for that action.
- Full Xcode is installed at `/Applications/Xcode-beta.app` for optional local Apple builds. Linux integration is the acceptance target.

## Release control and client state

- Keep release discovery event driven and independent of app/CRDT state sync. Preserve the small release-control v1 hello/upgrade envelope as the bootstrap compatibility boundary, including when CRDT protocols change. On every reconnect send the current release hash; publish upgrade events only after the complete signed code replica is active.
- Breaking app/CRDT protocol changes are acceptable during this PoC. Do not let obsolete state-protocol handshakes prevent a client from discovering or loading the current release.
- For now, a disconnected client may discard unconfirmed RAM edits when upgrading. Do not require preservation, migration or durable client storage for those edits. While connected, continue to wait for confirmed edits and finished gestures. Revisit preservation of offline edits as an explicit future requirement before adding it.
