// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded static glTF 2 binary resources, shared by native GLES2 and WebGL1.
//! This closed subset has embedded JPEG base color, opaque materials, indexed
//! triangles and static node transforms. It never fetches or executes asset data.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};

pub const MAX_GLB_BYTES: usize = 1_048_576;
pub const MAX_TRIANGLES: usize = 12_000;
pub const MAX_BATCHES: usize = 16;
pub const MAX_TEXTURES: usize = 4;
pub const MAX_TEXTURE_EDGE: usize = 1024;
pub const VERTEX_FLOATS: usize = 12;
pub type Matrix = [f32; 16];
pub const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
type Result<T> = std::result::Result<T, String>;

#[derive(Debug)]
pub struct Texture {
    pub id: [u8; 32],
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub repeat: [bool; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}
#[derive(Debug)]
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub texture: Arc<Texture>,
    pub color: [f32; 4],
    pub double_sided: bool,
}
/// Immutable decoded resource. Node transforms are applied once during loading;
/// shared materials keep one decoded texture, including across mesh instances.
#[derive(Debug)]
pub struct Model {
    primitives: Vec<Primitive>,
    bounds: [[f32; 3]; 2],
}
pub struct Instance<'a> {
    pub model: &'a Model,
    pub pose: Matrix,
    pub opacity: f32,
}
#[derive(Clone, Copy)]
pub struct Camera {
    pub center: [f32; 2],
    pub pixels_per_unit: f32,
    pub distance: f32,
    pub near: f32,
    pub far: f32,
}
#[derive(Clone)]
pub struct Batch {
    pub first: u32,
    pub count: u32,
    pub texture: Arc<Texture>,
    pub double_sided: bool,
}
#[derive(Clone, Default)]
pub struct Frame {
    /// Per vertex: homogeneous logical x/y/w, UV, camera normal, straight RGBA.
    /// Host shaders multiply the x/y/w vector by the ordinary UI projection;
    /// the retained w gives perspective-correct UV and normal interpolation.
    pub vertices: Vec<f32>,
    pub batches: Vec<Batch>,
    /// A and B in clip_z=A*w+B, for GLES2/WebGL's [-w,w] depth range.
    pub depth: [f32; 2],
}
impl Frame {
    pub fn same_content(&self, other: &Self) -> bool {
        self.vertices == other.vertices
            && self.depth == other.depth
            && self.batches.len() == other.batches.len()
            && self.batches.iter().zip(&other.batches).all(|(a, b)| {
                a.first == b.first
                    && a.count == b.count
                    && a.double_sided == b.double_sided
                    && a.texture.id == b.texture.id
            })
    }
    pub fn project(instances: &[Instance<'_>], camera: Camera) -> Result<Self> {
        let numbers = [
            camera.center[0],
            camera.center[1],
            camera.pixels_per_unit,
            camera.distance,
            camera.near,
            camera.far,
        ];
        if numbers.iter().any(|n| !n.is_finite() || n.abs() > 100_000.)
            || camera.pixels_per_unit <= 0.
            || camera.near <= 0.
            || camera.distance <= camera.near
            || camera.far <= camera.distance
        {
            return Err("Invalid model camera".into());
        }
        let mut frame = Self {
            depth: [
                (camera.far + camera.near) / (camera.far - camera.near),
                -2. * camera.far * camera.near / (camera.far - camera.near),
            ],
            ..Self::default()
        };
        for instance in instances {
            if !instance.opacity.is_finite() || !(0. ..=1.).contains(&instance.opacity) {
                return Err("Invalid model opacity".into());
            }
            validate_matrix(&instance.pose)?;
            let normal = normal_matrix(&instance.pose)?;
            for primitive in &instance.model.primitives {
                if frame.batches.len() >= MAX_BATCHES
                    || frame.vertices.len() / VERTEX_FLOATS + primitive.indices.len()
                        > MAX_TRIANGLES * 3
                {
                    return Err("Model frame budget".into());
                }
                let first = (frame.vertices.len() / VERTEX_FLOATS) as u32;
                let transformed: Vec<_> = primitive
                    .vertices
                    .iter()
                    .map(|v| {
                        (
                            point(&instance.pose, v.position),
                            unit(vector(&normal, v.normal)),
                        )
                    })
                    .collect();
                let mut order = primitive.indices.clone();
                if determinant(&instance.pose) < 0. {
                    for tri in order.chunks_exact_mut(3) {
                        tri.swap(1, 2);
                    }
                }
                for index in &order {
                    let v = &primitive.vertices[usize::from(*index)];
                    let (p, n) = transformed[usize::from(*index)];
                    let w = camera.distance - p[2];
                    if w < camera.near * 0.1 {
                        return Err("Model crosses camera".into());
                    }
                    let x = camera.center[0] * w + p[0] * camera.pixels_per_unit * camera.distance;
                    let y = camera.center[1] * w - p[1] * camera.pixels_per_unit * camera.distance;
                    if [x, y, w]
                        .iter()
                        .any(|n| !n.is_finite() || n.abs() > 100_000.)
                    {
                        return Err("Model projection bounds".into());
                    }
                    frame.vertices.extend([
                        x,
                        y,
                        w,
                        v.uv[0],
                        v.uv[1],
                        n[0],
                        n[1],
                        n[2],
                        primitive.color[0],
                        primitive.color[1],
                        primitive.color[2],
                        primitive.color[3] * instance.opacity,
                    ]);
                }
                if !frame
                    .batches
                    .iter()
                    .any(|b| b.texture.id == primitive.texture.id)
                    && frame
                        .batches
                        .iter()
                        .map(|b| b.texture.id)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        >= MAX_TEXTURES
                {
                    return Err("Model frame texture budget".into());
                }
                frame.batches.push(Batch {
                    first,
                    count: primitive.indices.len() as u32,
                    texture: primitive.texture.clone(),
                    double_sided: primitive.double_sided,
                });
            }
        }
        Ok(frame)
    }
}
fn number(v: &Value) -> Result<usize> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "Expected GLB unsigned integer".into())
}
fn field(v: &Value, name: &str) -> Result<usize> {
    number(&v[name])
}
fn offset(v: &Value, name: &str) -> Result<usize> {
    v.get(name).map_or(Ok(0), number)
}
fn array<'a>(v: &'a Value, name: &str, max: usize) -> Result<&'a [Value]> {
    let a = v[name]
        .as_array()
        .ok_or_else(|| format!("Missing GLB {name}"))?;
    if a.len() > max {
        return Err(format!("GLB {name} budget"));
    }
    Ok(a)
}
fn floats<const N: usize>(v: &Value, default: [f32; N]) -> Result<[f32; N]> {
    if v.is_null() {
        return Ok(default);
    }
    let a = v
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or("GLB float vector dimensions")?;
    let mut out = default;
    for (out, v) in out.iter_mut().zip(a) {
        *out = v.as_f64().ok_or("GLB numeric vector")? as f32;
        if !out.is_finite() || out.abs() > 100_000. {
            return Err("GLB nonfinite/bounded number".into());
        }
    }
    Ok(out)
}
fn forbidden(v: &Value, depth: usize) -> bool {
    if depth > 32 {
        return true;
    }
    match v {
        Value::Object(o) => o.iter().any(|(k, v)| {
            matches!(
                k.as_str(),
                "uri"
                    | "extensions"
                    | "skin"
                    | "skins"
                    | "animations"
                    | "targets"
                    | "sparse"
                    | "weights"
                    | "normalTexture"
                    | "occlusionTexture"
                    | "emissiveTexture"
                    | "metallicRoughnessTexture"
            ) || forbidden(v, depth + 1)
        }),
        Value::Array(a) => a.iter().any(|v| forbidden(v, depth + 1)),
        _ => false,
    }
}
fn word(bytes: &[u8], at: usize) -> Result<usize> {
    let b: [u8; 4] = bytes
        .get(at..at.checked_add(4).ok_or("GLB offset")?)
        .ok_or("GLB truncated word")?
        .try_into()
        .map_err(|_| "GLB word")?;
    Ok(u32::from_le_bytes(b) as usize)
}
fn chunks(bytes: &[u8]) -> Result<(Value, &[u8])> {
    if bytes.len() > MAX_GLB_BYTES
        || bytes.get(..4) != Some(b"glTF")
        || word(bytes, 4)? != 2
        || word(bytes, 8)? != bytes.len()
    {
        return Err("GLB header/version/size".into());
    }
    let json_len = word(bytes, 12)?;
    if json_len > 131_072 || json_len % 4 != 0 || word(bytes, 16)? != 0x4e4f534a {
        return Err("GLB JSON chunk".into());
    }
    let bin_at = 20usize.checked_add(json_len).ok_or("GLB JSON size")?;
    let bin_len = word(bytes, bin_at)?;
    if bin_len % 4 != 0
        || word(bytes, bin_at + 4)? != 0x004e4942
        || bin_at.checked_add(8).and_then(|n| n.checked_add(bin_len)) != Some(bytes.len())
    {
        return Err("GLB BIN chunk".into());
    }
    let doc: Value = serde_json::from_slice(bytes.get(20..bin_at).ok_or("GLB JSON range")?)
        .map_err(|e| e.to_string())?;
    if forbidden(&doc, 0)
        || doc["asset"]["version"] != "2.0"
        || doc
            .get("extensionsRequired")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err("Unsupported static GLB feature or external URI".into());
    }
    let buffers = array(&doc, "buffers", 1)?;
    if buffers.len() != 1 {
        return Err("GLB needs one embedded buffer".into());
    }
    let length = field(&buffers[0], "byteLength")?;
    if length > bin_len || bin_len - length > 3 {
        return Err("GLB declared buffer length".into());
    }
    Ok((doc, &bytes[bin_at + 8..bin_at + 8 + length]))
}
struct Reader<'a> {
    doc: &'a Value,
    bin: &'a [u8],
}
impl<'a> Reader<'a> {
    fn view(&self, index: usize) -> Result<(&'a [u8], usize)> {
        let views = array(self.doc, "bufferViews", 128)?;
        let v = views.get(index).ok_or("GLB buffer view index")?;
        if field(v, "buffer")? != 0 {
            return Err("GLB nonembedded buffer".into());
        }
        let start = offset(v, "byteOffset")?;
        let end = start
            .checked_add(field(v, "byteLength")?)
            .ok_or("GLB view overflow")?;
        Ok((
            self.bin.get(start..end).ok_or("GLB buffer view range")?,
            offset(v, "byteStride")?,
        ))
    }
    fn accessor(
        &self,
        index: usize,
        component: usize,
        kind: &str,
        size: usize,
    ) -> Result<(&'a [u8], usize, usize)> {
        let a = array(self.doc, "accessors", 128)?
            .get(index)
            .ok_or("GLB accessor index")?;
        if field(a, "componentType")? != component
            || a["type"] != kind
            || a.get("normalized").is_some_and(|v| v != false)
        {
            return Err("Unsupported GLB accessor encoding".into());
        }
        let count = field(a, "count")?;
        if count == 0 || count > MAX_TRIANGLES * 3 {
            return Err("GLB accessor count budget".into());
        }
        let (view, stride) = self.view(field(a, "bufferView")?)?;
        let stride = if stride == 0 { size } else { stride };
        let start = offset(a, "byteOffset")?;
        let alignment = if component == 5126 { 4 } else { 2 };
        if stride < size || stride > 252 || stride % alignment != 0 || start % alignment != 0 {
            return Err("GLB accessor stride/alignment".into());
        }
        let end = (count - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(size))
            .and_then(|n| n.checked_add(start))
            .ok_or("GLB accessor overflow")?;
        Ok((
            view.get(start..end).ok_or("GLB accessor exceeds view")?,
            count,
            stride,
        ))
    }
    fn values<const N: usize>(&self, index: usize, kind: &str) -> Result<Vec<[f32; N]>> {
        let (bytes, count, stride) = self.accessor(index, 5126, kind, N * 4)?;
        if count > 16384 {
            return Err("GLB vertex budget".into());
        }
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let mut v = [0.; N];
            for (j, n) in v.iter_mut().enumerate() {
                let b: [u8; 4] = bytes[i * stride + j * 4..i * stride + j * 4 + 4]
                    .try_into()
                    .map_err(|_| "GLB float")?;
                *n = f32::from_le_bytes(b);
                if !n.is_finite() || n.abs() > 100_000. {
                    return Err("GLB nonfinite/bounded vertex".into());
                }
            }
            out.push(v);
        }
        Ok(out)
    }
}
impl Model {
    pub fn primitives(&self) -> &[Primitive] {
        &self.primitives
    }
    pub fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }
    pub fn from_glb(bytes: &[u8]) -> Result<Self> {
        let (doc, bin) = chunks(bytes)?;
        let reader = Reader { doc: &doc, bin };
        let mut images = vec![];
        for image in array(&doc, "images", MAX_TEXTURES)? {
            if image["mimeType"] != "image/jpeg" {
                return Err("Static GLB supports embedded JPEG base color only".into());
            }
            let (data, stride) = reader.view(field(image, "bufferView")?)?;
            if stride != 0 {
                return Err("GLB image stride".into());
            }
            let options = DecoderOptions::new_safe()
                .set_max_width(MAX_TEXTURE_EDGE)
                .set_max_height(MAX_TEXTURE_EDGE)
                .jpeg_set_max_scans(16)
                .jpeg_set_out_colorspace(ColorSpace::RGB);
            let mut jpeg = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
            jpeg.decode_headers()
                .map_err(|e| format!("GLB JPEG headers: {e}"))?;
            let (w, h) = jpeg.dimensions().ok_or("GLB JPEG dimensions")?;
            if w == 0 || h == 0 || w > MAX_TEXTURE_EDGE || h > MAX_TEXTURE_EDGE {
                return Err("GLB texture dimensions".into());
            }
            let rgb = jpeg.decode().map_err(|e| format!("GLB JPEG: {e}"))?;
            if rgb.len() != w * h * 3 {
                return Err("GLB JPEG RGB dimensions".into());
            }
            let mut rgba = Vec::with_capacity(w * h * 4);
            for p in rgb.chunks_exact(3) {
                rgba.extend([p[0], p[1], p[2], 255]);
            }
            images.push((w, h, rgba));
        }
        let samplers = doc
            .get("samplers")
            .map(|_| array(&doc, "samplers", MAX_TEXTURES))
            .transpose()?
            .unwrap_or(&[]);
        let mut textures = vec![];
        for tex in array(&doc, "textures", MAX_TEXTURES)? {
            let source = field(tex, "source")?;
            let (w, h, rgba) = images.get(source).ok_or("GLB texture source")?;
            let sampler = tex
                .get("sampler")
                .map(|v| {
                    number(v)
                        .and_then(|i| samplers.get(i).ok_or_else(|| "GLB sampler index".into()))
                })
                .transpose()?;
            let mut repeat = [true; 2];
            if let Some(s) = sampler {
                for (i, k) in ["wrapS", "wrapT"].iter().enumerate() {
                    repeat[i] = match s.get(*k).map(number).transpose()?.unwrap_or(10497) {
                        10497 => true,
                        33071 => false,
                        _ => return Err("GLB texture wrap mode".into()),
                    };
                }
            }
            if repeat.iter().any(|r| *r) && (!w.is_power_of_two() || !h.is_power_of_two()) {
                return Err("GLB repeating texture must be power of two for GLES2".into());
            }
            let mut hash = Sha256::new();
            hash.update(rgba);
            hash.update((*w as u32).to_le_bytes());
            hash.update((*h as u32).to_le_bytes());
            hash.update(repeat.map(u8::from));
            textures.push(Arc::new(Texture {
                id: hash.finalize().into(),
                width: *w as u32,
                height: *h as u32,
                rgba: rgba.clone(),
                repeat,
            }));
        }
        let materials = array(&doc, "materials", MAX_BATCHES)?;
        let meshes = array(&doc, "meshes", MAX_BATCHES)?;
        let nodes = array(&doc, "nodes", 64)?;
        let scenes = array(&doc, "scenes", 16)?;
        let scene = doc.get("scene").map(number).transpose()?.unwrap_or(0);
        let roots = array(scenes.get(scene).ok_or("GLB scene index")?, "nodes", 64)?;
        let mut placements = vec![];
        fn walk(
            nodes: &[Value],
            index: usize,
            parent: Matrix,
            path: &mut Vec<usize>,
            out: &mut Vec<(usize, Matrix)>,
            visits: &mut usize,
        ) -> Result<()> {
            *visits += 1;
            if *visits > 128 {
                return Err("GLB node traversal budget".into());
            }
            if path.len() >= 32 || path.contains(&index) || out.len() > MAX_BATCHES {
                return Err("GLB node cycle/depth/instance budget".into());
            }
            let node = nodes.get(index).ok_or("GLB node index")?;
            path.push(index);
            let m = multiply(parent, node_matrix(node)?);
            if let Some(mesh) = node.get("mesh") {
                out.push((number(mesh)?, m));
            }
            if node.get("camera").is_some() {
                return Err("GLB cameras are supplied by the host".into());
            }
            if node.get("children").is_some() {
                for child in array(node, "children", 64)? {
                    walk(nodes, number(child)?, m, path, out, visits)?;
                }
            }
            path.pop();
            Ok(())
        }
        let mut visits = 0;
        for root in roots {
            walk(
                nodes,
                number(root)?,
                IDENTITY,
                &mut vec![],
                &mut placements,
                &mut visits,
            )?;
        }
        let mut model = Self {
            primitives: vec![],
            bounds: [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
        };
        let mut triangle_count = 0;
        for (index, pose) in placements {
            let normal = normal_matrix(&pose)?;
            let mesh = meshes.get(index).ok_or("GLB mesh index")?;
            for p in array(mesh, "primitives", MAX_BATCHES)? {
                if model.primitives.len() >= MAX_BATCHES
                    || p.get("mode").map(number).transpose()?.unwrap_or(4) != 4
                {
                    return Err("GLB triangle primitive budget/type".into());
                }
                let attrs = p["attributes"].as_object().ok_or("GLB attributes")?;
                if attrs.len() != 3
                    || !["POSITION", "NORMAL", "TEXCOORD_0"]
                        .iter()
                        .all(|a| attrs.contains_key(*a))
                {
                    return Err("Unsupported static GLB attributes".into());
                }
                let positions =
                    reader.values::<3>(number(&p["attributes"]["POSITION"])?, "VEC3")?;
                let normals = reader.values::<3>(number(&p["attributes"]["NORMAL"])?, "VEC3")?;
                let uvs = reader.values::<2>(number(&p["attributes"]["TEXCOORD_0"])?, "VEC2")?;
                if positions.len() != normals.len() || positions.len() != uvs.len() {
                    return Err("GLB attribute lengths".into());
                }
                let mut vertices = Vec::with_capacity(positions.len());
                for ((p, n), uv) in positions.into_iter().zip(normals).zip(uvs) {
                    if dot(n, n) < 1e-12 || uv.iter().any(|n| n.abs() > 64.) {
                        return Err("GLB invalid normal/UV".into());
                    }
                    let p = point(&pose, p);
                    if p.iter().any(|n| !n.is_finite() || n.abs() > 100_000.) {
                        return Err("GLB transformed position bound".into());
                    }
                    for (axis, value) in p.iter().enumerate() {
                        model.bounds[0][axis] = model.bounds[0][axis].min(*value);
                        model.bounds[1][axis] = model.bounds[1][axis].max(*value);
                    }
                    vertices.push(Vertex {
                        position: p,
                        normal: unit(vector(&normal, n)),
                        uv,
                    });
                }
                let (bytes, count, stride) =
                    reader.accessor(field(p, "indices")?, 5123, "SCALAR", 2)?;
                triangle_count += count / 3;
                if count % 3 != 0 || triangle_count > MAX_TRIANGLES {
                    return Err("GLB triangle limit".into());
                }
                let mut indices = Vec::with_capacity(count);
                for i in 0..count {
                    let n = u16::from_le_bytes([bytes[i * stride], bytes[i * stride + 1]]);
                    if usize::from(n) >= vertices.len() {
                        return Err("GLB index exceeds vertex buffer".into());
                    }
                    indices.push(n);
                }
                if determinant(&pose) < 0. {
                    for tri in indices.chunks_exact_mut(3) {
                        tri.swap(1, 2);
                    }
                }
                let material = materials
                    .get(field(p, "material")?)
                    .ok_or("GLB material index")?;
                if material.get("alphaMode").is_some_and(|v| v != "OPAQUE")
                    || material
                        .get("emissiveFactor")
                        .is_some_and(|v| !matches!(floats(v, [0.; 3]), Ok([0., 0., 0.])))
                {
                    return Err("Unsupported static GLB material".into());
                }
                let pbr = &material["pbrMetallicRoughness"];
                let mut color = floats(&pbr["baseColorFactor"], [1.; 4])?;
                if color.iter().any(|n| !(0. ..=1.).contains(n))
                    || pbr["baseColorTexture"]
                        .get("texCoord")
                        .is_some_and(|v| v != 0)
                {
                    return Err("GLB base color/UV set".into());
                }
                // glTF OPAQUE ignores the material alpha channel. Per-instance
                // opacity remains a presentation effect applied during projection.
                color[3] = 1.;
                let texture = textures
                    .get(field(&pbr["baseColorTexture"], "index")?)
                    .ok_or("GLB material texture index")?
                    .clone();
                model.primitives.push(Primitive {
                    vertices,
                    indices,
                    texture,
                    color,
                    double_sided: material["doubleSided"].as_bool().unwrap_or(false),
                });
            }
        }
        if model.primitives.is_empty()
            || (0..3).all(|i| model.bounds[1][i] - model.bounds[0][i] < 1e-6)
        {
            return Err("Empty/degenerate GLB bounds".into());
        }
        Ok(model)
    }
}
pub fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| {
        let row = i % 4;
        let col = i / 4;
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}
fn validate_matrix(m: &Matrix) -> Result<()> {
    if m.iter().any(|n| !n.is_finite() || n.abs() > 100_000.)
        || [m[3], m[7], m[11], m[15]] != [0., 0., 0., 1.]
    {
        return Err("GLB needs finite affine matrix".into());
    }
    Ok(())
}
fn node_matrix(node: &Value) -> Result<Matrix> {
    if node.get("matrix").is_some() {
        if ["translation", "rotation", "scale"]
            .iter()
            .any(|k| node.get(*k).is_some())
        {
            return Err("GLB matrix mixed with TRS".into());
        }
        let m = floats(&node["matrix"], IDENTITY)?;
        validate_matrix(&m)?;
        return Ok(m);
    }
    let [x, y, z, w] = floats(&node["rotation"], [0., 0., 0., 1.])?;
    if (x * x + y * y + z * z + w * w - 1.).abs() > 0.001 {
        return Err("GLB rotation quaternion".into());
    }
    let s = floats(&node["scale"], [1.; 3])?;
    let t = floats(&node["translation"], [0.; 3])?;
    let m = [
        (1. - 2. * (y * y + z * z)) * s[0],
        2. * (x * y + z * w) * s[0],
        2. * (x * z - y * w) * s[0],
        0.,
        2. * (x * y - z * w) * s[1],
        (1. - 2. * (x * x + z * z)) * s[1],
        2. * (y * z + x * w) * s[1],
        0.,
        2. * (x * z + y * w) * s[2],
        2. * (y * z - x * w) * s[2],
        (1. - 2. * (x * x + y * y)) * s[2],
        0.,
        t[0],
        t[1],
        t[2],
        1.,
    ];
    validate_matrix(&m)?;
    Ok(m)
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    // Rescale first: valid inverse-transpose normals can be large under very
    // nonuniform affine transforms, even though their direction is well defined.
    let scale = v
        .iter()
        .map(|n| n.abs())
        .fold(0., f32::max)
        .max(f32::MIN_POSITIVE);
    let v = v.map(|n| n / scale);
    let d = dot(v, v).sqrt().max(f32::MIN_POSITIVE);
    v.map(|n| n / d)
}
fn vector(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2])
}
fn point(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    let v = vector(m, p);
    std::array::from_fn(|i| v[i] + m[12 + i])
}
fn determinant(m: &Matrix) -> f32 {
    dot(
        [m[0], m[1], m[2]],
        cross([m[4], m[5], m[6]], [m[8], m[9], m[10]]),
    )
}
fn normal_matrix(m: &Matrix) -> Result<Matrix> {
    let a = [m[0], m[1], m[2]];
    let b = [m[4], m[5], m[6]];
    let c = [m[8], m[9], m[10]];
    let x = cross(b, c);
    let y = cross(c, a);
    let z = cross(a, b);
    let d = dot(a, x);
    if !d.is_finite() || d.abs() < 1e-12 {
        return Err("Singular model transform".into());
    }
    Ok([
        x[0] / d,
        x[1] / d,
        x[2] / d,
        0.,
        y[0] / d,
        y[1] / d,
        y[2] / d,
        0.,
        z[0] / d,
        z[1] / d,
        z[2] / d,
        0.,
        0.,
        0.,
        0.,
        1.,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    const HAT: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/models/relay-hat.glb"
    ));
    fn rewrite(doc: &Value, bin: &[u8]) -> Vec<u8> {
        let mut json = serde_json::to_vec(doc).unwrap();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut buffer = bin.to_vec();
        while buffer.len() % 4 != 0 {
            buffer.push(0);
        }
        let size = 28 + json.len() + buffer.len();
        let mut out = b"glTF".to_vec();
        for n in [2, size as u32, json.len() as u32, 0x4e4f534a] {
            out.extend(n.to_le_bytes());
        }
        out.extend(json);
        out.extend((buffer.len() as u32).to_le_bytes());
        out.extend(0x004e4942u32.to_le_bytes());
        out.extend(buffer);
        out
    }
    #[test]
    fn static_glb_keeps_uvs_and_shared_textures_and_rejects_unbounded_inputs() {
        let model = Model::from_glb(HAT).unwrap();
        assert!(HAT.len() < 300_000);
        assert!(
            model.primitives[0]
                .vertices
                .windows(2)
                .any(|v| v[0].uv != v[1].uv)
        );
        let (doc, bin) = chunks(HAT).unwrap();
        let mut instanced = doc.clone();
        instanced["nodes"][0]["children"] = serde_json::json!([1]);
        let mut node = instanced["nodes"][0].clone();
        node.as_object_mut().unwrap().remove("children");
        node["translation"] = serde_json::json!([2., 0., 0.]);
        node["scale"] = serde_json::json!([0.1, 0.2, 0.3]);
        instanced["nodes"].as_array_mut().unwrap().push(node);
        // Two instances would exceed the triangle budget for this detailed hat;
        // shorten the index accessor while retaining valid shared vertex data.
        let index = number(&doc["meshes"][0]["primitives"][0]["indices"]).unwrap();
        instanced["accessors"][index]["count"] = serde_json::json!(3);
        let copies = Model::from_glb(&rewrite(&instanced, bin)).unwrap();
        assert_eq!(copies.primitives.len(), 2);
        assert!(Arc::ptr_eq(
            &copies.primitives[0].texture,
            &copies.primitives[1].texture
        ));
        assert!(copies.bounds[1][0] > 1.9);
        let frame = Frame::project(
            &[Instance {
                model: &copies,
                pose: IDENTITY,
                opacity: 0.7,
            }],
            Camera {
                center: [100., 100.],
                pixels_per_unit: 20.,
                distance: 4.5,
                near: 1.,
                far: 8.,
            },
        )
        .unwrap();
        assert_eq!(frame.batches.len(), 2);
        assert!(frame.vertices.iter().all(|n| n.is_finite()));
        let v = &copies.primitives[0].vertices[usize::from(copies.primitives[0].indices[0])];
        assert_eq!(frame.vertices[3..5], v.uv);
        for bad in 0..6 {
            let mut d = doc.clone();
            let mut b = bin.to_vec();
            match bad {
                0 => {
                    d["buffers"][0]["uri"] = serde_json::json!("https://example.invalid/asset.bin")
                }
                1 => d["accessors"][index]["count"] = serde_json::json!(MAX_TRIANGLES * 3 + 3),
                2 => d["bufferViews"][0]["byteOffset"] = serde_json::json!(u64::MAX),
                3 => d["nodes"][0]["children"] = serde_json::json!([0]),
                4 => d["meshes"][0]["primitives"][0]["targets"] = serde_json::json!([]),
                _ => {
                    let accessor = &d["accessors"][number(
                        &d["meshes"][0]["primitives"][0]["attributes"]["POSITION"],
                    )
                    .unwrap()];
                    let view = &d["bufferViews"][field(accessor, "bufferView").unwrap()];
                    let at = offset(view, "byteOffset").unwrap()
                        + offset(accessor, "byteOffset").unwrap();
                    b[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
                }
            }
            assert!(
                Model::from_glb(&rewrite(&d, &b)).is_err(),
                "accepted malformed case {bad}"
            );
        }
        assert!(Model::from_glb(&HAT[..HAT.len() - 1]).is_err());
    }
}
