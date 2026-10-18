# OTriOS Codex Cloud setup

Prepare a reusable Linux environment for `otri/otrios`, then validate a fresh task before handing off implementation. This file is a setup recipe; committing it does not create or publish a Cloud environment.

## Environment configuration

| Setting | Initial value |
| --- | --- |
| Name | OTriOS PoC |
| Repository | `otri/otrios`, default branch `main` |
| Execution | Linux; Rust 1.90.0; standard native linker and Git |
| Install script | `bash scripts/cloud-install.sh` from the repository checkout |
| Start skill | No daemon at scaffold stage; run `bash scripts/check.sh` |
| Secrets | None required for the scaffold |
| Internet | Package managers preset for toolchain and later dependency downloads |
| Environment sharing | Keep personal initially; configure collaborators after their accounts and workspace support are confirmed |

The current workspace has no external Rust dependencies. After toolchain preparation, its check script runs offline. Do not put GitHub tokens or signing keys in source files or install scripts. Use account-managed repository access and environment secret controls only if a later task needs them.

## Create and verify

1. Sign in to ChatGPT on the web or desktop app. In a new task choose **Work in > Cloud > Select environment > Create environment**, or use **Settings > Codex Cloud > Environments > Create environment**.
2. Select `otri/otrios` and **Get started**. If it is missing, check the connected GitHub account's repository selection and installation access. Adding repository access may require the owner's action.
3. Ask setup to install official rustup if absent, use the repository's pinned toolchain, and run `bash scripts/cloud-install.sh`. Review installation output and all checks. No background services are needed yet.
4. Save the tested setup and select **Publish**. Wait for **Environment published**. Saving a draft alone is insufficient.
5. Start a fresh task in that environment and run `bash scripts/check.sh`. Record the environment identity, checked commit, Linux platform, and successful output.
6. Once Tess's integration is confirmed, verify that her execution identity can see the repository and start the environment. Grant only the access the user approves, then dispatch the brief when requested.

Cloud tasks use separate working files. Sharing the environment does not share someone else's running task or automatically grant repository permissions. Shared workspace controls depend on the account and workspace capabilities.

## Setup request to paste

> Prepare this repository for the OTriOS Linux userspace PoC. Read AGENTS.md and docs/CODEX_CLOUD.md. Use Rust 1.90.0 with rustfmt and clippy, plus a native linker. Install official rustup if needed, then run bash scripts/cloud-install.sh from the repository checkout. No application services or secrets are required at this stage. Show build and test results and any missing configuration before publishing. Do not implement PoC milestones during environment setup.

## Sources

The [official Cloud environment guide](https://learn.chatgpt.com/docs/environments/cloud-environments) documents environment creation, install scripts, publication, sharing, network settings, and fresh-task behavior. The [Cloud overview](https://learn.chatgpt.com/docs/cloud) describes the setup and review flow. Account access and available controls must be verified in the signed-in UI.
