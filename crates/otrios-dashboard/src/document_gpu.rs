// SPDX-License-Identifier: Apache-2.0
//! Same-thread GPU resource ownership; wrapping and scheduling live in core.
use otrios_core::canvas::document::{
    Frame, GPU_BYTES_PER_GLYPH, GPU_GEOMETRY_BYTES, UPLOAD_BYTES_PER_FRAME,
};
use otrios_linux_display::{Display, MAX_SCENE_SLOTS};
use std::collections::{BTreeMap, BTreeSet};
struct Entry {
    slot: u32,
    bytes: usize,
    used: u64,
}
#[derive(Default)]
pub(super) struct Cache {
    entries: BTreeMap<u64, Entry>,
    epoch: Option<u64>,
    stamp: u64,
    pub uploaded_bytes: u64,
    pub uploads: u64,
    pub evictions: u64,
}
impl Cache {
    pub fn clear(&mut self, display: &mut Display) -> Result<(), String> {
        for entry in self.entries.values() {
            display.retire_slot(entry.slot)?;
        }
        self.entries.clear();
        self.epoch = None;
        Ok(())
    }
    /// The returned ordered slots reference only admitted versions. A cold
    /// frame can take several upload turns; its loading state remains explicit.
    pub fn update(
        &mut self,
        display: &mut Display,
        frame: Option<&Frame>,
    ) -> Result<(Vec<u32>, bool), String> {
        let Some(frame) = frame else {
            if !self.entries.is_empty() {
                self.clear(display)?;
            }
            return Ok((vec![], false));
        };
        if self.epoch != Some(frame.epoch) {
            self.clear(display)?;
            self.epoch = Some(frame.epoch);
        }
        self.stamp += 1;
        let wanted: BTreeSet<_> = frame.draws.iter().map(|d| d.chunk.version).collect();
        let required: usize = frame
            .draws
            .iter()
            .map(|d| d.chunk.clusters.len() * GPU_BYTES_PER_GLYPH + 12)
            .sum();
        if required > GPU_GEOMETRY_BYTES || wanted.len() > (MAX_SCENE_SLOTS - 3) as usize {
            return Err("Visible document exceeds GPU resource budget".into());
        }
        let mut upload_bytes = 0;
        let mut slots = Vec::with_capacity(frame.draws.len());
        let mut pending = false;
        for draw in &frame.draws {
            let id = draw.chunk.version;
            if !self.entries.contains_key(&id) {
                let glyphs = &draw.chunk.mesh.glyph_vertices;
                if upload_bytes + glyphs.len() * 4 > UPLOAD_BYTES_PER_FRAME {
                    pending = true;
                    continue;
                }
                let bytes = draw.chunk.clusters.len() * GPU_BYTES_PER_GLYPH + 12;
                while self.entries.values().map(|e| e.bytes).sum::<usize>() + bytes
                    > GPU_GEOMETRY_BYTES
                    || self.entries.len() == (MAX_SCENE_SLOTS - 3) as usize
                {
                    let retired = self
                        .entries
                        .iter()
                        .filter(|(id, _)| !wanted.contains(id))
                        .min_by_key(|(_, e)| e.used)
                        .map(|(id, e)| (*id, e.slot))
                        .ok_or("Pinned document GPU budget exhausted")?;
                    display.retire_slot(retired.1)?;
                    self.entries.remove(&retired.0);
                    self.evictions += 1;
                }
                let slot = (3..MAX_SCENE_SLOTS)
                    .find(|slot| !self.entries.values().any(|e| e.slot == *slot))
                    .ok_or("Document resource slots exhausted")?;
                display.upload_scene(slot, &[], glyphs, &draw.chunk.mesh.draws)?;
                display.text_styles(slot, &[frame.em])?;
                upload_bytes += glyphs.len() * 4;
                self.uploaded_bytes += (glyphs.len() * 4) as u64;
                self.uploads += 1;
                self.entries.insert(
                    id,
                    Entry {
                        slot,
                        bytes,
                        used: self.stamp,
                    },
                );
            }
            let entry = self
                .entries
                .get_mut(&id)
                .ok_or("Missing admitted document resource")?;
            entry.used = self.stamp;
            display.text_views(entry.slot, &draw.view)?;
            slots.push(entry.slot);
        }
        Ok((slots, pending))
    }
}
