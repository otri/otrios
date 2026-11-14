// SPDX-License-Identifier: LGPL-3.0-only
//! Retained UI primitives: immutable local geometry/glyphs and moving, clipped views.
use super::{
    font::Font,
    render::{Clip, Mesh, RetainedPaint},
    scene::Scene,
};
use otrios_interfaces::scene::{Primitive, SceneNode};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};
#[derive(Default)]
pub struct TextPresentation {
    key: Option<SceneNode>,
    font: String,
    mesh: Arc<Mesh>,
    runs: Vec<Run>,
    styles: Arc<Vec<f32>>,
    paints: Arc<Vec<f32>>,
    admitted: BTreeSet<u32>,
    primitives: BTreeMap<u32, Arc<CachedPrimitive>>,
    chunks: Vec<CachedChunk>,
    next_resource: u64,
    em_cache: Vec<(u32, f32)>,
    generation: u32,
}
#[derive(Clone)]
struct Run {
    draw: usize,
    id: u32,
    geometry: bool,
    right: Arc<Vec<f32>>,
    left: Arc<Vec<f32>>,
    top: f32,
    bottom: f32,
}
#[derive(Clone, Default)]
pub struct TextFrame {
    pub mesh: Arc<Mesh>,
    /// Independent immutable resources in painter order. Empty with nonempty
    /// `mesh` selects the bounded legacy fallback. The compatibility mesh above
    /// remains available to document/reflow hosts and exporters.
    pub chunks: Vec<TextChunk>,
    /// Eight floats per draw: visible first/count, translation xy, clip l/t/r/b.
    /// Zero geometry entries retain fixed UI placement; nonzero triangle ranges
    /// opt into the same translation/clip contract. No vertices change on scrolling.
    pub views: Vec<f32>,
    /// Canonical logical em per draw; zero requests legacy inference.
    pub styles: Arc<Vec<f32>>,
    /// Authored straight RGBA per draw, independent of geometry. An empty array
    /// selects the fixed renderer's legacy per-vertex colors.
    pub paints: Arc<Vec<f32>>,
    pub generation: u32,
}
/// One bounded immutable geometry resource. Versions are local to a presentation
/// owner; hosts namespace them by layer and reset epoch, never as content hashes.
pub struct TextResource {
    pub version: u64,
    pub mesh: Arc<Mesh>,
    pub styles: Arc<Vec<f32>>,
}
#[derive(Clone)]
pub struct TextChunk {
    pub resource: Arc<TextResource>,
    /// The same eight floats per draw, with resource-local vertex ranges.
    pub views: Vec<f32>,
    pub paints: Arc<Vec<f32>>,
}
/// Fixed consecutive grouping bounds host resources without changing paint order.
/// At most 512 scene nodes admit at most 32 resources per presentation layer.
const DRAWS_PER_CHUNK: usize = 16;
pub const MAX_TEXT_CHUNKS: usize = super::scene::MAX_NODES.div_ceil(DRAWS_PER_CHUNK);
/// Shared base/overlay host resource budget. Documents use their separate
/// existing budget and select the compatibility presentation path.
pub const MAX_TEXT_RESOURCE_BYTES: usize = 16 * 1024 * 1024;
struct CachedPrimitive {
    key: SceneNode,
    factor: f32,
    mesh: Arc<Mesh>,
    run: Option<Run>,
    style: f32,
}
struct CachedChunk {
    sources: Vec<Arc<CachedPrimitive>>,
    resource: Arc<TextResource>,
    paints: Arc<Vec<f32>>,
}

