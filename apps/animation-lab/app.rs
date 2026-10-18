// SPDX-License-Identifier: Apache-2.0
// Migration inventory only; not an independent Canvas app yet.
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "animation-lab",
        "Animation Lab",
        "legacy-adapter",
        &[
            "crates/otrios-core/src/dashboard/motion.rs",
            "crates/otrios-core/src/dashboard/shell.rs",
        ],
    )
}
