# Tess OTriOS handoff

Prepared for Tess; no task has been assigned and no integration is configured. Repository access, the execution identity, and a supported task-launch mechanism must be confirmed before dispatch. Environment sharing alone does not prove Tess can use it.

## First task

Implement **P1 Content store** from docs/POC.md on a feature branch. Read AGENTS.md, LICENSE.md, and docs/ARCHITECTURE.md first. Keep P2 through P5 as later work.

Define and document a small canonical envelope with schema/version identity, payload length, and explicit limits. Add a standard SHA-256 dependency after checking its license and maintenance provenance. Implement an in-memory immutable content store with deduplication and explicit errors for corrupt input, missing content, and exhausted limits. Keep the store implementation in otrios-core and only needed contract types in otrios-interfaces. Preserve the SDK's independence from runtime implementation.

## Acceptance criteria

- A known SHA-256 fixture verifies the chosen encoding; changing schema identity, version, or bytes changes the digest.
- Equal canonical objects deduplicate, and published bytes cannot be modified through the store API.
- Invalid lengths, overflow, oversized objects, and total-store exhaustion fail without partial state.
- Lookup of missing content returns an explicit error. Old references continue returning their original bytes after inserting new revisions.
- All checks in scripts/check.sh pass; Cargo.lock is updated; tests include boundary values and failed-insert state preservation.
- The pull request describes public API changes, the dependency/license decision, test commands and results, and remaining limitations. Do not claim later milestones are implemented.

## Completion report

Provide the branch or PR URL, implemented criteria, commands and results, and any design question needing review. Human review and merge follow the report. Do not assign other agents, add collaborators, change repository access, publish packages, or start later milestones as part of this task.

## Integration checklist before dispatch

The user confirms Tess's supported integration and identity; the repository is visible to that identity; the OTriOS environment is published and a fresh task passes checks; branch/PR permissions are sufficient; and the user explicitly asks to dispatch the handoff. Until then this file is a ready-to-use task brief.
