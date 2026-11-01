<!-- SPDX-License-Identifier: Apache-2.0 -->
# Spaces map M1 acceptance evidence

See the [Frame acceptance report](../../spaces-map-frame-2026-11-01.md) for scope,
commands and limitations. JSON records contain technical counts and hashes only.
All screenshots use disposable browser fixtures. Private Frame pixels and data
payloads remain outside the repository.

- `final-candidate.json`: frozen source, exact signed Wasm and complete signature/closure verification.
- `browser-map.json` and `browser-regression.json`: isolated Chromium qualification against that exact signed Wasm.
- `map-edit.png`, `map-390.png`, `space-settings.png`, `app-settings.png`, `add-app.png`: synthetic visual checks, including restored icons and centered names.
- `backup-mac.json`: verified private operational archive, before final cutover.
- `initial-migration.json`: earlier OTSP5 → OTSP6 metadata comparison; final code deployment stays OTSP6.
- `deployment.json`: final deployment, compatible code rollback/reactivation, state audits, physical native readiness/navigation and restored collector.
- `local-checks.json`: required repository validation and CI provenance.
- `arm-qualification.json`: isolated physical ARM reconnect test from its explicitly recorded earlier source; final native acceptance is in `deployment.json`.

A code rollback retains the latest registry, ledger, owner lock and paired storage.
No data snapshot is restored over current confirmed state. Aaron reported that
the updated appearance looked good. Physical finger-touch observation was not
separately reported.