impl TextFrame {
    /// Materialize per-draw tint for legacy mesh exporters. Production hosts
    /// consume `paints` directly and retain the original immutable mesh.
    pub fn baked_mesh(&self) -> Mesh {
        let mut mesh = Mesh {
            vertices: self.mesh.vertices.clone(),
            glyph_vertices: self.mesh.glyph_vertices.clone(),
            draws: self.mesh.draws.clone(),
        };
        if self.paints.is_empty() {
            return mesh;
        }
        for (draw, range) in mesh.draws.chunks_exact(3).enumerate() {
            let color = &self.paints[draw * 4..draw * 4 + 4];
            let (vertices, stride, offset) = if range[0] == 0 {
                (&mut mesh.vertices, 6, 2)
            } else {
                (&mut mesh.glyph_vertices, 16, 4)
            };
            for vertex in range[1] as usize..(range[1] + range[2]) as usize {
                vertices[vertex * stride + offset..vertex * stride + offset + 4]
                    .copy_from_slice(color);
            }
        }
        mesh
    }
}
/// Opt-in probe timings. Ordinary composition reads no profiling clock.
#[derive(Default, Debug)]
pub struct ComposeProfile {
    pub placement_us: u128,
    pub rebuild_us: u128,
    pub cull_us: u128,
    pub mesh_payload_bytes: usize,
    pub viewport_bytes: usize,
}
impl TextPresentation {
    pub fn compose(&mut self, scene: &Scene, font: &Font) -> TextFrame {
        self.compose_inner(scene, font, None)
    }
    pub fn compose_profiled(
        &mut self,
        scene: &Scene,
        font: &Font,
        profile: &mut ComposeProfile,
    ) -> TextFrame {
        *profile = ComposeProfile::default();
        self.compose_inner(scene, font, Some(profile))
    }
    fn compose_inner(
        &mut self,
        scene: &Scene,
        font: &Font,
        mut profile: Option<&mut ComposeProfile>,
    ) -> TextFrame {
        #[cfg(feature = "canvas")]
        let _profile = crate::canvas::profiling::span("render.retained.compose");
        let start = profile.as_ref().map(|_| Instant::now());
        #[cfg(feature = "canvas")]
        let normalize_profile = crate::canvas::profiling::span("render.retained.normalize");
        let mut key = scene.root().clone();
        let mut positions = BTreeMap::new();
        let mut colors = BTreeMap::new();
        let mut admission = Admission::default();
        let extent = [scene.root().frame.width, scene.root().frame.height];
        let mut glyphs = 0;
        if self.font != font.hash() {
            self.em_cache.clear();
        }
        let mut scales = EmScales {
            font,
            cache: &mut self.em_cache,
        };
        normalize(
            &mut key,
            [0., 0.],
            1.,
            [0., 0., extent[0], extent[1]],
            [
                -OVERSCAN,
                -OVERSCAN,
                extent[0] + OVERSCAN,
                extent[1] + OVERSCAN,
            ],
            &mut Normalization {
                out: &mut positions,
                glyphs: &mut glyphs,
                scales: &mut scales,
                colors: &mut colors,
                admission: &mut admission,
            },
        );
        #[cfg(feature = "canvas")]
        drop(normalize_profile);
        if let Some(p) = profile.as_deref_mut() {
            p.placement_us = start.unwrap().elapsed().as_micros();
        }
        let rebuild_start = profile.as_ref().map(|_| Instant::now());
        #[cfg(feature = "canvas")]
        let rebuild_profile = crate::canvas::profiling::span("render.retained.rebuild");
        if self.key.as_ref() != Some(&key)
            || self.font != font.hash()
            || !admission.visible.is_subset(&self.admitted)
        {
            // Per-primitive resources survive unrelated label/status edits. A
            // compatibility mesh is assembled by copying only; tessellation and
            // glyph shaping run solely for changed/new admitted primitives.
            if self.font != font.hash() {
                self.primitives.clear();
            }
            let mut mesh = Mesh::default();
            let mut styles = Vec::new();
            self.runs.clear();
            let mut retain = glyphs <= 16384;
            if retain {
                #[cfg(feature = "canvas")]
                let _mesh_profile = crate::canvas::profiling::span("render.retained.mesh.paint");
                let mut ordered = PrimitiveCollection::default();
                retain = collect_primitives(
                    &key,
                    1.,
                    &admission.near,
                    font,
                    &positions,
                    &PrimitiveReuse {
                        previous: &self.primitives,
                        order: self
                            .chunks
                            .iter()
                            .flat_map(|chunk| chunk.sources.iter().map(|source| source.key.id))
                            .collect(),
                    },
                    &mut ordered,
                );
                let mut primitives = BTreeMap::new();
                let mut drawn = Vec::new();
                for primitive in ordered.entries {
                    primitives.insert(primitive.key.id, primitive.clone());
                    if let Some(mut run) = primitive.run.clone() {
                        run.draw = mesh.draws.len() / 3;
                        append_mesh(&mut mesh, &primitive.mesh);
                        self.runs.push(run);
                        styles.push(primitive.style);
                        drawn.push(primitive);
                    }
                }
                self.primitives = primitives;
                // These limits apply to the complete frame, not to each chunk.
                retain = retain
                    && mesh.vertices.len() / 6 <= 65536
                    && mesh.glyph_vertices.len() / 16 <= 65536;
                if retain {
                    self.rebuild_chunks(&drawn);
                }
            }
            if !retain {
                mesh = super::render_scene_with_font(scene, font);
                self.runs.clear();
                self.primitives.clear();
                self.chunks.clear();
                styles = vec![0.; mesh.draws.len() / 3];
            }
            self.styles = Arc::new(styles);
            self.admitted = if retain {
                admission.near
            } else {
                BTreeSet::new()
            };
            self.mesh = Arc::new(mesh);
            if let Some(p) = profile.as_deref_mut() {
                p.mesh_payload_bytes = (self.mesh.vertices.len()
                    + self.mesh.glyph_vertices.len()
                    + self.mesh.draws.len())
                    * 4;
            }
            self.font = font.hash().into();
            self.key = retain.then_some(key);
            self.generation = self.generation.wrapping_add(1);
        }
        let mut paints = vec![0.; self.mesh.draws.len() / 3 * 4];
        for run in &self.runs {
            if let Some(color) = colors.get(&run.id) {
                paints[run.draw * 4..run.draw * 4 + 4].copy_from_slice(color);
            }
        }
        if self.runs.is_empty() {
            paints.clear();
        }
        if self.paints.as_ref() != &paints {
            self.paints = Arc::new(paints);
        }
        #[cfg(feature = "canvas")]
        drop(rebuild_profile);
        if let Some(p) = profile.as_deref_mut() {
            p.rebuild_us = rebuild_start.unwrap().elapsed().as_micros();
        }
        let cull_start = profile.as_ref().map(|_| Instant::now());
        #[cfg(feature = "canvas")]
        let _cull_profile = crate::canvas::profiling::span("render.retained.cull");
        let mut views = vec![0.; self.mesh.draws.len() / 3 * 8];
        for run in &self.runs {
            let Some(&[x, y, l, t, r, b, _]) = positions.get(&run.id) else {
                continue;
            };
            let visible = r > l && b > t && run.bottom + y > t && run.top + y < b;
            if run.geometry {
                // A zero geometry range is the legacy fixed-placement sentinel.
                // Retain its admitted range and use an empty clip to hide it.
                let visible = visible && run.right[0] + x > l && run.left[0] + x < r;
                views[run.draw * 8..run.draw * 8 + 8].copy_from_slice(&[
                    self.mesh.draws[run.draw * 3 + 1] as f32,
                    self.mesh.draws[run.draw * 3 + 2] as f32,
                    x,
                    y,
                    l,
                    t,
                    if visible { r } else { l },
                    b,
                ]);
                continue;
            }
            let start = run.right.partition_point(|right| *right <= l - x);
            let end = run.left.partition_point(|left| *left < r - x);
            let count = if visible {
                end.saturating_sub(start) * 4
            } else {
                0
            };
            views[run.draw * 8..run.draw * 8 + 8].copy_from_slice(&[
                self.mesh.draws[run.draw * 3 + 1] as f32 + (start * 4) as f32,
                count as f32,
                x,
                y,
                l,
                t,
                r,
                b,
            ]);
        }
        if self.runs.is_empty() {
            views.clear();
        }
        let mut draw = 0;
        let chunks = self
            .chunks
            .iter_mut()
            .map(|chunk| {
                let count = chunk.resource.mesh.draws.len() / 3;
                let mut local_views = views[draw * 8..(draw + count) * 8].to_vec();
                for (index, local) in chunk.resource.mesh.draws.chunks_exact(3).enumerate() {
                    let global = &self.mesh.draws[(draw + index) * 3..(draw + index + 1) * 3];
                    local_views[index * 8] += local[1] as f32 - global[1] as f32;
                }
                let local_paints = &self.paints[draw * 4..(draw + count) * 4];
                if chunk.paints.as_ref() != local_paints {
                    chunk.paints = Arc::new(local_paints.to_vec());
                }
                draw += count;
                TextChunk {
                    resource: chunk.resource.clone(),
                    views: local_views,
                    paints: chunk.paints.clone(),
                }
            })
            .collect();
        if let Some(p) = profile {
            p.cull_us = cull_start.unwrap().elapsed().as_micros();
            p.viewport_bytes = views.len() * 4;
        }
        TextFrame {
            mesh: self.mesh.clone(),
            chunks,
            views,
            generation: self.generation,
            styles: self.styles.clone(),
            paints: self.paints.clone(),
        }
    }
}
impl TextPresentation {
    fn rebuild_chunks(&mut self, drawn: &[Arc<CachedPrimitive>]) {
        let previous = std::mem::take(&mut self.chunks);
        self.chunks = drawn
            .chunks(DRAWS_PER_CHUNK)
            .map(|sources| {
                if let Some(old) = previous.iter().find(|old| {
                    old.sources.len() == sources.len()
                        && old
                            .sources
                            .iter()
                            .zip(sources)
                            .all(|(a, b)| Arc::ptr_eq(&a.mesh, &b.mesh) && a.style == b.style)
                }) {
                    return CachedChunk {
                        sources: sources.to_vec(),
                        resource: old.resource.clone(),
                        paints: old.paints.clone(),
                    };
                }
                let mut mesh = Mesh::default();
                let mut styles = Vec::new();
                for primitive in sources {
                    append_mesh(&mut mesh, &primitive.mesh);
                    styles.push(primitive.style);
                }
                self.next_resource += 1;
                CachedChunk {
                    sources: sources.to_vec(),
                    paints: Arc::default(),
                    resource: Arc::new(TextResource {
                        version: self.next_resource,
                        mesh: Arc::new(mesh),
                        styles: Arc::new(styles),
                    }),
                }
            })
            .collect();
    }
}

