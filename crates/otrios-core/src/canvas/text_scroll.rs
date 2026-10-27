// SPDX-License-Identifier: LGPL-3.0-only
//! Local, text-only page panning. Hosts retain glyph buffers and submit view ranges.
use super::{Result, text_viewport::TextViewport};
use crate::dashboard::{Mesh, font::Font, scene::Scene};
use otrios_interfaces::scene::{PointerEvent, PointerPhase, SceneNode};
use std::collections::VecDeque;
#[derive(Clone)]
pub(super) struct Scroll {
    node: u32,
    content: [f32; 2],
    axes: [TextViewport; 2],
    scale: f32,
    pub bounds_dirty: bool,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Info {
    pub offset: [f32; 2],
    pub limit: [f32; 2],
    pub active: bool,
    pub velocity: [f64; 2],
}
impl Scroll {
    pub fn new(node: u16, content: [f32; 2]) -> Self {
        Self {
            node: u32::from(node),
            content,
            axes: Default::default(),
            scale: 1.,
            bounds_dirty: true,
        }
    }
    pub fn info(&self) -> Info {
        Info {
            offset: self.axes.each_ref().map(|a| a.offset),
            limit: self.axes.each_ref().map(|a| a.limit()),
            active: self.axes.iter().any(TextViewport::animating),
            velocity: self.axes.each_ref().map(|a| a.velocity()),
        }
    }
    pub fn touching(&self) -> bool {
        self.axes.iter().any(TextViewport::active)
    }
    pub fn cancel(&mut self) {
        for a in &mut self.axes {
            a.cancel();
        }
    }
    pub fn sync(&mut self, scene: &Scene) -> Result<()> {
        let path = scene.path(self.node).ok_or("Missing scroll viewport")?;
        let p = path.last().ok_or("Missing scroll frame")?;
        self.axes[0].bounds(self.content[0], p.node.frame.width);
        self.axes[1].bounds(self.content[1], p.node.frame.height);
        self.bounds_dirty = false;
        Ok(())
    }
    pub fn tick(&mut self, now: u64) -> bool {
        let active = self.info().active;
        let a = self.axes[0].tick(now);
        let b = self.axes[1].tick(now);
        a || b || active != self.info().active
    }
    pub fn pointer(
        &mut self,
        scene: &Scene,
        event: PointerEvent,
        now: u64,
    ) -> Result<Option<bool>> {
        if event.position.iter().any(|v| !v.is_finite()) {
            return Ok(None);
        }
        let owned = self.axes[0].owns(event.id);
        if event.phase == PointerPhase::Down {
            if self.touching() {
                return Ok(Some(false));
            }
            let path = scene.path(self.node).ok_or("Missing scroll viewport")?;
            let p = path.last().ok_or("Missing scroll frame")?;
            let local = [
                (event.position[0] - p.origin[0]) / p.scale,
                (event.position[1] - p.origin[1]) / p.scale,
            ];
            if local[0] < 0.
                || local[1] < 0.
                || local[0] >= p.node.frame.width
                || local[1] >= p.node.frame.height
            {
                return Ok(None);
            }
            self.sync(scene)?;
            self.scale = p.scale;
            for i in 0..2 {
                self.axes[i].down(event.id, event.position[i] / self.scale, now);
            }
        } else if !owned {
            return Ok(None);
        } else {
            match event.phase {
                PointerPhase::Move => {
                    for i in 0..2 {
                        self.axes[i].motion(event.position[i] / self.scale, now);
                    }
                }
                PointerPhase::Up => {
                    for i in 0..2 {
                        self.axes[i].up(event.position[i] / self.scale, now);
                    }
                }
                PointerPhase::Cancel => self.cancel(),
                PointerPhase::Down => unreachable!(),
            }
        }
        Ok(Some(true))
    }
    pub fn present(&self, root: &mut SceneNode) {
        fn apply(node: &mut SceneNode, id: u32, offset: [f32; 2]) {
            if node.id == id {
                for child in &mut node.children {
                    child.frame.x -= offset[0];
                    child.frame.y -= offset[1];
                }
            } else {
                for child in &mut node.children {
                    apply(child, id, offset);
                }
            }
        }
        apply(root, self.node, self.info().offset);
    }
}
/// Counts submitted browser frames or completed native page flips, never worker ticks.
#[derive(Default)]
pub struct FrameMeter {
    last: Option<f64>,
    samples: VecDeque<(f64, f64)>,
}
impl FrameMeter {
    pub fn record(&mut self, now_ms: f64) {
        if !now_ms.is_finite() {
            return;
        }
        if let Some(last) = self.last {
            let dt = now_ms - last;
            if dt > 0. && dt < 500. {
                self.samples.push_back((now_ms, dt));
            } else {
                self.samples.clear();
            }
        }
        self.last = Some(now_ms);
        while self.samples.len() > 240 || self.samples.front().is_some_and(|p| now_ms - p.0 > 1000.)
        {
            self.samples.pop_front();
        }
    }
    pub fn label(&self, active: bool) -> String {
        if !active {
            return "Idle - drag in any direction; release to coast".into();
        }
        let total: f64 = self.samples.iter().map(|p| p.1).sum();
        if total == 0. {
            "Measuring scroll frames...".into()
        } else {
            format!(
                "{:.1} fps - retained text / momentum scroll",
                self.samples.len() as f64 * 1000. / total
            )
        }
    }
}
pub fn hud(label: &str, extent: [f32; 2], font: &Font) -> Result<Mesh> {
    use crate::dashboard::components::{BG, WHITE, background, canvas, text};
    use otrios_interfaces::scene::Rect;
    let scene = Scene::new(canvas(
        0,
        Rect::new(0., 0., extent[0], extent[1]),
        vec![
            background(1, extent[0], 48., BG, 0.),
            text(
                2,
                Rect::new(24., extent[1] - 40., extent[0] - 48., 24.),
                label,
                2,
                WHITE,
            ),
        ],
    ))?;
    // The background covers the footer, not the app's header.
    let mut root = scene.root().clone();
    root.children[0].frame.y = extent[1] - 48.;
    Ok(crate::dashboard::render_scene_with_font(
        &Scene::new(root)?,
        font,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meter_uses_real_frames_and_discards_idle_gaps() {
        let mut m = FrameMeter::default();
        for i in 0..61 {
            m.record(f64::from(i) * 1000. / 60.);
        }
        assert!(m.label(true).starts_with("60.0 fps"));
        m.record(5000.);
        assert_eq!(m.label(true), "Measuring scroll frames...");
        assert!(m.label(false).starts_with("Idle"));
    }
}
