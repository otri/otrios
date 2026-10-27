// SPDX-License-Identifier: LGPL-3.0-only
//! Independent unpaged text + scene-avatar oracle versus stable block chunks.
//! Mesh flattening here is bounded qualification serialization, never a host path.
#[path = "common/message_fixture.rs"]
mod fixture;
#[path = "common/document_reference.rs"]
#[allow(
    dead_code,
    reason = "the shared oracle also supports a flat-document scene; this probe constructs a message scene"
)]
mod reference;
use fixture::Sample;
use otrios_core::{
    canvas::document::{Anchor, Reader},
    dashboard::{
        Mesh, font::Font, font_reference, render_scene, retained_text::TextPresentation,
        scene::Scene,
    },
};
use otrios_interfaces::scene::{Primitive, Rect, SceneNode};
use std::{io::Write, sync::Arc};
fn write(
    out: &mut impl Write,
    generation: u32,
    mesh: &Mesh,
    views: &[f32],
    styles: &[f32],
) -> Result<(), String> {
    for n in [
        generation,
        mesh.vertices.len() as u32,
        mesh.glyph_vertices.len() as u32,
        mesh.draws.len() as u32,
        views.len() as u32,
        styles.len() as u32,
    ] {
        out.write_all(&n.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    for n in mesh.vertices.iter().chain(&mesh.glyph_vertices) {
        out.write_all(&n.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    for n in &mesh.draws {
        out.write_all(&n.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    for n in views.iter().chain(styles) {
        out.write_all(&n.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn settle(reader: &mut Reader) -> Result<(), String> {
    for _ in 0..100_000 {
        reader.work(16)?;
        if reader.extent().1 && !reader.frame().loading {
            return Ok(());
        }
    }
    Err("Message fixture did not settle".into())
}
fn references(samples: &[Sample], font: &Font, width: f64) -> Vec<reference::Reference> {
    samples
        .iter()
        .map(|s| {
            reference::Reference::new(
                s.text.strip_suffix('\n').unwrap_or(&s.text),
                font,
                2,
                width - 64.,
            )
        })
        .collect()
}
// Build each oracle row in local coordinates, then apply independently computed
// full-layout placement through the host's common view contract. This avoids
// comparing CPU-clipped world-space triangle arithmetic with GPU translation.
fn expected(
    samples: &[Sample],
    refs: &[reference::Reference],
    font: &Font,
    width: f32,
    offset: f64,
) -> Result<(Mesh, Vec<f32>, Vec<f32>), String> {
    let mut mesh = Mesh::default();
    let mut views = vec![];
    let mut styles = vec![];
    let mut top = 0.;
    for (sample, reference) in samples.iter().zip(refs) {
        let y = (top - offset) as f32;
        top += reference.height;
        if f64::from(y) + reference.height < -256. || y > 856. {
            continue;
        }
        if let Some(avatar) = sample.avatar {
            let icon = render_scene(&Scene::new(otrios_core::canvas::chat::icons::avatar(
                avatar, 1, 32.,
            ))?);
            let base = (mesh.vertices.len() / 6) as u32;
            mesh.vertices.extend(&icon.vertices);
            for run in icon.draws.chunks_exact(3) {
                mesh.draws.extend([0, base + run[1], run[2]]);
                views.extend([
                    (base + run[1]) as f32,
                    run[2] as f32,
                    60.,
                    96. + y,
                    48.,
                    100.,
                    48. + width,
                    700.,
                ]);
                styles.push(0.);
            }
        }
        let mut children = vec![];
        let mut id = 1;
        for line in &reference.lines {
            for (x, value) in &line.runs {
                children.push(SceneNode {
                    id,
                    frame: Rect::new(64. + *x as f32, line.top as f32, 8192., 16.),
                    primitive: Primitive::Text {
                        value: value.clone(),
                        scale: 2,
                        color: otrios_core::canvas::chat::icons::TEXT,
                    },
                    children: vec![],
                    responder: None,
                    semantics: None,
                });
                id += 1;
            }
        }
        let scene = Scene::new(SceneNode {
            id: 0,
            frame: Rect::new(0., 0., width, 8192.),
            primitive: Primitive::Canvas { clip: true },
            children,
            responder: None,
            semantics: None,
        })?;
        let text = TextPresentation::default().compose(&scene, font);
        let base = (mesh.glyph_vertices.len() / 16) as u32;
        mesh.glyph_vertices.extend(&text.mesh.glyph_vertices);
        for ((run, view), style) in text
            .mesh
            .draws
            .chunks_exact(3)
            .zip(text.views.chunks_exact(8))
            .zip(text.styles.iter())
        {
            if run[0] != 1 {
                return Err("Unexpected oracle geometry".into());
            }
            mesh.draws.extend([1, base + run[1], run[2]]);
            views.extend([
                base as f32 + view[0],
                view[1],
                48. + view[2],
                100. + y + view[3],
                48.,
                100.,
                48. + width,
                700.,
            ]);
            styles.push(*style);
        }
    }
    Ok((mesh, views, styles))
}

fn main() -> Result<(), String> {
    let output = std::env::args().nth(1).ok_or("Supply output prefix")?;
    let font = Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?);
    let atlas = font.atlas();
    let mut packet = b"OTRFONT1".to_vec();
    for n in [
        atlas.curve_width,
        atlas.curve_height,
        atlas.band_width,
        atlas.band_height,
    ] {
        packet.extend(n.to_le_bytes());
    }
    for n in &atlas.curves {
        packet.extend(n.to_le_bytes());
    }
    packet.extend(&atlas.bands);
    let mut files = vec![];
    for suffix in ["reference", "blocks"] {
        let path = format!("{output}-{suffix}.mesh");
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(b"OTRTEXT3")
            .and_then(|_| file.write_all(&80_u32.to_le_bytes()))
            .map_err(|e| e.to_string())?;
        std::fs::write(format!("{path}.font"), &packet).map_err(|e| e.to_string())?;
        files.push(file);
    }
    let mut samples = fixture::samples(100, 32)?;
    let mut width = 1184.;
    let mut refs = references(&samples, &font, width);
    let mut reader = Reader::new(
        fixture::source(&samples)?,
        font.clone(),
        2.,
        otrios_core::canvas::chat::icons::TEXT,
        width,
        600.,
    )?;
    settle(&mut reader)?;
    let target = samples
        .iter()
        .position(|s| s.id.object == 110 && s.id.part == 0)
        .unwrap();
    reader.jump_to(Anchor {
        block: target as u32,
        source_byte: 0,
        viewport_y: -3.5,
    })?;
    settle(&mut reader)?;
    let mut peak = 0;
    let mut comparisons = 0;
    for i in 0..80_u32 {
        if i == 17 || i == 59 {
            samples = fixture::samples(if i == 17 { 90 } else { 91 }, 64)?;
            reader.replace_blocks(fixture::source(&samples)?)?;
            refs = references(&samples, &font, width);
        } else if i == 40 || i == 79 {
            width = if i == 40 { 704. } else { 1184. };
            reader.resize(width, 600.)?;
            refs = references(&samples, &font, width);
        } else if i > 0 {
            reader.scroll_to(reader.offset() + if i == 1 { 0.125 } else { 3.125 });
        }
        settle(&mut reader)?;
        let expected_height = refs.iter().map(|r| r.height).sum::<f64>();
        if (reader.extent().0 - expected_height).abs() > 1e-7 {
            return Err(format!(
                "Independent message height mismatch at frame {i}: {} != {expected_height}",
                reader.extent().0
            ));
        }
        for (block, line) in reader.resident_lines() {
            let expected = refs[block as usize]
                .lines
                .get(line.line as usize)
                .ok_or("Missing independent line")?;
            let end = expected.end
                + u32::from(
                    samples[block as usize]
                        .text
                        .as_bytes()
                        .get(expected.end as usize)
                        == Some(&b'\n'),
                );
            if line.source.start != expected.start || line.source.end != end {
                return Err(format!(
                    "Independent message line mismatch at frame {i}, block {block}, line {}",
                    line.line
                ));
            }
            comparisons += 1;
        }
        peak = peak.max(reader.stats().working_bytes);
        let (mesh, views, styles) =
            expected(&samples, &refs, &font, width as f32, reader.offset())?;
        write(&mut files[0], i + 1, &mesh, &views, &styles)?;
        let frame = reader.frame();
        let mut mesh = Mesh::default();
        let mut views = vec![];
        let mut styles = vec![];
        for draw in frame.draws {
            let bases = [
                (mesh.vertices.len() / 6) as u32,
                (mesh.glyph_vertices.len() / 16) as u32,
            ];
            mesh.vertices.extend(&draw.chunk.mesh.vertices);
            mesh.glyph_vertices.extend(&draw.chunk.mesh.glyph_vertices);
            for (run, view) in draw
                .chunk
                .mesh
                .draws
                .chunks_exact(3)
                .zip(draw.views().chunks_exact(8))
            {
                mesh.draws
                    .extend([run[0], run[1] + bases[run[0] as usize], run[2]]);
                let mut view = <[f32; 8]>::try_from(view).unwrap();
                view[0] += bases[run[0] as usize] as f32;
                view[2] += 48.;
                view[3] += 100.;
                view[4..].copy_from_slice(&[48., 100., 48. + width as f32, 700.]);
                views.extend(view);
                styles.push(if run[0] == 1 { frame.em } else { 0. });
            }
        }
        write(&mut files[1], i + 1, &mesh, &views, &styles)?;
    }
    println!(
        "{}",
        serde_json::json!({"frames":80,"independent_line_comparisons":comparisons,"working_peak_bytes":peak,"cases":{"0":"initial clipped header/avatar","1":"fractional scroll","17":"prepend coverage","59":"append coverage","79":"width reflow"},"fixture_only":true})
    );
    Ok(())
}