fn append_mesh(target: &mut Mesh, source: &Mesh) {
    let geometry = (target.vertices.len() / 6) as u32;
    let glyphs = (target.glyph_vertices.len() / 16) as u32;
    target.vertices.extend_from_slice(&source.vertices);
    target
        .glyph_vertices
        .extend_from_slice(&source.glyph_vertices);
    for range in source.draws.chunks_exact(3) {
        target.draws.extend([
            range[0],
            range[1] + if range[0] == 0 { geometry } else { glyphs },
            range[2],
        ]);
    }
}

#[derive(Default)]
struct PrimitiveCollection {
    entries: Vec<Arc<CachedPrimitive>>,
    geometry: usize,
    glyphs: usize,
    drawn: usize,
}
struct PrimitiveReuse<'a> {
    previous: &'a BTreeMap<u32, Arc<CachedPrimitive>>,
    // One bounded fallback per current painter ordinal; no quadratic geometry
    // search when semantic controls are replaced with fresh input identities.
    order: Vec<u32>,
}
impl PrimitiveCollection {
    fn admit(&mut self, primitive: Arc<CachedPrimitive>) -> bool {
        self.geometry += primitive.mesh.vertices.len() / 6;
        self.glyphs += primitive.mesh.glyph_vertices.len() / 16;
        if self.geometry > 65536 || self.glyphs > 65536 {
            return false;
        }
        self.drawn += usize::from(primitive.run.is_some());
        self.entries.push(primitive);
        true
    }
}

fn collect_primitives(
    node: &SceneNode,
    factor: f32,
    admitted: &BTreeSet<u32>,
    font: &Font,
    positions: &BTreeMap<u32, [f32; 7]>,
    reuse: &PrimitiveReuse<'_>,
    ordered: &mut PrimitiveCollection,
) -> bool {
    if matches!(
        node.primitive,
        Primitive::Canvas { .. } | Primitive::Viewport { .. }
    ) {
        let factor = if let Primitive::Viewport { scale } = node.primitive {
            factor * scale
        } else {
            factor
        };
        for child in &node.children {
            if !collect_primitives(child, factor, admitted, font, positions, reuse, ordered) {
                return false;
            }
        }
        return true;
    }
    if !admitted.contains(&node.id) {
        return true;
    }
    let compatible = |old: &&Arc<CachedPrimitive>| {
        old.factor == factor && old.key.frame == node.frame && old.key.primitive == node.primitive
    };
    let old = reuse.previous.get(&node.id).filter(compatible).or_else(|| {
        reuse
            .order
            .get(ordered.drawn)
            .and_then(|id| reuse.previous.get(id))
            .filter(compatible)
    });
    if let Some(old) = old {
        if old.key == *node {
            return ordered.admit(old.clone());
        }
        // Geometry has no scene IDs. Bind the shared immutable bytes/bounds to
        // this frame's identity so hit tests, placement and paint stay current.
        let mut run = old.run.clone();
        if let Some(run) = &mut run {
            run.id = node.id;
        }
        return ordered.admit(Arc::new(CachedPrimitive {
            key: node.clone(),
            factor,
            mesh: old.mesh.clone(),
            run,
            style: old.style,
        }));
    }
    let mut mesh = Mesh::default();
    let mut runs = Vec::new();
    let mut retained = RetainedPaint {
        runs: &mut runs,
        admitted,
    };
    mesh.paint(
        node,
        [0., 0.],
        factor,
        Clip {
            left: -100_000.,
            top: -100_000.,
            right: 100_000.,
            bottom: 100_000.,
        },
        Some(font),
        Some(&mut retained),
    );
    let style = if matches!(node.primitive, Primitive::Text { .. }) {
        positions.get(&node.id).map_or(0., |p| p[6])
    } else {
        0.
    };
    let run = runs.first().map(|(_, id)| primitive_run(*id, &mesh));
    ordered.admit(Arc::new(CachedPrimitive {
        key: node.clone(),
        factor,
        mesh: Arc::new(mesh),
        run,
        style,
    }))
}

