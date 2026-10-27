// SPDX-License-Identifier: LGPL-3.0-only
//! Preserve scene painter order around the external document viewport. This
//! split runs only when the small UI scene changes, never during document pan.
use super::Result;
use crate::dashboard::scene::Scene;
use otrios_interfaces::scene::SceneNode;

pub(crate) fn split(scene: &Scene, viewport: u32) -> Result<(Scene, Scene)> {
    fn walk(
        node: &SceneNode,
        viewport: u32,
        seen: &mut bool,
    ) -> (Option<SceneNode>, Option<SceneNode>) {
        if node.id == viewport {
            *seen = true;
            return (None, None);
        }
        if *seen {
            return (None, Some(node.clone()));
        }
        if node.children.is_empty() {
            return (Some(node.clone()), None);
        }
        let mut before = node.clone();
        let mut after = node.clone();
        before.children.clear();
        after.children.clear();
        for child in &node.children {
            let (a, b) = walk(child, viewport, seen);
            before.children.extend(a);
            after.children.extend(b);
        }
        (
            (!before.children.is_empty()).then_some(before),
            (!after.children.is_empty()).then_some(after),
        )
    }
    let mut seen = false;
    let (a, b) = walk(scene.root(), viewport, &mut seen);
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
