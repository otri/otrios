// SPDX-License-Identifier: LGPL-3.0-only
//! Synthetic Input Lab mesh stream for isolated renderer measurements; no live state.
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../tools/canvas/components.rs"]
pub mod components;
#[path = "../../../apps/input-lab/app.rs"]
mod input_lab;
use otrios_core::{
    canvas::*,
    dashboard::{font::Font, render_scene_with_font},
};
use otrios_interfaces::{
    canvas::*,
    layout::Value,
    scene::{PointerEvent, PointerPhase},
};
use std::{io::Write, sync::Arc, time::Instant};
fn main() -> Result<()> {
    let output = std::env::args().nth(1).ok_or("Supply mesh output path")?;
    let dense = std::env::args().any(|a| a == "--dense");
    let args: Vec<_> = std::env::args().collect();
    let arg = |name: &str| {
        args.windows(2)
            .find(|a| a[0] == name)
            .map(|a| a[1].as_str())
    };
    let text_scale = arg("--scale")
        .unwrap_or("2")
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    let hierarchy = args.iter().any(|a| a == "--hierarchy");
    let viewport_scale = arg("--viewport-scale")
        .unwrap_or("1")
        .parse::<f32>()
        .map_err(|e| e.to_string())?;
    let viewport_scale = if hierarchy {
        1.25 * 0.66
    } else {
        viewport_scale
    };
    let origin = arg("--origin-x")
        .unwrap_or("16")
        .parse::<f32>()
        .map_err(|e| e.to_string())?;
    let measure_only = args.iter().any(|a| a == "--measure-only");
    let rebuild = args.iter().any(|a| a == "--rebuild");
    if !(1..=8).contains(&text_scale)
        || !(0.1..=4.).contains(&viewport_scale)
        || !(0. ..=32.).contains(&origin)
    {
        return Err("Invalid text probe dimensions".into());
    }
    let with_styles = args.iter().any(|a| a == "--styles");
    let retained = std::env::args().any(|a| a == "--retained");
    let math = if std::env::args().any(|a| a == "--fixed") {
        otrios_core::dashboard::retained_text::TextMath::Fixed
    } else {
        otrios_core::dashboard::retained_text::TextMath::Float32
    };
    let mut presentation = otrios_core::dashboard::retained_text::TextPresentation::with_math(math);
    let mut generation = 0;
    let frame_count = if dense { 120_u32 } else { 240_u32 };
    let font = Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
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
    for value in &atlas.curves {
        packet.extend(value.to_le_bytes());
    }
    packet.extend(&atlas.bands);
    if !measure_only {
        std::fs::write(format!("{output}.font"), packet).map_err(|e| e.to_string())?;
    }
    let (app, make_ui) = input_lab::screens().remove(0);
    let ui = make_ui(&[]);
    let bytes = encode_ui(&ui)?;
    let state = "probe:input-text";
    let mut doc = Document::new(ui.state_schema.clone(), state)?;
    let text = "The quick brown fox jumps over the lazy dog. ".repeat(5);
    doc.set_typed(&[(1, Value::Text(text.clone())), (2, Value::Text(text))])?;
    let mut client = Client::default();
    client.set_font(font.clone());
    let mut app = app;
    app.state_ref = state.into();
    client.admit_catalog(&encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![CatalogEntry {
            app,
            path: "input-lab".into(),
            ui_hash: hash(&bytes),
            private: false,
        }],
    })?)?;
    client.mount_local(&bytes, state, &[], &doc.save()?)?;
    let first = client
        .scene()?
        .controls()
        .into_iter()
        .find(|n| {
            n.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "First text field")
        })
        .ok_or("Missing field")?
        .node
        .id;
    client.activate(client.epoch(), first)?;
    client.tick(0, 0)?;
    client.pointer(
        client.epoch(),
        PointerEvent {
            id: 1,
            phase: PointerPhase::Down,
            position: [900., 220.],
        },
    )?;
    let stream: Box<dyn Write> = if measure_only {
        Box::new(std::io::sink())
    } else {
        Box::new(std::fs::File::create(output).map_err(|e| e.to_string())?)
    };
    let mut output = std::io::BufWriter::new(stream);
    output
        .write_all(if retained {
            if with_styles {
                b"OTRTEXT3"
            } else {
                b"OTRTEXT2"
            }
        } else {
            b"OTRTEXT1"
        })
        .map_err(|e| e.to_string())?;
    output
        .write_all(&frame_count.to_le_bytes())
        .map_err(|e| e.to_string())?;
    let mut times = vec![];
    let mut glyph_count = 0;
    for i in 0..u64::from(frame_count) {
        let start = Instant::now();
        client.tick(i * 16, 0)?;
        let offset = if i < 120 {
            i as f32 * 4.371
        } else {
            (240 - i) as f32 * 4.371
        };
        client.pointer(
            client.epoch(),
            PointerEvent {
                id: 1,
                phase: PointerPhase::Move,
                position: [900. - offset, 220.],
            },
        )?;
        let scene = if dense {
            use otrios_core::dashboard::{
                components::{BG, WHITE, background, canvas, text},
                scene::Scene,
            };
            use otrios_interfaces::scene::Rect;
            let mut paragraph = "The quick brown fox jumps over the lazy dog. ".repeat(5);
            if rebuild && i % 2 != 0 {
                paragraph.replace_range(..1, "t");
            }
            if hierarchy {
                paragraph.insert_str(0, "(jy) Égy ");
            }
            let offset = offset.min(
                (font.text_width(&paragraph, text_scale as f32) * viewport_scale - 1248.).max(0.),
            );
            let mut nodes = vec![background(1, 1280., 800., BG, 0.)];
            let height = 13. * text_scale as f32 * viewport_scale;
            let rows = (768. / (height + 1.)).floor().clamp(1., 64.) as u32;
            for row in 0..rows {
                let mut row_node = canvas(
                    10 + row * 2,
                    Rect::new(origin, 16. + row as f32 * (height + 1.), 1248., height),
                    vec![text(
                        11 + row * 2,
                        Rect::new(
                            -offset / viewport_scale,
                            0.,
                            ((1248. + offset) / viewport_scale).min(8192.),
                            height / viewport_scale,
                        ),
                        &paragraph,
                        text_scale,
                        WHITE,
                    )],
                );
                if viewport_scale != 1. {
                    row_node.primitive = otrios_interfaces::scene::Primitive::Viewport {
                        scale: viewport_scale,
                    };
                }
                if hierarchy {
                    use otrios_interfaces::scene::Primitive;
                    let mut child = row_node.children.remove(0);
                    child.frame.y = -1.125;
                    let mut nested = canvas(
                        1000 + row * 3,
                        Rect::new(0., 0., 1248. / 1.25, height / 1.25),
                        vec![child],
                    );
                    nested.primitive = Primitive::Viewport { scale: 0.66 };
                    row_node.primitive = Primitive::Viewport { scale: 1.25 };
                    row_node.children = vec![nested];
                    row_node.frame.x = origin - 8192.;
                    let mut small = canvas(
                        1001 + row * 3,
                        Rect::new(0.0001, 0., 1280., 800.),
                        vec![row_node],
                    );
                    small.primitive = Primitive::Canvas { clip: false };
                    let mut large = canvas(
                        1002 + row * 3,
                        Rect::new(8192., 0., 1280., 800.),
                        vec![small],
                    );
                    large.primitive = Primitive::Canvas { clip: false };
                    nodes.push(large);
                } else {
                    nodes.push(row_node);
                }
            }
            Arc::new(Scene::new(canvas(
                0,
                Rect::new(0., 0., 1280., 800.),
                nodes,
            ))?)
        } else {
            client.scene()?
        };
        let frame = retained.then(|| presentation.compose(&scene, &font));
        let flat;
        let mesh = if let Some(frame) = &frame {
            frame.mesh.as_ref()
        } else {
            flat = render_scene_with_font(&scene, &font);
            &flat
        };
        let changed = frame.as_ref().is_none_or(|f| f.generation != generation);
        let views = frame.as_ref().map_or(&[][..], |f| f.views.as_slice());
        if let Some(frame) = &frame {
            generation = frame.generation;
        }
        glyph_count += if retained {
            views.chunks_exact(8).map(|v| v[1] as usize / 4).sum()
        } else {
            mesh.glyph_vertices.len() / 64
        };
        times.push(start.elapsed().as_micros());
        if measure_only {
            continue;
        }
        if retained {
            output
                .write_all(&generation.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        for count in [
            mesh.vertices.len(),
            mesh.glyph_vertices.len(),
            mesh.draws.len(),
        ]
        .map(|n| if changed { n } else { 0 })
        {
            output
                .write_all(&(count as u32).to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        if retained {
            output
                .write_all(&(views.len() as u32).to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        if retained && with_styles {
            output
                .write_all(
                    &(if changed {
                        frame.as_ref().map_or(0, |f| f.styles.len()) as u32
                    } else {
                        0
                    })
                    .to_le_bytes(),
                )
                .map_err(|e| e.to_string())?;
        }
        for n in mesh
            .vertices
            .iter()
            .chain(&mesh.glyph_vertices)
            .take(if changed { usize::MAX } else { 0 })
        {
            output
                .write_all(&n.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        for n in mesh.draws.iter().take(if changed { usize::MAX } else { 0 }) {
            output
                .write_all(&n.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        for n in views {
            output
                .write_all(&n.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        if retained && with_styles && changed {
            for n in frame.as_ref().unwrap().styles.iter() {
                output
                    .write_all(&n.to_le_bytes())
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let cold = times[0];
    times.sort_unstable();
    println!(
        "Fixed text runs / fallback runs: {:?}",
        presentation.math_counts()
    );
    println!("Cold layout/mesh microseconds: {cold}");
    println!(
        "Synthetic layout/mesh microseconds: p50={} p95={} max={}",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() - 1]
    );
    println!(
        "Visible glyphs per frame: {}",
        glyph_count / frame_count as usize
    );
    Ok(())
}