fn primitive_run(id: u32, mesh: &Mesh) -> Run {
    let range = &mesh.draws[..3];
    let first = range[1] as usize;
    let count = range[2] as usize;
    if range[0] == 0 {
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for vertex in mesh.vertices[first * 6..(first + count) * 6].chunks_exact(6) {
            bounds[0] = bounds[0].min(vertex[0]);
            bounds[1] = bounds[1].min(vertex[1]);
            bounds[2] = bounds[2].max(vertex[0]);
            bounds[3] = bounds[3].max(vertex[1]);
        }
        return Run {
            draw: 0,
            id,
            geometry: true,
            left: Arc::new(vec![bounds[0]]),
            right: Arc::new(vec![bounds[2]]),
            top: bounds[1],
            bottom: bounds[3],
        };
    }
    let mut right = Vec::new();
    let mut left = Vec::new();
    let mut top = f32::INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for quad in mesh.glyph_vertices[first * 16..(first + count) * 16].chunks_exact(64) {
        maximum = maximum.max(quad[16]);
        right.push(maximum);
        left.push(quad[0]);
        top = top.min(quad[1]);
        bottom = bottom.max(quad[33]);
    }
    let mut minimum = f32::INFINITY;
    for x in left.iter_mut().rev() {
        minimum = minimum.min(*x);
        *x = minimum;
    }
    Run {
        draw: 0,
        id,
        geometry: false,
        right: Arc::new(right),
        left: Arc::new(left),
        top,
        bottom,
    }
}

