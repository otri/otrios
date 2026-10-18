// SPDX-License-Identifier: Apache-2.0
//! Monitoring adapter entry; app-specific collection/projection lives with the app.
#[path = "../../../apps/monitoring/adapter.rs"]
mod adapter;
fn main() -> Result<(), String> {
    adapter::main()
}
