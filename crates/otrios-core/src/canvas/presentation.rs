// SPDX-License-Identifier: LGPL-3.0-only
//! Host-independent retained frame assembly. GPU objects stay with each host.
use super::{Client, Result, document, reflow};
use crate::dashboard::{
    font::Font,
    retained_text::{TextFrame, TextPresentation},
    scene::Scene,
};
use std::sync::Arc;

pub use otrios_interfaces::presentation::{Layer, Revisions};

#[derive(Clone, Default)]
pub struct Frame {
    pub base: TextFrame,
    pub overlay: Option<TextFrame>,
    pub document: Option<document::Frame>,
    pub reflow: Option<reflow::Frame>,
    pub effects: Arc<Vec<f32>>,
    pub viewport: [f32; 2],
    pub order: Vec<Layer>,
    pub revisions: Revisions,
}

#[derive(Default)]
pub struct Composer {
    scene: Option<Arc<Scene>>,
    font: String,
    epoch: u32,
    base: TextPresentation,
    overlay: TextPresentation,
    frame: Option<Frame>,
    revisions: Revisions,
}
fn advance(revision: &mut u32) {
    *revision = revision.wrapping_add(1).max(1);
}
impl Composer {
    /// Forget private resources without reusing their revision numbers.
    pub fn clear(&mut self) {
        self.scene = None;
        self.frame = None;
        self.base = TextPresentation::default();
        self.overlay = TextPresentation::default();
        self.font.clear();
    }
    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }
    /// Call only after obtaining renderer capacity. State/input can continue
    /// changing while no capacity is available; only their latest view is built.
    pub fn compose(&mut self, client: &mut Client, font: &Font) -> Result<&Frame> {
        let scene = client.scene()?;
        let changed = self.epoch != client.epoch()
            || self.font != font.hash()
            || self.scene.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &scene));
        let (base, overlay) = if changed {
            if self.epoch != client.epoch() || self.font != font.hash() {
                self.base = TextPresentation::default();
                self.overlay = TextPresentation::default();
            }
            let (base, overlay) =
                if let Some((before, after)) = client.document_scene_parts(&scene)? {
                    (
                        self.base.compose(&before, font),
                        Some(self.overlay.compose(&after, font)),
                    )
                } else {
                    (self.base.compose(&scene, font), None)
                };
            let old = self.frame.as_ref();
            if old.is_none_or(|f| {
                !Arc::ptr_eq(&f.base.mesh, &base.mesh)
                    || match (&f.overlay, &overlay) {
                        (Some(a), Some(b)) => !Arc::ptr_eq(&a.mesh, &b.mesh),
                        (None, None) => false,
                        _ => true,
                    }
            }) {
                advance(&mut self.revisions.content);
            }
            if old.is_none_or(|f| {
                f.base.views != base.views
                    || f.viewport != [scene.root().frame.width, scene.root().frame.height]
                    || f.overlay.as_ref().map(|f| &f.views) != overlay.as_ref().map(|f| &f.views)
            }) {
                advance(&mut self.revisions.placement);
            }
            advance(&mut self.revisions.semantics);
            self.epoch = client.epoch();
            self.font = font.hash().into();
            self.scene = Some(scene.clone());
            (base, overlay)
        } else {
            let old = self.frame.as_ref().ok_or("Missing retained frame")?;
            (old.base.clone(), old.overlay.clone())
        };
        let effects = client.effect_vertices()?;
        client.feedback_prepared();
        let effects = if let Some(old) = &self.frame
            && old.effects.as_ref() == &effects
        {
            old.effects.clone()
        } else {
            advance(&mut self.revisions.effects);
            Arc::new(effects)
        };
        let document = client.document_frame();
        let mut order = vec![Layer::Base];
        if document.is_some() {
            order.push(Layer::Document);
        }
        if overlay.is_some() {
            order.push(Layer::Overlay);
        }
        if !effects.is_empty() {
            order.push(Layer::Effects);
        }
        self.frame = Some(Frame {
            base,
            overlay,
            document,
            effects,
            order,
            reflow: client.reflow_frame()?,
            viewport: [scene.root().frame.width, scene.root().frame.height],
            revisions: self.revisions,
        });
        Ok(self.frame.as_ref().unwrap())
    }
}

/// One outstanding immutable frame. Dirty requests coalesce before composition,
/// not after allocating and packing a frame that the renderer cannot consume.
#[derive(Default)]
pub struct Demand {
    dirty: bool,
    in_flight: Option<u64>,
    serial: u64,
}
impl Demand {
    pub fn request(&mut self) {
        self.dirty = true;
    }
    pub fn begin(&mut self) -> Option<u64> {
        if !self.dirty || self.in_flight.is_some() {
            return None;
        }
        self.serial = self.serial.wrapping_add(1).max(1);
        self.in_flight = Some(self.serial);
        self.dirty = false;
        Some(self.serial)
    }
    /// Stale/duplicate acknowledgements cannot grant another frame's capacity.
    pub fn presented(&mut self, serial: u64) -> bool {
        if self.in_flight != Some(serial) {
            return false;
        }
        self.in_flight = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_renderer_coalesces_work_and_ignores_stale_credits() {
        let mut demand = Demand::default();
        assert_eq!(demand.begin(), None);
        demand.request();
        let first = demand.begin().unwrap();
        for _ in 0..1000 {
            demand.request();
            assert_eq!(demand.begin(), None);
        }
        assert!(!demand.presented(first + 1));
        assert_eq!(demand.begin(), None);
        assert!(demand.presented(first));
        let last = demand.begin().unwrap();
        assert!(!demand.presented(first));
        assert!(demand.presented(last));
        assert_eq!(demand.begin(), None);
    }
}
