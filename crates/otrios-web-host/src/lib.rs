// SPDX-License-Identifier: Apache-2.0
//! Browser adapter; Canvas state/guest memory live only in the worker's RAM.
use otrios_core::canvas::{Client, decode_catalog};
use otrios_core::dashboard::{Mesh, font::Font, render_scene_with_font, scene::Scene};
use otrios_interfaces::scene::{PointerEvent, PointerPhase, Primitive};
use otrios_interfaces::{
    canvas::{Catalog, Presentation},
    keyboard::Edit,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
#[derive(Default)]
pub struct CanvasApp {
    catalog: Option<Catalog>,
    font: Option<Arc<Font>>,
    rendered: Option<(Arc<Scene>, Mesh)>,
    inner: Client,
}
#[wasm_bindgen]
impl CanvasApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn open(&mut self, ui: &str, state: &str) {
        self.inner.open(ui, state)
    }
    pub fn admit_catalog(&mut self, bytes: &[u8]) -> Result<(), String> {
        let catalog = decode_catalog(bytes)?;
        self.inner.admit_catalog(bytes)?;
        self.catalog = Some(catalog);
        Ok(())
    }
    pub fn screen(&self) -> String {
        self.catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|a| a.app.ui_ref == self.inner.ui_ref()))
            .map(|e| e.app.id.clone())
            .unwrap_or_else(|| "home".into())
    }
    pub fn navigate(&mut self, name: &str) -> Result<(), String> {
        let app = self
            .catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|a| a.app.id == name))
            .ok_or("Unknown admitted app")?
            .app
            .clone();
        self.inner.open(&app.ui_ref, &app.state_ref);
        Ok(())
    }
    pub fn set_session_controls(&mut self, enabled: bool) -> bool {
        self.inner.set_session_controls(enabled)
    }
    pub fn take_sign_out(&mut self) -> bool {
        self.inner.take_sign_out()
    }
    pub fn auth_required(&self) -> bool {
        self.inner.auth_required()
    }
    pub fn rejected_mount_pending(&self) -> bool {
        self.inner.rejected_mount_pending()
    }
    pub fn private(&self) -> bool {
        self.inner.private()
    }
    pub fn pending_ui_ref(&self) -> String {
        self.inner.pending_ui_ref()
    }
    pub fn pending_state_ref(&self) -> String {
        self.inner.pending_state_ref()
    }
    pub fn retry_mount(&mut self) {
        self.inner.retry_mount();
    }
    pub fn reset_connection(&mut self) {
        self.inner.reset_connection();
    }
    pub fn clear_private(&mut self) {
        self.inner.clear_private();
        self.rendered = None;
    }
    pub fn input_busy(&self) -> bool {
        self.inner.input_busy()
    }
    pub fn keyboard_visible(&self) -> bool {
        self.inner.keyboard_visible()
    }
    pub fn editing(&self) -> bool {
        self.inner.editing()
    }
    pub fn show_keyboard(&mut self) -> Result<bool, String> {
        self.inner.show_keyboard()
    }
    pub fn back(&mut self) -> Result<bool, String> {
        self.inner.back()
    }
    pub fn cancel_input(&mut self) -> Result<bool, String> {
        self.inner.cancel()
    }
    pub fn tick(&mut self, now: f64, wall: f64) -> Result<bool, String> {
        self.inner.tick(now.max(0.) as u64, wall.max(0.) as i64)
    }
    pub fn ink_outgoing(&mut self, now: f64) -> Vec<u8> {
        self.inner.ink_outgoing(now.max(0.) as u64)
    }
    pub fn receive_ink(&mut self, bytes: &[u8], now: f64) -> Result<bool, String> {
        self.inner.receive_ink(bytes, now.max(0.) as u64)
    }
    pub fn type_text(&mut self, text: &str) -> Result<bool, String> {
        self.inner.hardware_edit(&Edit::Commit(text.into()))
    }
    pub fn key(&mut self, name: &str, select: bool) -> Result<bool, String> {
        if name == "Escape" {
            return self.inner.back();
        }
        let edit = match name {
            "Backspace" => Edit::Backspace,
            "ArrowLeft" => Edit::Left { select },
            "ArrowRight" => Edit::Right { select },
            "SelectAll" => Edit::SelectAll,
            "Tab" => Edit::Tab { backward: select },
            "Enter" => Edit::Enter,
            _ => return Ok(false),
        };
        self.inner.hardware_edit(&edit)
    }
    pub fn presentation_kind(&self) -> String {
        match self.inner.presentation() {
            Some(Presentation::Motion { .. }) => "motion",
            Some(Presentation::Editor { .. }) => "input",
            Some(Presentation::TouchPad { .. }) => "touch-pad",
            _ => "none",
        }
        .into()
    }
    pub fn motion_intent(&self) -> Result<Vec<f32>, String> {
        Ok(match self.inner.motion_intent()? {
            Some((three_d, intent)) => vec![
                u8::from(intent.playing) as f32,
                intent.speed,
                intent.count as f32,
                u8::from(three_d) as f32,
            ],
            None => vec![],
        })
    }
    pub fn adjust_control(&mut self, id: u32, dx: i16, dy: i16) -> Result<bool, String> {
        self.inner.adjust_control(id, dx, dy)
    }
    pub fn control_value(&self, id: u32) -> Result<f64, String> {
        Ok(self.inner.control_value(id)?.unwrap_or(f64::NAN))
    }
    pub fn stress_tick(&mut self, now: f64) -> Result<bool, String> {
        self.inner.stress_tick(now)
    }
    pub fn stress_updates(&self) -> f64 {
        self.inner.stress_updates() as f64
    }
    pub fn stress_stall_ms(&self) -> f64 {
        self.inner.stress_stall_ms() as f64
    }
    pub fn control_draggable(&self, id: u32) -> Result<bool, String> {
        Ok(self.inner.scene()?.controls().iter().any(|c| {
            c.node.id == id
                && matches!(
                    c.node.responder,
                    Some(otrios_interfaces::scene::TouchResponder::Drag(_))
                )
        }))
    }
    pub fn control_checked(&self, id: u32) -> Result<bool, String> {
        Ok(self
            .inner
            .scene()?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .and_then(|c| c.node.semantics.as_ref())
            .is_some_and(|s| s.checked))
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.inner.outgoing()
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool, String> {
        self.inner.receive(bytes)
    }
    pub fn epoch(&self) -> u32 {
        self.inner.epoch()
    }
    pub fn ui_ref(&self) -> String {
        self.inner.ui_ref()
    }
    pub fn state_ref(&self) -> String {
        self.inner.state_ref()
    }
    pub fn title(&self) -> String {
        self.inner.title()
    }
    pub fn confirmed(&mut self) -> bool {
        self.inner.confirmed()
    }
    pub fn vertices(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.compose()?.vertices.clone())
    }
    pub fn glyph_vertices(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.compose()?.glyph_vertices.clone())
    }
    pub fn draws(&mut self) -> Result<Vec<u32>, String> {
        Ok(self.compose()?.draws.clone())
    }
    pub fn set_font(&mut self, bytes: &[u8], hash: &str) -> Result<(), String> {
        let font = Arc::new(Font::from_zstd(bytes, hash)?);
        self.font = Some(font);
        self.rendered = None;
        Ok(())
    }
    pub fn font_dimensions(&self) -> Result<Vec<u32>, String> {
        let a = self
            .font
            .as_ref()
            .ok_or("Font resource not loaded")?
            .atlas();
        Ok(vec![
            a.curve_width,
            a.curve_height,
            a.band_width,
            a.band_height,
        ])
    }
    pub fn font_curves(&self) -> Result<Vec<f32>, String> {
        Ok(self
            .font
            .as_ref()
            .ok_or("Font resource not loaded")?
            .atlas()
            .curves
            .clone())
    }
    pub fn font_bands(&self) -> Result<Vec<u8>, String> {
        Ok(self
            .font
            .as_ref()
            .ok_or("Font resource not loaded")?
            .atlas()
            .bands
            .clone())
    }

    pub fn text(&self) -> Result<String, String> {
        fn visit(n: &otrios_interfaces::scene::SceneNode, out: &mut Vec<String>) {
            if let Primitive::Text { value, .. } = &n.primitive {
                out.push(value.clone())
            }
            for c in &n.children {
                visit(c, out)
            }
        }
        let mut out = vec![];
        visit(self.inner.scene()?.root(), &mut out);
        Ok(out.join(". "))
    }
    pub fn controls(&self) -> Result<Vec<u32>, String> {
        Ok(self
            .inner
            .scene()?
            .controls()
            .iter()
            .map(|c| c.node.id)
            .collect())
    }
    pub fn label(&self, id: u32) -> Result<String, String> {
        Ok(self
            .inner
            .scene()?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .and_then(|c| c.node.semantics.as_ref())
            .map(|s| s.label.clone())
            .unwrap_or_default())
    }
    pub fn bounds(&self, id: u32) -> Result<Vec<f32>, String> {
        Ok(self
            .inner
            .scene()?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .map(|c| {
                vec![
                    c.origin[0],
                    c.origin[1],
                    c.node.frame.width * c.scale,
                    c.node.frame.height * c.scale,
                ]
            })
            .unwrap_or_default())
    }
    pub fn activate(&mut self, epoch: u32, id: u32) -> Result<bool, String> {
        self.inner.activate(epoch, id)
    }
    pub fn pointer(
        &mut self,
        epoch: u32,
        phase: u8,
        id: u32,
        x: f32,
        y: f32,
    ) -> Result<bool, String> {
        let phase = match phase {
            0 => PointerPhase::Down,
            1 => PointerPhase::Move,
            2 => PointerPhase::Up,
            3 => PointerPhase::Cancel,
            _ => return Err("Invalid pointer phase".into()),
        };
        self.inner.pointer(
            epoch,
            PointerEvent {
                id,
                phase,
                position: [x, y],
            },
        )
    }
}

