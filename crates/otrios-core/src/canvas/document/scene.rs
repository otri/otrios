// SPDX-License-Identifier: LGPL-3.0-only
//! Preserve scene painter order around the external document viewport. This
//! split runs only when the small UI scene changes, never during document pan.
use super::Result;
use crate::dashboard::scene::Scene;
use otrios_interfaces::scene::SceneNode;

pub(crate) fn split(scene: &Scene, viewport: u32) -> Result<(Scene, Scene)> {
    partition(scene, viewport, false, false)
}
/// Retain a UI subtree in the overlay, preserving the original painter order.
pub(crate) fn split_before(scene: &Scene, viewport: u32) -> Result<(Scene, Scene)> {
    partition(scene, viewport, true, false)
}
/// Split a retained drawing surface after its background, before its children.
pub(crate) fn split_surface(scene: &Scene, viewport: u32) -> Result<(Scene, Scene)> {
    partition(scene, viewport, true, true)
}
fn partition(scene: &Scene, viewport: u32, include: bool, surface: bool) -> Result<(Scene, Scene)> {
    fn walk(
        node: &SceneNode,
        viewport: u32,
        seen: &mut bool,
        include: bool,
        surface: bool,
    ) -> (Option<SceneNode>, Option<SceneNode>) {
        if node.id == viewport {
            *seen = true;
            if surface {
                let mut before = node.clone();
                before.children.clear();
                let mut after = node.clone();
                after.primitive = otrios_interfaces::scene::Primitive::Canvas { clip: true };
                return (Some(before), Some(after));
            }
            return (None, include.then(|| node.clone()));
        }
        if *seen {
            return (None, Some(node.clone()));
        }
        if node.children.is_empty() {
            return (Some(node.clone()), None);
        }
        let mut before = node.clone();
        let mut after = node.clone();
        if surface
            && !matches!(
                after.primitive,
                otrios_interfaces::scene::Primitive::Canvas { .. }
                    | otrios_interfaces::scene::Primitive::Viewport { .. }
            )
        {
            after.primitive = otrios_interfaces::scene::Primitive::Canvas { clip: false };
        }
        before.children.clear();
        after.children.clear();
        for child in &node.children {
            let (a, b) = walk(child, viewport, seen, include, surface);
            before.children.extend(a);
            after.children.extend(b);
        }
        (
            (!before.children.is_empty()).then_some(before),
            (!after.children.is_empty()).then_some(after),
        )
    }
    let mut seen = false;
    let (a, b) = walk(scene.root(), viewport, &mut seen, include, surface);
    if !seen {
        return Err("Missing document painter-order boundary".into());
    }
    let empty = || {
        let mut root = scene.root().clone();
        root.children.clear();
        root
    };
    Ok((
        Scene::new(a.unwrap_or_else(empty))?,
        Scene::new(b.unwrap_or_else(empty))?,
    ))
}
