// SPDX-License-Identifier: LGPL-3.0-only
//! Independent full-layout reference versus bounded reader frames. Flattening
//! here is fixture serialization only; production hosts retain separate chunks.
#[path = "common/document_reference.rs"]
mod reference;
use otrios_core::{
    canvas::document::{Reader, Source},
    dashboard::{Mesh, font::Font, font_reference, retained_text::TextPresentation},
};
use std::{io::Write, sync::Arc};
fn write_frame(
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
    Err("Document fixture did not settle".into())
}
fn main() -> Result<(), String> {
    let output = std::env::args().nth(1).ok_or("Supply output prefix")?;
    let text = "jgyÉ aa aa-aaaa\t The quick brown fox jumps over the lazy dog. Unsupported cluster 👩‍💻, combining á, and  spaces.\r\n\n".repeat(35);
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
    let mut a =
        std::fs::File::create(format!("{output}-reference.mesh")).map_err(|e| e.to_string())?;
    let mut b =
        std::fs::File::create(format!("{output}-chunked.mesh")).map_err(|e| e.to_string())?;
    for file in [&mut a, &mut b] {
        file.write_all(b"OTRTEXT3").map_err(|e| e.to_string())?;
        file.write_all(&80_u32.to_le_bytes())
            .map_err(|e| e.to_string())?;
    }
    for suffix in ["reference", "chunked"] {
        std::fs::write(format!("{output}-{suffix}.mesh.font"), &packet)
            .map_err(|e| e.to_string())?;
    }
    let mut reader = Reader::new(
        Source::new(text.clone())?,
        font.clone(),
        2.,
        [239, 244, 236, 255],
        1184.,
        600.,
    )?;
    let mut reference = reference::Reference::new(&text, &font, 2, 1184.);
    let mut presentation = TextPresentation::default();
    for i in 0..80 {
        if i == 40 {
            reader.resize(704., 600.)?;
            reference = reference::Reference::new(&text, &font, 2, 704.);
        }
        if i != 40 {
            let phase = if i < 40 {
                f64::from(i) / 39.
            } else {
                f64::from(79 - i) / 38.
            };
            reader.scroll_to(phase * (reference.height - 600.).max(0.) + 0.125);
        }
        settle(&mut reader)?;
        let width = if i < 40 { 1184. } else { 704. };
        // Check independent line maps before producing any raster comparison.
        for (block, line) in reader.resident_lines() {
            let expected = reference
                .lines
                .iter()
                .find(|l| l.block == block && l.number == line.line)
                .ok_or("Missing reference line")?;
            if (line.source.start, line.source.end) != (expected.start, expected.end) {
                return Err("Independent document line mismatch".into());
            }
        }
        let scalar = presentation.compose(&reference.scene(width, 600., reader.offset()), &font);
        write_frame(&mut a, i + 1, &scalar.mesh, &scalar.views, &scalar.styles)?;
        let frame = reader.frame();
        let mut mesh = Mesh::default();
        let mut views = Vec::new();
        let mut styles = Vec::new();
        for draw in frame.draws {
            let first = (mesh.glyph_vertices.len() / 16) as u32;
            mesh.glyph_vertices.extend(&draw.chunk.mesh.glyph_vertices);
            mesh.draws
                .extend([1, first, (draw.chunk.mesh.glyph_vertices.len() / 16) as u32]);
            let mut view = draw.view;
            view[0] += first as f32;
            view[2] += 48.;
            view[3] += 100.;
            view[4..].copy_from_slice(&[48., 100., 48. + width, 700.]);
            views.extend(view);
            styles.push(frame.em);
        }
        write_frame(&mut b, i + 1, &mesh, &views, &styles)?;
    }
    Ok(())
}
