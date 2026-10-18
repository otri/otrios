// SPDX-License-Identifier: Apache-2.0
// Migration inventory only; not an independent Canvas app yet.
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "input-lab",
        "Input Lab",
        "legacy-adapter",
        &[
            "crates/otrios-core/src/dashboard/keyboard.rs",
            "crates/otrios-core/src/dashboard/shell.rs",
        ],
    )
}
