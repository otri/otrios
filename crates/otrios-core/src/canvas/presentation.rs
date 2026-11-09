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
    /// Namespace for local resource versions; changes on mount/font reset or clear.
    pub presentation_epoch: u64,
    pub base: TextFrame,
    pub overlay: Option<TextFrame>,
    pub document: Option<document::Frame>,
    pub reflow: Option<reflow::Frame>,
    pub effects: Arc<Vec<f32>>,
    pub viewport: [f32; 2],
    pub motion: Option<[f32; 4]>,
    pub order: Vec<Layer>,
    pub revisions: Revisions,
}

impl Frame {
    /// Document/reflow hosts retain their existing ownership path. A resource
    /// fallback in either UI layer keeps the entire UI on its compatible path.
    pub fn chunked(&self) -> bool {
        use crate::dashboard::retained_text::{MAX_TEXT_CHUNKS, MAX_TEXT_RESOURCE_BYTES};
        if self.document.is_some() || self.reflow.is_some() {
            return false;
        }
        let mut bytes = 0usize;
        let mut resources = 0usize;
        for layer in [Some(&self.base), self.overlay.as_ref()]
            .into_iter()
            .flatten()
        {
            if layer.chunks.is_empty() && !layer.mesh.draws.is_empty() {
                return false;
            }
            if layer.chunks.len() > MAX_TEXT_CHUNKS {
                return false;
            }
            let mut geometry = 0usize;
            let mut glyphs = 0usize;
            let mut draws = 0usize;
            for chunk in &layer.chunks {
                let mesh = &chunk.resource.mesh;
                geometry += mesh.vertices.len() / 6;
                glyphs += mesh.glyph_vertices.len() / 16;
                draws += mesh.draws.len() / 3;
                // Analytic glyphs plus conservative compact-quad/index storage.
                // Mirrors the Web transfer bound, not a driver allocation claim.
                bytes = bytes
                    .saturating_add(mesh.vertices.len() * 4)
                    .saturating_add(mesh.glyph_vertices.len() * 8)
                    .saturating_add(mesh.draws.len() / 3 * 16);
            }
            if geometry > 65536 || glyphs > 65536 || draws > 4096 {
                return false;
            }
            resources += layer.chunks.len();
        }
        resources > 0 && bytes <= MAX_TEXT_RESOURCE_BYTES
    }
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
    presentation_epoch: u64,
}
fn advance(revision: &mut u32) {
    *revision = revision.wrapping_add(1).max(1);
}
impl Composer {
    /// Forget private resources without reusing their revision numbers.
    pub fn clear(&mut self) {
        self.presentation_epoch = self.presentation_epoch.wrapping_add(1).max(1);
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
                self.presentation_epoch = self.presentation_epoch.wrapping_add(1).max(1);
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
                !Arc::ptr_eq(&f.base.paints, &base.paints)
                    || match (&f.overlay, &overlay) {
                        (Some(a), Some(b)) => !Arc::ptr_eq(&a.paints, &b.paints),
                        (None, None) => false,
                        _ => true,
                    }
            }) {
                advance(&mut self.revisions.paint);
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
            presentation_epoch: self.presentation_epoch,
            base,
            overlay,
            document,
            effects,
            order,
            reflow: client.reflow_frame()?,
            viewport: [scene.root().frame.width, scene.root().frame.height],
            motion: client.motion_viewport()?,
            revisions: self.revisions,
        });
        Ok(self.frame.as_ref().unwrap())
    }
}

/// One outstanding immutable frame awaiting renderer consumption. Dirty requests
/// coalesce before allocating and packing a frame the renderer cannot consume.
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
    /// Release capacity once the renderer consumes this immutable frame. This
    /// does not acknowledge display completion. Stale/duplicate releases cannot
    /// grant another frame's capacity.
    pub fn release_capacity(&mut self, serial: u64) -> bool {
        if self.in_flight != Some(serial) {
            return false;
        }
        self.in_flight = None;
        true
    }
    /// Compatibility for hosts that consume a frame only at completed present.
    pub fn presented(&mut self, serial: u64) -> bool {
        self.release_capacity(serial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combined_ui_resource_budget_falls_back_before_host_transfer() {
        use crate::dashboard::{
            Mesh,
            retained_text::{TextChunk, TextResource},
        };
        let resource = Arc::new(TextResource {
            version: 1,
            mesh: Arc::new(Mesh {
                vertices: vec![],
                glyph_vertices: vec![0.; 65536 * 16],
                draws: vec![1, 0, 65536],
            }),
            styles: Arc::new(vec![1.]),
        });
        let layer = TextFrame {
            chunks: vec![TextChunk {
                resource,
                views: vec![0.; 8],
                paints: Arc::new(vec![1.; 4]),
            }],
            ..Default::default()
        };
        let mut frame = Frame {
            base: layer.clone(),
            ..Default::default()
        };
        assert!(frame.chunked());
        frame.overlay = Some(layer);
        assert!(
            !frame.chunked(),
            "combined glyph resources exceed the host budget although each layer fits"
        );
    }
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
    #[test]
    fn consumed_frame_allows_one_successor_before_late_present() {
        let mut demand = Demand::default();
        demand.request();
        let first = demand.begin().unwrap();
        demand.request();
        assert!(demand.release_capacity(first));
        let second = demand.begin().unwrap();
        for _ in 0..1000 {
            demand.request();
            assert!(!demand.release_capacity(first));
            assert!(!demand.presented(first));
            assert_eq!(demand.begin(), None);
        }
        assert!(demand.release_capacity(second));
        assert!(demand.begin().is_some());
        assert!(!demand.presented(second));
    }
}