struct EmScales<'a> {
    font: &'a Font,
    cache: &'a mut Vec<(u32, f32)>,
}
#[derive(Default)]
struct Admission {
    visible: BTreeSet<u32>,
    near: BTreeSet<u32>,
}
/// A small scroll guard band keeps short pans on immutable local geometry. A
/// newly visible uncached primitive replaces this bounded working set; merely
/// moving a cached primitive offscreen never invalidates a captured frame.
const OVERSCAN: f32 = 64.;
fn primitive_bounds(node: &SceneNode, x: f32, y: f32, factor: f32, font: &Font) -> [f32; 4] {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut include = |l: f32, t: f32, r: f32, b: f32| {
        bounds[0] = bounds[0].min(l);
        bounds[1] = bounds[1].min(t);
        bounds[2] = bounds[2].max(r);
        bounds[3] = bounds[3].max(b);
    };
    match &node.primitive {
        Primitive::Text { value, scale, .. } => {
            let scale = *scale as f32 * factor;
            let unit = font.metrics().scale(scale);
            let baseline = y + font.metrics().baseline_from_top(scale);
            let mut pen = x;
            for glyph in font.text_glyphs(value) {
                if glyph.drawable() {
                    let b = glyph.ink_bounds;
                    include(
                        pen + b[0] * unit - 1.,
                        baseline - b[3] * unit - 1.,
                        pen + b[2] * unit + 1.,
                        baseline - b[1] * unit + 1.,
                    );
                }
                pen += glyph.advance * unit;
            }
        }
        Primitive::Triangles { points, .. } | Primitive::Path { points, .. } => {
            let margin = if let Primitive::Path { width, .. } = node.primitive {
                width * factor / 2.
            } else {
                0.
            };
            for p in points {
                let px = x + p[0] * factor;
                let py = y + p[1] * factor;
                include(px - margin, py - margin, px + margin, py + margin);
            }
        }
        _ => include(
            x,
            y,
            x + node.frame.width * factor,
            y + node.frame.height * factor,
        ),
    }
    bounds
}
impl EmScales<'_> {
    fn get(&mut self, scale: f32) -> f32 {
        let key = scale.to_bits();
        if let Some((_, em)) = self.cache.iter().find(|(s, _)| *s == key) {
            return *em;
        }
        if self.cache.len() == 8 {
            self.cache.clear();
        }
        let em = self.font.metrics().cache_em(scale);
        self.cache.push((key, em));
        em
    }
}
struct Normalization<'a, 'font> {
    out: &'a mut BTreeMap<u32, [f32; 7]>,
    glyphs: &'a mut usize,
    scales: &'a mut EmScales<'font>,
    colors: &'a mut BTreeMap<u32, [f32; 4]>,
    admission: &'a mut Admission,
}
fn normalize(
    node: &mut SceneNode,
    parent: [f32; 2],
    factor: f32,
    clip: [f32; 4],
    overscan_clip: [f32; 4],
    context: &mut Normalization<'_, '_>,
) {
    let f = node.frame;
    let x = parent[0] + f.x * factor;
    let y = parent[1] + f.y * factor;
    let mut clip = clip;
    let mut overscan_clip = overscan_clip;
    let mut factor = factor;
    if !matches!(
        node.primitive,
        Primitive::Canvas { .. } | Primitive::Viewport { .. }
    ) {
        let bounds = primitive_bounds(node, x, y, factor, context.scales.font);
        let intersects = |clip: [f32; 4]| {
            clip[2] > clip[0]
                && clip[3] > clip[1]
                && bounds[2] > clip[0]
                && bounds[0] < clip[2]
                && bounds[3] > clip[1]
                && bounds[1] < clip[3]
        };
        if intersects(clip) {
            context.admission.visible.insert(node.id);
        }
        if intersects(overscan_clip) {
            context.admission.near.insert(node.id);
            if let Primitive::Text { value, .. } = &node.primitive {
                *context.glyphs += value.chars().count();
            }
        }
        match &mut node.primitive {
            Primitive::Text { color, .. }
            | Primitive::Superellipse { color, .. }
            | Primitive::Circle { color }
            | Primitive::Triangles { color, .. }
            | Primitive::Path { color, .. } => {
                context
                    .colors
                    .insert(node.id, color.map(|v| f32::from(v) / 255.));
                *color = [255; 4];
            }
            _ => {}
        }
    }
    // Input clipping/semantics are rebuilt with the scene, independently from
    // painted content. Offscreen controls must not invalidate retained geometry.
    node.responder = None;
    node.semantics = None;
    match &node.primitive {
        Primitive::Text { scale, .. } => {
            context.out.insert(
                node.id,
                [
                    x,
                    y,
                    clip[0],
                    clip[1],
                    clip[2],
                    clip[3],
                    context.scales.get(*scale as f32 * factor),
                ],
            );
            node.frame = otrios_interfaces::scene::Rect::new(0., 0., 0., 0.);
        }
        Primitive::Canvas { clip: true } | Primitive::Viewport { .. } => {
            // Intersect expanded ancestor clips separately. Expanding an
            // already empty exact intersection cannot recover a nearby row
            // whose own clipped canvas lies just outside the panel.
            overscan_clip = [
                overscan_clip[0].max(x - OVERSCAN),
                overscan_clip[1].max(y - OVERSCAN),
                overscan_clip[2].min(x + f.width * factor + OVERSCAN),
                overscan_clip[3].min(y + f.height * factor + OVERSCAN),
            ];
            clip = [
                clip[0].max(x),
                clip[1].max(y),
                clip[2].min(x + f.width * factor),
                clip[3].min(y + f.height * factor),
            ];
            if let Primitive::Viewport { scale } = node.primitive {
                factor *= scale;
            }
        }
        _ => {}
    }
    if !matches!(
        node.primitive,
        Primitive::Canvas { .. } | Primitive::Viewport { .. } | Primitive::Text { .. }
    ) {
        context
            .out
            .insert(node.id, [x, y, clip[0], clip[1], clip[2], clip[3], 0.]);
    }
    // Placement and container clipping are carried by views. In particular,
    // clipping a partially visible button changes its semantic Canvas bounds,
    // without changing any painted content. Viewport scales remain in the key.
    node.frame.x = 0.;
    node.frame.y = 0.;
    if matches!(
        node.primitive,
        Primitive::Canvas { .. } | Primitive::Viewport { .. }
    ) {
        node.frame.width = 0.;
        node.frame.height = 0.;
    }
    for child in &mut node.children {
        normalize(child, [x, y], factor, clip, overscan_clip, context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::components::{WHITE, canvas, text};
    use otrios_interfaces::scene::Rect;
    fn scene(offset: f32, y: f32, value: &str) -> Scene {
        Scene::new(canvas(
            0,
            Rect::new(0., 0., 200., 100.),
            vec![canvas(
                1,
                Rect::new(10., 10., 40., 24.),
                vec![text(2, Rect::new(-offset, y, 1000., 24.), value, 2, WHITE)],
            )],
        ))
        .unwrap()
    }
    #[test]
    fn scrolling_retains_vertices_and_culls_glyph_ranges_and_whole_runs() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let a = renderer.compose(&scene(0., 0., text), &font);
        let original = a.mesh.glyph_vertices.clone();
        let b = renderer.compose(&scene(30.25, 0., text), &font);
        assert!(Arc::ptr_eq(&a.mesh, &b.mesh));
        assert!(Arc::ptr_eq(&a.styles, &b.styles));
        assert_eq!(a.generation, b.generation);
        assert_eq!(&b.views[2..8], &[-20.25, 10., 10., 10., 50., 34.]);
        assert!(b.views[0] > a.views[0]);
        assert!(b.views[1] > 0. && b.views[1] < b.mesh.glyph_vertices.len() as f32 / 16.);
        let off = renderer.compose(&scene(30.25, 100., text), &font);
        assert!(Arc::ptr_eq(&b.mesh, &off.mesh));
        assert_eq!(off.views[1], 0.);
        let changed = renderer.compose(&scene(30.25, 0., "New contents"), &font);
        assert!(!Arc::ptr_eq(&changed.mesh, &b.mesh));
        assert_eq!(
            a.mesh.glyph_vertices, original,
            "Held snapshots stay immutable after a new content generation"
        );
    }
    #[test]
    fn declared_font_scale_does_not_depend_on_glyph_placement_rounding() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        for scale in [0.1_f32, 0.33, 1., 1.25, 2., 4.] {
            let mut root = scene(30.25, 0., "ABC").root().clone();
            root.children[0].primitive = Primitive::Viewport { scale };
            let scene = Scene::new(root).unwrap();
            let a = renderer.compose(&scene, &font);
            let expected = 8. * 2. * f64::from(scale) * f64::from(font.metrics().units_per_em)
                / f64::from(font.metrics().cap_height);
            let actual = a.styles.iter().copied().find(|v| *v > 0.).unwrap();
            assert!((f64::from(actual) - expected).abs() < 0.00002);
        }
    }
    #[test]
    fn retained_geometry_invalidates_for_content_scale_and_topology() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&scene(0., 0., "ABC"), &font);
        let second = renderer.compose(&scene(0., 0., "ABC"), &font);
        assert!(Arc::ptr_eq(&first.mesh, &second.mesh));
        let scrolled = renderer.compose(&scene(1.25, 0., "ABC"), &font);
        assert!(Arc::ptr_eq(&first.mesh, &scrolled.mesh));
        let mut root = scene(1.25, 0., "ABC").root().clone();
        root.children[0].primitive = Primitive::Viewport { scale: 1.25 };
        let scaled = renderer.compose(&Scene::new(root.clone()).unwrap(), &font);
        assert!(!Arc::ptr_eq(&first.mesh, &scaled.mesh));
        let original = scaled.mesh.glyph_vertices.clone();
        root.children[0].children[0].primitive = Primitive::Text {
            value: "XYZ".into(),
            scale: 2,
            color: WHITE,
        };
        let changed = renderer.compose(&Scene::new(root.clone()).unwrap(), &font);
        assert!(!Arc::ptr_eq(&scaled.mesh, &changed.mesh));
        root.children[0].children[0].id = 7;
        let topology = renderer.compose(&Scene::new(root).unwrap(), &font);
        assert!(!Arc::ptr_eq(&changed.mesh, &topology.mesh));
        assert_eq!(
            scaled.mesh.glyph_vertices, original,
            "Held snapshots never change"
        );
    }

    #[test]
    fn moving_shapes_keep_local_vertices_and_clip_offscreen_runs() {
        use otrios_interfaces::scene::{Action, TouchResponder};
        let font = super::super::font::test_font();
        let make = |x: f32, size: f32, color: [u8; 4]| {
            let mut shape = super::super::components::node(
                2,
                Rect::new(x, 4., size, size),
                Primitive::Circle { color },
            );
            shape.responder = Some(TouchResponder::Activate(Action::AppEvent(1)));
            Scene::new(canvas(
                0,
                Rect::new(0., 0., 100., 60.),
                vec![canvas(1, Rect::new(10., 10., 40., 30.), vec![shape])],
            ))
            .unwrap()
        };
        let mut renderer = TextPresentation::default();
        let before = renderer.compose(&make(0., 20., WHITE), &font);
        let original = before.mesh.vertices.clone();
        let partial_scene = make(30., 20., WHITE);
        let partial = renderer.compose(&partial_scene, &font);
        assert!(Arc::ptr_eq(&before.mesh, &partial.mesh));
        assert_eq!(&partial.views[2..], &[40., 14., 10., 10., 50., 40.]);
        assert_eq!(partial.views[1] as u32, partial.mesh.draws[2]);
        assert!(
            partial_scene
                .hit_path([45., 20.])
                .iter()
                .any(|n| n.node.id == 2),
            "The visible part of a clipped control remains clickable"
        );
        assert!(
            !partial_scene
                .hit_path([55., 20.])
                .iter()
                .any(|n| n.node.id == 2),
            "The clipped part of the control remains excluded from input"
        );
        let hidden = renderer.compose(&make(50., 20., WHITE), &font);
        assert!(Arc::ptr_eq(&before.mesh, &hidden.mesh));
        assert!(
            hidden.views[1] > 0.,
            "Do not use the legacy fixed-placement sentinel"
        );
        assert_eq!(
            hidden.views[4], hidden.views[6],
            "Offscreen shapes have an empty clip"
        );
        let resized = renderer.compose(&make(30., 22., WHITE), &font);
        assert!(!Arc::ptr_eq(&before.mesh, &resized.mesh));
        let recolored = renderer.compose(&make(30., 22., [1, 2, 3, 255]), &font);
        assert!(Arc::ptr_eq(&resized.mesh, &recolored.mesh));
        assert!(Arc::ptr_eq(&resized.styles, &recolored.styles));
        assert_eq!(resized.generation, recolored.generation);
        assert_eq!(
            recolored.paints.as_ref(),
            &[1. / 255., 2. / 255., 3. / 255., 1.]
        );
        assert_eq!(
            resized.paints.as_ref(),
            &WHITE.map(|v| f32::from(v) / 255.),
            "Held paint snapshots stay immutable"
        );
        assert_eq!(before.mesh.vertices, original);
    }

    #[test]
    fn paint_only_updates_preserve_draw_order_and_match_fixed_color_bytes() {
        let font = super::super::font::test_font();
        let make = |color| {
            Scene::new(canvas(
                0,
                Rect::new(0., 0., 200., 100.),
                vec![
                    super::super::components::node(
                        1,
                        Rect::new(20., 20., 20., 20.),
                        Primitive::Circle { color },
                    ),
                    text(2, Rect::new(50., 20., 100., 24.), "ABC", 2, color),
                    super::super::components::node(
                        3,
                        Rect::new(160., 20., 20., 20.),
                        Primitive::Superellipse {
                            radius: 5.,
                            exponent: 4.,
                            color,
                        },
                    ),
                ],
            ))
            .unwrap()
        };
        let mut renderer = TextPresentation::default();
        let old = renderer.compose(&make([255; 4]), &font);
        let color = [17, 83, 192, 127];
        let scene = make(color);
        let new = renderer.compose(&scene, &font);
        let baked = new.baked_mesh();
        let fixed = super::super::render_scene_with_font(&scene, &font);
        assert!(Arc::ptr_eq(&old.mesh, &new.mesh));
        assert!(!Arc::ptr_eq(&old.paints, &new.paints));
        assert_eq!(old.views, new.views);
        assert_eq!(
            new.mesh.draws, fixed.draws,
            "Painter order and ranges stay identical"
        );
        for (draw, range) in new.mesh.draws.chunks_exact(3).enumerate() {
            let (retained, expected, stride, offset) = if range[0] == 0 {
                (&baked.vertices, &fixed.vertices, 6, 2)
            } else {
                (&baked.glyph_vertices, &fixed.glyph_vertices, 16, 4)
            };
            for vertex in range[1] as usize..(range[1] + range[2]) as usize {
                let mut actual = retained[vertex * stride..(vertex + 1) * stride].to_vec();
                actual[0] += new.views[draw * 8 + 2];
                actual[1] += new.views[draw * 8 + 3];
                assert_eq!(
                    &actual[offset..offset + 4],
                    &expected[vertex * stride + offset..vertex * stride + offset + 4],
                    "Paint substitution reproduces fixed RGBA bytes exactly"
                );
                for (actual, expected) in actual
                    .iter()
                    .zip(&expected[vertex * stride..(vertex + 1) * stride])
                {
                    assert!(
                        (actual - expected).abs() < 0.00004,
                        "Style substitution reproduces the fixed vertex payload: {actual} != {expected}"
                    );
                }
            }
        }
        let identical = renderer.compose(&scene, &font);
        assert!(Arc::ptr_eq(&new.paints, &identical.paints));
        assert_eq!(old.paints.as_ref(), &[1.; 12]);
    }

    #[test]
    fn offscreen_geometry_uses_bounded_overscan_and_small_pans_reuse_it() {
        let font = super::super::font::test_font();
        let make = |offset| {
            Scene::new(canvas(
                0,
                Rect::new(0., 0., 200., 100.),
                (1..=127)
                    .map(|id| {
                        super::super::components::node(
                            id,
                            Rect::new(10., (id - 1) as f32 * 20. - offset, 20., 20.),
                            Primitive::Superellipse {
                                radius: 5.,
                                exponent: 4.,
                                color: WHITE,
                            },
                        )
                    })
                    .collect(),
            ))
            .unwrap()
        };
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&make(0.), &font);
        assert_eq!(first.mesh.vertices.len() / 6, 9 * 156);
        assert_eq!(renderer.runs.len(), 9);
        let short_pan = renderer.compose(&make(10.), &font);
        assert!(Arc::ptr_eq(&first.mesh, &short_pan.mesh));
        let long_pan = renderer.compose(&make(180.), &font);
        assert!(!Arc::ptr_eq(&first.mesh, &long_pan.mesh));
        assert!(renderer.runs.len() <= 13);
        for id in 10..=14 {
            assert!(
                renderer.runs.iter().any(|r| r.id == id),
                "Visible shape {id} is admitted"
            );
        }
        assert!(first.mesh.vertices.len() < 127 * 156 * 6);
        assert_eq!(
            first.mesh.vertices.len() / 6,
            9 * 156,
            "Held old geometry is unchanged"
        );
    }

    #[test]
    fn overscan_survives_an_empty_exact_clip_on_a_nearby_row_canvas() {
        let font = super::super::font::test_font();
        let make = |offset: f32| {
            Scene::new(canvas(
                0,
                Rect::new(0., 0., 100., 100.),
                vec![canvas(
                    1,
                    Rect::new(10., 120. - offset, 40., 20.),
                    vec![super::super::components::node(
                        2,
                        Rect::new(0., 0., 20., 20.),
                        Primitive::Circle { color: WHITE },
                    )],
                )],
            ))
            .unwrap()
        };
        let mut renderer = TextPresentation::default();
        let before = renderer.compose(&make(0.), &font);
        assert_eq!(
            before.mesh.draws.len(),
            3,
            "Near row geometry is prepared before its exact clip becomes visible"
        );
        assert_eq!(before.views[4], before.views[6]);
        let visible = renderer.compose(&make(30.), &font);
        assert!(Arc::ptr_eq(&before.mesh, &visible.mesh));
        assert!(visible.views[6] > visible.views[4]);
        assert!(visible.views[7] > visible.views[5]);
    }

    #[test]
    fn culling_uses_projected_points_and_ink_instead_of_logical_frames() {
        let font = super::super::font::test_font();
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., 100., 100.),
            vec![
                super::super::components::node(
                    1,
                    Rect::new(300., 10., 1., 1.),
                    Primitive::Triangles {
                        points: vec![[-290., 0.], [-280., 0.], [-290., 10.]],
                        color: WHITE,
                    },
                ),
                text(2, Rect::new(90., 20., 1., 8.), "ABCDEFGHIJK", 2, WHITE),
            ],
        ))
        .unwrap();
        let mut renderer = TextPresentation::default();
        let frame = renderer.compose(&scene, &font);
        assert_eq!(frame.mesh.draws.len(), 6);
        assert!(renderer.runs.iter().any(|r| r.id == 1));
        assert!(renderer.runs.iter().any(|r| r.id == 2));
    }

    fn chunk_scene(changed: bool, shift: f32, tint: [u8; 4]) -> Scene {
        Scene::new(canvas(
            0,
            Rect::new(0., 0., 500., 300.),
            (1..=40)
                .map(|id| {
                    let frame = Rect::new(
                        ((id - 1) % 8) as f32 * 55. - shift,
                        ((id - 1) / 8) as f32 * 45.,
                        30.,
                        30.,
                    );
                    if id % 2 == 0 {
                        super::super::components::node(
                            id,
                            frame,
                            Primitive::Superellipse {
                                radius: 4.,
                                exponent: 4.,
                                color: tint,
                            },
                        )
                    } else {
                        text(
                            id,
                            frame,
                            if changed && id == 19 {
                                "Changed"
                            } else {
                                "ABC"
                            },
                            1,
                            tint,
                        )
                    }
                })
                .collect(),
        ))
        .unwrap()
    }
    fn assert_chunk_projection(frame: &TextFrame) {
        let mut mesh = Mesh::default();
        let mut styles = Vec::new();
        let mut paints = Vec::new();
        let mut draw = 0;
        for chunk in &frame.chunks {
            assert!(chunk.resource.mesh.draws.len() / 3 <= DRAWS_PER_CHUNK);
            let geometry = mesh.vertices.len() / 6;
            let glyphs = mesh.glyph_vertices.len() / 16;
            for (index, range) in chunk.resource.mesh.draws.chunks_exact(3).enumerate() {
                let mut view = chunk.views[index * 8..(index + 1) * 8].to_vec();
                view[0] += if range[0] == 0 { geometry } else { glyphs } as f32;
                assert_eq!(view, frame.views[draw * 8..(draw + 1) * 8]);
                draw += 1;
            }
            append_mesh(&mut mesh, &chunk.resource.mesh);
            styles.extend_from_slice(&chunk.resource.styles);
            paints.extend_from_slice(&chunk.paints);
        }
        assert_eq!(mesh.vertices, frame.mesh.vertices);
        assert_eq!(mesh.glyph_vertices, frame.mesh.glyph_vertices);
        assert_eq!(
            mesh.draws, frame.mesh.draws,
            "Chunk boundaries preserve exact painter order"
        );
        assert_eq!(styles, *frame.styles);
        assert_eq!(paints, *frame.paints);
        assert_eq!(draw, frame.mesh.draws.len() / 3);
    }
    #[test]
    fn one_label_change_reuses_other_primitive_geometry_and_chunk_resources() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&chunk_scene(false, 0., WHITE), &font);
        assert_eq!(first.chunks.len(), 3);
        let old_primitive = renderer.primitives[&2].clone();
        let old_label = renderer.primitives[&19].clone();
        let old_vertices = first.mesh.vertices.clone();
        let old_glyphs = first.mesh.glyph_vertices.clone();
        let changed = renderer.compose(&chunk_scene(true, 0., WHITE), &font);
        assert!(Arc::ptr_eq(&old_primitive, &renderer.primitives[&2]));
        assert!(!Arc::ptr_eq(&old_label, &renderer.primitives[&19]));
        assert!(Arc::ptr_eq(
            &first.chunks[0].resource,
            &changed.chunks[0].resource
        ));
        assert!(!Arc::ptr_eq(
            &first.chunks[1].resource,
            &changed.chunks[1].resource
        ));
        assert!(Arc::ptr_eq(
            &first.chunks[2].resource,
            &changed.chunks[2].resource
        ));
        assert!(
            changed.chunks[1].resource.version
                > first
                    .chunks
                    .iter()
                    .map(|c| c.resource.version)
                    .max()
                    .unwrap()
        );
        let cold = TextPresentation::default().compose(&chunk_scene(true, 0., WHITE), &font);
        assert_eq!(changed.mesh.vertices, cold.mesh.vertices);
        assert_eq!(changed.mesh.glyph_vertices, cold.mesh.glyph_vertices);
        assert_eq!(changed.views, cold.views);
        assert_eq!(first.mesh.vertices, old_vertices);
        assert_eq!(first.mesh.glyph_vertices, old_glyphs);
        assert_chunk_projection(&first);
        assert_chunk_projection(&changed);
    }
    #[test]
    fn chunk_draw_ranges_and_paints_survive_mixed_pans_and_recolors() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&chunk_scene(false, 0., WHITE), &font);
        let pan = renderer.compose(&chunk_scene(false, 4.25, WHITE), &font);
        let tint = [37, 111, 193, 129];
        let recolored = renderer.compose(&chunk_scene(false, 4.25, tint), &font);
        for ((before, pan), paint) in first.chunks.iter().zip(&pan.chunks).zip(&recolored.chunks) {
            assert!(Arc::ptr_eq(&before.resource, &pan.resource));
            assert!(Arc::ptr_eq(&pan.resource, &paint.resource));
            assert!(Arc::ptr_eq(&before.paints, &pan.paints));
            assert!(!Arc::ptr_eq(&pan.paints, &paint.paints));
            assert_eq!(pan.views, paint.views);
        }
        assert_chunk_projection(&pan);
        assert_chunk_projection(&recolored);
        assert_eq!(
            first.paints.as_ref(),
            &WHITE.map(|value| f32::from(value) / 255.).repeat(40),
            "Held per-draw paints stay immutable"
        );
    }
    #[test]
    fn fresh_input_ids_rebind_existing_geometry_and_current_paint_placement() {
        use otrios_interfaces::scene::{Action, SwitchSemantics, TouchResponder};
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&chunk_scene(false, 0., WHITE), &font);
        let old_mesh = renderer.primitives[&2].mesh.clone();
        let held_views = first.views.clone();
        let mut root = chunk_scene(false, 4.25, [17, 91, 205, 140]).root().clone();
        for child in &mut root.children {
            child.id += 1000;
        }
        root.children[0].responder = Some(TouchResponder::Activate(Action::AppEvent(77)));
        root.children[0].semantics = Some(SwitchSemantics {
            label: "Replacement control".into(),
            checked: true,
        });
        let scene = Scene::new(root).unwrap();
        let after = renderer.compose(&scene, &font);
        assert!(Arc::ptr_eq(&old_mesh, &renderer.primitives[&1002].mesh));
        assert_eq!(renderer.primitives[&1002].run.as_ref().unwrap().id, 1002);
        for (old, new) in first.chunks.iter().zip(&after.chunks) {
            assert!(Arc::ptr_eq(&old.resource, &new.resource));
        }
        assert_eq!(scene.hit_path([5., 15.]).last().unwrap().node.id, 1001);
        let cold = TextPresentation::default().compose(&scene, &font);
        assert_eq!(after.mesh.vertices, cold.mesh.vertices);
        assert_eq!(after.mesh.glyph_vertices, cold.mesh.glyph_vertices);
        assert_eq!(after.views, cold.views);
        assert_eq!(after.paints, cold.paints);
        assert_eq!(first.views, held_views);
        assert_chunk_projection(&after);
    }
    #[test]
    fn id_rebinding_does_not_reuse_different_geometry() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&chunk_scene(false, 0., WHITE), &font);
        let old = renderer.primitives[&2].mesh.clone();
        let mut root = chunk_scene(false, 0., WHITE).root().clone();
        for child in &mut root.children {
            child.id += 1000;
        }
        root.children[1].frame.width = 28.;
        let scene = Scene::new(root).unwrap();
        let after = renderer.compose(&scene, &font);
        assert!(!Arc::ptr_eq(&old, &renderer.primitives[&1002].mesh));
        assert!(!Arc::ptr_eq(
            &first.chunks[0].resource,
            &after.chunks[0].resource
        ));
        for (old, new) in first.chunks.iter().zip(&after.chunks).skip(1) {
            assert!(Arc::ptr_eq(&old.resource, &new.resource));
        }
        let cold = TextPresentation::default().compose(&scene, &font);
        assert_eq!(after.mesh.vertices, cold.mesh.vertices);
        assert_eq!(after.mesh.glyph_vertices, cold.mesh.glyph_vertices);
        assert_eq!(after.views, cold.views);
        assert_chunk_projection(&after);
    }
    #[test]
    fn font_cache_reset_replaces_resources_without_reusing_live_versions() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&chunk_scene(false, 0., WHITE), &font);
        // Exercise replacement of a previously admitted font identity; geometry
        // bytes happen to match, but UV/band ownership must still be replaced.
        renderer.font = "retired-font-resource".into();
        let replacement = renderer.compose(&chunk_scene(false, 0., WHITE), &font);
        for (old, new) in first.chunks.iter().zip(&replacement.chunks) {
            assert!(!Arc::ptr_eq(&old.resource, &new.resource));
            assert!(
                new.resource.version
                    > first
                        .chunks
                        .iter()
                        .map(|c| c.resource.version)
                        .max()
                        .unwrap()
            );
        }
        assert_eq!(first.mesh.glyph_vertices, replacement.mesh.glyph_vertices);
        assert_chunk_projection(&replacement);
    }

    #[test]
    fn oversized_geometry_keeps_the_bounded_legacy_view_contract() {
        let font = super::super::font::test_font();
        let make = |x| {
            let shapes = (1..=430)
                .map(|id| {
                    super::super::components::node(
                        id,
                        Rect::new(x, 10., 20., 20.),
                        Primitive::Superellipse {
                            radius: 5.,
                            exponent: 4.,
                            color: WHITE,
                        },
                    )
                })
                .collect();
            Scene::new(canvas(0, Rect::new(0., 0., 200., 100.), shapes)).unwrap()
        };
        let mut renderer = TextPresentation::default();
        let first = renderer.compose(&make(0.), &font);
        assert!(first.mesh.vertices.len() / 6 > 65536);
        assert!(first.views.is_empty());
        assert!(
            first.chunks.is_empty(),
            "Aggregate geometry admission does not multiply per chunk"
        );
        assert!(first.paints.is_empty());
        assert_eq!(first.baked_mesh().vertices, first.mesh.vertices);
        let moved = renderer.compose(&make(12.), &font);
        assert!(!Arc::ptr_eq(&first.mesh, &moved.mesh));
        assert!(moved.views.is_empty());
        assert_eq!(moved.mesh.vertices[0] - first.mesh.vertices[0], 12.);
    }
}
