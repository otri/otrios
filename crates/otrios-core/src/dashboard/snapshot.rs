// SPDX-License-Identifier: LGPL-3.0-only
use super::Result;
use flexbuffers::{Builder, BuilderOptions, Reader};
use otrios_interfaces::{ContentId, DashboardView, ViewContent, ViewNode};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;

const MAX_SNAPSHOT: usize = super::layout::MAX_LAYOUT_BYTES + 32 * 1024;
/// Complete, validated, immutable binary view. Clone pins the previous revision.
#[derive(Clone)]
pub struct ViewSnapshot {
    bytes: Arc<[u8]>,
    view: Arc<DashboardView>,
    scene: Arc<super::scene::Scene>,
    layout: super::layout::Layout,
    id: ContentId,
}
impl ViewSnapshot {
    pub fn build(view: DashboardView) -> Result<Self> {
        Self::build_with_layout(view, None)
    }
    pub fn build_with_layout(view: DashboardView, layout: Option<&[u8]>) -> Result<Self> {
        Self::build_cached(view, layout, None)
    }
    pub(crate) fn build_cached(
        view: DashboardView,
        layout: Option<&[u8]>,
        cached: Option<&Self>,
    ) -> Result<Self> {
        let mut b = Builder::new(BuilderOptions::default());
        {
            let mut root = b.start_map();
            root.push("schema", "otrios-view-v1");
            if let Some(bytes) = layout {
                if bytes.len() > super::layout::MAX_LAYOUT_BYTES {
                    return Err("Layout too large".into());
                }
                root.push("layout", flexbuffers::Blob(bytes));
            }
            {
                let mut heads = root.start_vector("heads");
                for h in &view.heads {
                    heads.push(h.as_str());
                }
            }
            {
                let mut nodes = root.start_vector("nodes");
                for n in &view.nodes {
                    let mut m = nodes.start_map();
                    m.push("id", n.id);
                    {
                        let mut r = m.start_vector("bounds");
                        for v in n.bounds {
                            r.push(v);
                        }
                    }
                    match &n.content {
                        ViewContent::Label(label) => {
                            m.push("kind", 0_u32);
                            m.push("label", label.as_str());
                        }
                        ViewContent::Switch { label, enabled } => {
                            m.push("kind", 1_u32);
                            m.push("label", label.as_str());
                            m.push("enabled", *enabled);
                        }
                        ViewContent::Graph { label, samples } => {
                            m.push("kind", 2_u32);
                            m.push("label", label.as_str());
                            let mut v = m.start_vector("samples");
                            for x in samples {
                                v.push(*x);
                            }
                        }
                    }
                }
            }
        }
        Self::from_bytes_cached(b.view(), cached)
    }
    /// Check lengths before allocation and validate every consumed field before activation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_cached(bytes, None)
    }
    fn from_bytes_cached(bytes: &[u8], cached: Option<&Self>) -> Result<Self> {
        let parse = || -> std::result::Result<DashboardView, Box<dyn std::error::Error>> {
            if bytes.len() > MAX_SNAPSHOT {
                return Err("Snapshot too large".into());
            }
            let root = Reader::get_root(bytes)?.get_map()?;
            if root.index("schema")?.get_str()? != "otrios-view-v1" {
                return Err("Unknown snapshot schema".into());
            }
            let hs = root.index("heads")?.get_vector()?;
            if hs.len() > 64 {
                return Err("Too many heads".into());
            }
            let mut heads = Vec::new();
            for i in 0..hs.len() {
                let h = hs.index(i)?.get_str()?;
                if h.len() != 64 || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err("Invalid head".into());
                }
                heads.push(h.to_string());
            }
            let ns = root.index("nodes")?.get_vector()?;
            if ns.len() > 16 {
                return Err("Too many nodes".into());
            }
            let mut nodes = Vec::new();
            let mut ids = BTreeSet::new();
            for i in 0..ns.len() {
                let m = ns.index(i)?.get_map()?;
                let id = u32::try_from(m.index("id")?.get_u64()?)?;
                if !ids.insert(id) {
                    return Err("Duplicate node".into());
                }
                let rect = m.index("bounds")?.get_vector()?;
                if rect.len() != 4 {
                    return Err("Invalid bounds".into());
                }
                let mut bounds = [0_u32; 4];
                for (i, v) in bounds.iter_mut().enumerate() {
                    *v = u32::try_from(rect.index(i)?.get_u64()?)?;
                }
                let [x, y, w, h] = bounds;
                if w == 0
                    || h == 0
                    || x.checked_add(w).is_none_or(|v| v > 1280)
                    || y.checked_add(h).is_none_or(|v| v > 800)
                {
                    return Err("Bounds outside canvas".into());
                }
                let label = m.index("label")?.get_str()?;
                if label.len() > 96 {
                    return Err("Label too long".into());
                }
                let content = match m.index("kind")?.get_u64()? {
                    0 => ViewContent::Label(label.to_string()),
                    1 => ViewContent::Switch {
                        label: label.to_string(),
                        enabled: m.index("enabled")?.get_bool()?,
                    },
                    2 => {
                        let vs = m.index("samples")?.get_vector()?;
                        if vs.len() > 32 {
                            return Err("Too many samples".into());
                        }
                        let mut samples = Vec::new();
                        for j in 0..vs.len() {
                            let v = vs.index(j)?.get_u64()?;
                            if v > 100 {
                                return Err("Invalid sample".into());
                            }
                            samples.push(v as u32);
                        }
                        ViewContent::Graph {
                            label: label.to_string(),
                            samples,
                        }
                    }
                    _ => return Err("Unknown node kind".into()),
                };
                let min = match &content {
                    ViewContent::Switch { .. } => [320, 200],
                    ViewContent::Graph { .. } => [160, 200],
                    ViewContent::Label(_) => [
                        if id == 4 { 32 } else { 1 },
                        if id == 1 {
                            48
                        } else if id == 4 {
                            32
                        } else {
                            16
                        },
                    ],
                };
                if w < min[0] || h < min[1] {
                    return Err("Node too small for this renderer".into());
                }
                nodes.push(ViewNode {
                    id,
                    bounds,
                    content,
                });
            }
            Ok(DashboardView { heads, nodes })
        };
        let view = parse().map_err(|e| e.to_string())?;
        let root = Reader::get_root(bytes)
            .map_err(|e| e.to_string())?
            .get_map()
            .map_err(|e| e.to_string())?;
        let source = if root.iter_keys().any(|key| key == "layout") {
            let value = root.index("layout").map_err(|e| e.to_string())?;
            if value.length() > super::layout::MAX_LAYOUT_BYTES {
                return Err("Layout blob too large".into());
            }
            value.get_blob().map_err(|e| e.to_string())?.0
        } else {
            super::layout::DEFAULT_LAYOUT
        };
        let reused = cached.filter(|snapshot| snapshot.layout.bytes() == source);
        let layout = if let Some(layout) = reused {
            layout.layout.clone()
        } else if source == super::layout::DEFAULT_LAYOUT {
            super::layout::Layout::bootstrap()?
        } else {
            super::layout::Layout::from_bytes(source)?
        };
        if reused.is_none() {
            layout.qualify(&view)?;
        }
        // Heads version the binary snapshot but are not layout bindings. Reuse
        // already validated geometry only when every consumed input is equal.
        let scene = if let Some(previous) = reused.filter(|s| s.view.nodes == view.nodes) {
            previous.scene.clone()
        } else {
            Arc::new(layout.resolve(&view)?)
        };
        // Versioned domain separates view bytes from other content object types.
        let mut hash = Sha256::new();
        hash.update(b"OTriOS\0view\0v1\0");
        hash.update(bytes);
        Ok(Self {
            bytes: bytes.into(),
            scene,
            layout,
            view: Arc::new(view),
            id: ContentId(hash.finalize().into()),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn view(&self) -> &DashboardView {
        &self.view
    }
    pub fn scene(&self) -> &Arc<super::scene::Scene> {
        &self.scene
    }
    pub fn layout(&self) -> &super::layout::Layout {
        &self.layout
    }
    pub fn id(&self) -> ContentId {
        self.id
    }
    pub fn hex_id(&self) -> String {
        self.id.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_and_truncated_snapshots_are_rejected() {
        let snap = crate::dashboard::Replica::new().unwrap().snapshot().clone();
        for n in 0..snap.bytes().len() {
            assert!(ViewSnapshot::from_bytes(&snap.bytes()[..n]).is_err());
        }
        assert!(ViewSnapshot::from_bytes(&vec![0; MAX_SNAPSHOT + 1]).is_err());
        let bad = DashboardView {
            heads: vec![],
            nodes: vec![ViewNode {
                id: 1,
                bounds: [u32::MAX, 0, 100, 10],
                content: ViewContent::Label("bad".into()),
            }],
        };
        assert!(ViewSnapshot::build(bad).is_err());
    }
}
