// SPDX-License-Identifier: LGPL-3.0-only
//! Shared Canvas layout, input and presentation primitives.
//! This is a bounded, trusted-peer demo; it does not implement Linux shared memory.

pub mod clock;
pub mod components;
pub mod display;
pub mod fixed;
mod fixed_text;
pub mod font;
pub mod font_reference;
pub mod ink;
pub mod input;
pub mod keyboard;
pub mod layout;
pub mod motion;
mod render;
pub mod scene;
pub use render::{Mesh, render_scene, render_scene_with_font};
pub mod retained_text;

pub type Result<T> = std::result::Result<T, String>;