impl CanvasApp {
    fn compose(&mut self) -> Result<&Mesh, String> {
        let scene = self.inner.scene()?;
        if self
            .rendered
            .as_ref()
            .is_none_or(|(old, _)| !Arc::ptr_eq(old, &scene))
        {
            let font = self.font.as_ref().ok_or("Font resource not loaded")?;
            let mesh = render_scene_with_font(&scene, font);
            self.rendered = Some((scene, mesh));
        }
        Ok(&self.rendered.as_ref().ok_or("No rendered scene")?.1)
    }
}

/// Has no CRDT, networking, storage, navigation or app-owned text.
#[wasm_bindgen]
#[derive(Default)]
pub struct CanvasPresentation {
    font: Option<Arc<Font>>,
    rendered_hud: Option<(u64, Mesh)>,
    updates: u64,
    inner: otrios_core::dashboard::motion::Presenter,
}
#[wasm_bindgen]
impl CanvasPresentation {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn vertices(
        &mut self,
        three_d: bool,
        playing: bool,
        speed: f32,
        count: u16,
        now: f64,
        updates: f64,
    ) -> Result<Vec<f32>, String> {
        let i = otrios_core::dashboard::motion::Intent {
            playing,
            speed,
            count,
        };
        i.validate()?;
        self.updates = updates.max(0.) as u64;
        Ok(self
            .inner
            .mesh(three_d, i, now, updates.max(0.) as u64)
            .vertices)
    }
    pub fn suspend(&mut self) {
        self.inner.suspend();
    }
    pub fn hud_revision(&self) -> f64 {
        self.inner.hud_revision() as f64
    }
    pub fn hud(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.compose_hud()?.vertices.clone())
    }
    pub fn hud_glyph_vertices(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.compose_hud()?.glyph_vertices.clone())
    }
    pub fn hud_draws(&mut self) -> Result<Vec<u32>, String> {
        Ok(self.compose_hud()?.draws.clone())
    }
    pub fn set_font(&mut self, bytes: &[u8], hash: &str) -> Result<(), String> {
        self.font = Some(Arc::new(Font::from_zstd(bytes, hash)?));
        self.rendered_hud = None;
        Ok(())
    }
    pub fn font_dimensions(&self) -> Result<Vec<u32>, String> {
        let a = self
            .font
            .as_ref()
            .ok_or("Font resource not loaded")?
            .atlas();
        Ok(vec![
            a.curve_width,
            a.curve_height,
            a.band_width,
            a.band_height,
        ])
    }
    pub fn font_curves(&self) -> Result<Vec<f32>, String> {
        Ok(self
            .font
            .as_ref()
            .ok_or("Font resource not loaded")?
            .atlas()
            .curves
            .clone())
    }
    pub fn font_bands(&self) -> Result<Vec<u8>, String> {
        Ok(self
            .font
            .as_ref()
            .ok_or("Font resource not loaded")?
            .atlas()
            .bands
            .clone())
    }

    pub fn stats(&self) -> String {
        self.inner.stats()
    }
}

impl CanvasPresentation {
    fn compose_hud(&mut self) -> Result<&Mesh, String> {
        let revision = self.inner.hud_revision();
        if self
            .rendered_hud
            .as_ref()
            .is_none_or(|(old, _)| *old != revision)
        {
            let font = self.font.as_ref().ok_or("Font resource not loaded")?;
            let mesh = otrios_core::dashboard::motion::render_hud_with_font(
                &self.inner.hud_label(self.updates),
                font,
            );
            self.rendered_hud = Some((revision, mesh));
        }
        Ok(&self.rendered_hud.as_ref().ok_or("No HUD")?.1)
    }
}
