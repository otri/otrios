// SPDX-License-Identifier: Apache-2.0
//! Bounded local synthetic mesh fixtures shared by renderer probes.
use std::{io::Read, sync::Arc};
pub struct Mesh {
    pub vertices: Vec<f32>,
    pub glyphs: Vec<f32>,
    pub draws: Vec<u32>,
}
pub struct Frame {
    pub mesh: Arc<Mesh>,
    pub views: Vec<f32>,
}
fn word(input: &mut impl Read) -> Result<u32, String> {
    let mut b = [0; 4];
    input.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(u32::from_le_bytes(b))
}
pub fn load(path: &str) -> Result<Vec<Frame>, String> {
    let mut input = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let mut magic = [0; 8];
    input.read_exact(&mut magic).map_err(|e| e.to_string())?;
    let retained = &magic == b"OTRTEXT2";
    if &magic != b"OTRTEXT1" && !retained {
        return Err("Invalid mesh stream".into());
    }
    let count = word(&mut input)?;
    if count > 1000 {
        return Err("Too many frames".into());
    }
    let mut meshes: Vec<Frame> = vec![];
    for _ in 0..count {
        if retained {
            word(&mut input)?;
        }
        let counts = [word(&mut input)?, word(&mut input)?, word(&mut input)?];
        let view_count = if retained { word(&mut input)? } else { 0 };
        if view_count > 4096 * 8 {
            return Err("Viewport limit".into());
        }
        if counts.iter().any(|n| *n > 1_000_000) {
            return Err("Mesh limit".into());
        }
        let mut floats = |n| -> Result<Vec<f32>, String> {
            (0..n)
                .map(|_| word(&mut input).map(f32::from_bits))
                .collect()
        };
        let vertices = floats(counts[0])?;
        let glyphs = floats(counts[1])?;
        let draws = (0..counts[2])
            .map(|_| word(&mut input))
            .collect::<Result<_, _>>()?;
        let views = (0..view_count)
            .map(|_| word(&mut input).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let mesh = if retained && counts == [0, 0, 0] {
            meshes.last().ok_or("Missing initial mesh")?.mesh.clone()
        } else {
            Arc::new(Mesh {
                vertices,
                glyphs,
                draws,
            })
        };
        meshes.push(Frame { mesh, views });
    }
    if meshes.is_empty() {
        return Err("Empty stream".into());
    }
    Ok(meshes)
}
