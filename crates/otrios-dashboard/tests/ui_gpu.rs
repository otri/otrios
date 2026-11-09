// SPDX-License-Identifier: Apache-2.0
//! Host resource ownership checks. No GPU or runtime implementation is substituted.
#[path = "../src/ui_gpu.rs"]
mod ui_gpu;
use otrios_core::{
    canvas::presentation::Frame,
    dashboard::{
        Mesh,
        retained_text::{TextChunk, TextFrame, TextResource},
    },
};
use std::sync::Arc;
#[derive(Default)]
struct Graphics {
    uploads: Vec<u32>,
    retired: Vec<u32>,
    metadata: Vec<u32>,
}
impl ui_gpu::Backend for Graphics {
    fn slots(&self) -> u32 {
        131
    }
    fn retire(&mut self, slot: u32) -> Result<(), String> {
        self.retired.push(slot);
        Ok(())
    }
    fn upload(&mut self, slot: u32, resource: &TextResource) -> Result<(), String> {
        assert!(!resource.mesh.draws.is_empty());
        self.uploads.push(slot);
        Ok(())
    }
    fn metadata(&mut self, slot: u32, _: &[f32], _: &[f32]) -> Result<(), String> {
        self.metadata.push(slot);
        Ok(())
    }
}
fn resource(version: u64) -> Arc<TextResource> {
    Arc::new(TextResource {
        version,
        mesh: Arc::new(Mesh {
            vertices: vec![0.; 18],
            glyph_vertices: vec![],
            draws: vec![0, 0, 3],
        }),
        styles: Arc::new(vec![0.]),
    })
}
fn chunk(resource: Arc<TextResource>) -> TextChunk {
    TextChunk {
        resource,
        views: vec![0., 3., 0., 0., 0., 0., 100., 100.],
        paints: Arc::new(vec![1.; 4]),
    }
}
fn frame(resources: Vec<Arc<TextResource>>) -> Frame {
    Frame {
        presentation_epoch: 1,
        base: TextFrame {
            chunks: resources.into_iter().map(chunk).collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}
#[test]
fn changed_label_uploads_one_resource_and_keeps_neighbor_order_and_buffers() {
    let a = resource(1);
    let b = resource(2);
    let mut frame = frame(vec![a.clone(), b.clone()]);
    let mut gpu = Graphics::default();
    let mut cache = ui_gpu::Cache::default();
    let first = cache.update(&mut gpu, &frame).unwrap();
    assert_eq!(first.uploads, [2, 0]);
    assert_eq!(first.bytes, 144);
    gpu.uploads.clear();
    frame.base.chunks[0].views[3] = 20.;
    frame.base.chunks[1].paints = Arc::new(vec![0.5; 4]);
    let pan = cache.update(&mut gpu, &frame).unwrap();
    assert_eq!(pan.slots, first.slots);
    assert_eq!(pan.bytes, 0);
    assert!(gpu.uploads.is_empty());
    frame.base.chunks[1] = chunk(resource(3));
    let edit = cache.update(&mut gpu, &frame).unwrap();
    assert_eq!(edit.uploads, [1, 0]);
    assert_eq!(edit.bytes, 72);
    assert_eq!(edit.slots[0][0], first.slots[0][0]);
    assert_eq!(gpu.retired, vec![first.slots[0][1]]);
    frame.base.chunks.reverse();
    let reordered = cache.update(&mut gpu, &frame).unwrap();
    assert_eq!(reordered.bytes, 0);
    assert_eq!(
        reordered.slots[0],
        edit.slots[0].iter().rev().copied().collect::<Vec<_>>()
    );
}
#[test]
fn layer_namespace_epoch_and_clear_retire_previous_ownership() {
    let a = resource(1);
    let mut frame = frame(vec![a.clone()]);
    frame.overlay = Some(TextFrame {
        chunks: vec![chunk(resource(1))],
        ..Default::default()
    });
    let mut gpu = Graphics::default();
    let mut cache = ui_gpu::Cache::default();
    let first = cache.update(&mut gpu, &frame).unwrap();
    assert_ne!(first.slots[0], first.slots[1]);
    assert_eq!(first.uploads, [1, 1]);
    frame.base.chunks[0] = chunk(resource(1));
    assert!(
        cache
            .update(&mut gpu, &frame)
            .unwrap_err()
            .contains("version reused")
    );
    frame.presentation_epoch = 2;
    let reset = cache.update(&mut gpu, &frame).unwrap();
    assert_eq!(reset.uploads, [1, 1]);
    assert_eq!(gpu.retired.len(), 2);
    cache.clear(&mut gpu).unwrap();
    assert_eq!(gpu.retired.len(), 4);
    let restart = cache.update(&mut gpu, &frame).unwrap();
    assert_eq!(restart.uploads, [1, 1]);
}
#[test]
fn duplicate_and_oversized_resource_lists_fail_before_gpu_mutation() {
    let mut gpu = Graphics::default();
    let mut cache = ui_gpu::Cache::default();
    let a = resource(1);
    let bad = frame(vec![a.clone(), a]);
    assert!(cache.update(&mut gpu, &bad).is_err());
    let large = frame((0..33).map(resource).collect());
    assert!(cache.update(&mut gpu, &large).is_err());
    assert!(gpu.uploads.is_empty());
    assert!(gpu.retired.is_empty());
}
