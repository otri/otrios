// SPDX-License-Identifier: Apache-2.0
//! Renderer-owned UI resources. Document slots are mutually exclusive with these.
use otrios_core::canvas::presentation::Frame;
use otrios_core::dashboard::retained_text::{
    MAX_TEXT_CHUNKS, MAX_TEXT_RESOURCE_BYTES, TextResource,
};
#[cfg(all(target_os = "linux", feature = "direct"))]
use otrios_linux_display::{Display, MAX_SCENE_SLOTS};
/// Same-thread graphics owner; isolated here so ownership transitions can be
/// checked without a device or a copied runtime implementation.
pub(super) trait Backend {
    fn slots(&self) -> u32;
    fn retire(&mut self, slot: u32) -> Result<(), String>;
    fn upload(&mut self, slot: u32, resource: &TextResource) -> Result<(), String>;
    fn metadata(&mut self, slot: u32, views: &[f32], paints: &[f32]) -> Result<(), String>;
}
#[cfg(all(target_os = "linux", feature = "direct"))]
impl Backend for Display {
    fn slots(&self) -> u32 {
        MAX_SCENE_SLOTS
    }
    fn retire(&mut self, slot: u32) -> Result<(), String> {
        self.retire_slot(slot)
    }
    fn upload(&mut self, slot: u32, resource: &TextResource) -> Result<(), String> {
        self.upload_scene(
            slot,
            &resource.mesh.vertices,
            &resource.mesh.glyph_vertices,
            &resource.mesh.draws,
        )?;
        self.text_styles(slot, &resource.styles)
    }
    fn metadata(&mut self, slot: u32, views: &[f32], paints: &[f32]) -> Result<(), String> {
        self.text_views(slot, views)?;
        self.text_paints(slot, paints)
    }
}
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
struct Entry {
    slot: u32,
    resource: Arc<TextResource>,
}
#[derive(Default)]
pub(super) struct Cache {
    entries: BTreeMap<(usize, u64), Entry>,
    epoch: Option<u64>,
}
#[derive(Default, Debug)]
pub(super) struct Update {
    pub slots: [Vec<u32>; 2],
    pub bytes: usize,
    pub uploads: [usize; 2],
}
impl Cache {
    pub fn clear(&mut self, display: &mut impl Backend) -> Result<(), String> {
        for entry in self.entries.values() {
            display.retire(entry.slot)?;
        }
        self.entries.clear();
        self.epoch = None;
        Ok(())
    }
    /// Publish the complete bounded candidate in painter order. No partial UI
    /// frame is drawn; unchanged immutable resources retain their GPU buffers.
    pub fn update(&mut self, display: &mut impl Backend, frame: &Frame) -> Result<Update, String> {
        if !frame.chunked() {
            return Err("UI resource candidate requires the retained path".into());
        }
        let layers = [Some(&frame.base), frame.overlay.as_ref()];
        let mut wanted = BTreeSet::new();
        let mut bytes = 0usize;
        for (layer, content) in layers.iter().enumerate() {
            if let Some(content) = content {
                if content.chunks.len() > MAX_TEXT_CHUNKS {
                    return Err("UI chunk limit exceeded".into());
                }
                for chunk in &content.chunks {
                    if !wanted.insert((layer, chunk.resource.version)) {
                        return Err("Duplicate UI resource identity".into());
                    }
                    bytes = bytes
                        .checked_add(
                            (chunk.resource.mesh.vertices.len()
                                + chunk.resource.mesh.glyph_vertices.len())
                                * 4,
                        )
                        .ok_or("UI resource size overflow")?;
                }
            }
        }
        if bytes > MAX_TEXT_RESOURCE_BYTES
            || wanted.len() > display.slots().saturating_sub(3) as usize
        {
            return Err("UI resource budget exceeded".into());
        }
        if self.epoch != Some(frame.presentation_epoch) {
            self.clear(display)?;
            self.epoch = Some(frame.presentation_epoch);
        }
        let retired: Vec<_> = self
            .entries
            .keys()
            .filter(|key| !wanted.contains(key))
            .copied()
            .collect();
        for key in retired {
            let entry = self
                .entries
                .remove(&key)
                .ok_or("Missing retired UI resource")?;
            display.retire(entry.slot)?;
        }
        let mut result = Update::default();
        for (layer, content) in layers.iter().enumerate() {
            let Some(content) = content else { continue };
            for chunk in &content.chunks {
                let key = (layer, chunk.resource.version);
                if let Some(entry) = self.entries.get(&key) {
                    if !Arc::ptr_eq(&entry.resource, &chunk.resource) {
                        return Err("UI resource version reused within one epoch".into());
                    }
                } else {
                    let slot = (3..display.slots())
                        .find(|slot| !self.entries.values().any(|entry| entry.slot == *slot))
                        .ok_or("UI resource slots exhausted")?;
                    let mesh = &chunk.resource.mesh;
                    display.upload(slot, &chunk.resource)?;
                    result.bytes += (mesh.vertices.len() + mesh.glyph_vertices.len()) * 4;
                    result.uploads[layer] += 1;
                    self.entries.insert(
                        key,
                        Entry {
                            slot,
                            resource: chunk.resource.clone(),
                        },
                    );
                }
                let slot = self
                    .entries
                    .get(&key)
                    .ok_or("Missing admitted UI resource")?
                    .slot;
                display.metadata(slot, &chunk.views, &chunk.paints)?;
                result.slots[layer].push(slot);
            }
        }
        Ok(result)
    }
}
