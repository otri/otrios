// SPDX-License-Identifier: Apache-2.0
// Migration inventory only; not an independent Canvas app yet.
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "monitoring",
        "Monitoring",
        "legacy-adapter",
        &[
            "crates/otrios-core/src/dashboard/monitoring.rs",
            "crates/otrios-core/src/dashboard/shell.rs",
            "capability:monitoring.read",
        ],
    )
}
