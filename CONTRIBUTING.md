# Contributing to OTriOS

Use an issue to define a bounded change and its acceptance criteria. Implement on a feature branch and open a pull request. Include the checks run and describe any missing Linux validation.

Run `bash scripts/check.sh` before requesting review. New dependencies require a reason, license review, and a committed lockfile. Public contract changes need versioning and compatibility notes. New unsafe code requires explicit safety invariants and tests around the boundary.

Contributions are offered under the license applicable to the affected path, as described in LICENSE.md. Preserve attribution and identify copied or generated material. Contributors retain their copyrights; this scaffold does not introduce a contributor license agreement or copyright assignment.

Review should focus on observable behavior, failure handling, memory lifetime, capability boundaries, and the runtime/interface license split. Merge after a human review and successful required checks once repository rules are configured.
