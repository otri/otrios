// SPDX-License-Identifier: Apache-2.0
//! Mali-400 regression: exact pixels after cache reuse and draw-plan resizing.
#[cfg(target_os = "linux")]
mod linux {
    use otrios_core::dashboard::{
        components::{canvas, node, text},
        display::Surface,
        font::Font,
        font_reference, render_scene_with_font,
        scene::Scene,
    };
    use otrios_interfaces::scene::{Primitive, Rect};
    use otrios_linux_display::Display;
    pub fn run() -> Result<(), String> {
        let font = Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            font_reference::RESOURCE_SHA256,
        )?;
        let atlas = font.atlas();
        let mut cases = 0;
        for rotation in [0, 90] {
            let surface = Surface::new(rotation)?;
            let [w, h] = surface.extent();
            let mut display = Display::headless([w, h], false)?;
            display.set_font(
                atlas.curve_width,
                atlas.curve_height,
                &atlas.curves,
                atlas.band_width,
                atlas.band_height,
                &atlas.bands,
            )?;
            let scene = |extra: bool, changed: bool, turn: u32| -> Result<Scene, String> {
                let mut children = vec![
                    node(
                        2,
                        Rect::new(0., 0., 1280., 800.),
                        Primitive::Superellipse {
                            radius: 0.,
                            exponent: 4.,
                            color: [23, 27, 32, 255],
                        },
                    ),
                    text(
                        3,
                        Rect::new(40., 40., 1000., 80.),
                        if changed {
                            "Brass cache: jgyÉ 67890"
                        } else {
                            "Brass cache: jgyÉ 12345"
                        },
                        3,
                        [239, 241, 237, 255],
                    ),
                ];
                if extra {
                    children.push(node(
                        4,
                        Rect::new(40. + turn as f32, 220., 40., 40.),
                        Primitive::Superellipse {
                            radius: 0.,
                            exponent: 4.,
                            color: [216, 183, 117, 255],
                        },
                    ));
                }
                Scene::new(canvas(1, Rect::new(0., 0., 1280., 800.), children))
            };
            for turn in 0_u32..20 {
                let mesh = render_scene_with_font(
                    &scene(turn % 4 < 2, turn.is_multiple_of(3), turn)?,
                    &font,
                );
                let views = mesh
                    .draws
                    .chunks_exact(3)
                    .flat_map(|d| {
                        if d[0] == 1 {
                            [d[1] as f32, d[2] as f32, 0., 0., 0., 0., 1280., 800.]
                        } else {
                            [0.; 8]
                        }
                    })
                    .collect::<Vec<_>>();
                display.upload_scene(0, &mesh.vertices, &mesh.glyph_vertices, &mesh.draws)?;
                display.text_styles(
                    0,
                    &vec![if turn % 4 < 2 { 32. } else { 48. }; mesh.draws.len() / 3],
                )?;
                display.text_views(0, &views)?;
                display.draw_slots(surface.projection(), &[0])?;
                let retained = display.pixels()?;
                display.retire_slot(1)?;
                display.upload_scene(1, &mesh.vertices, &mesh.glyph_vertices, &mesh.draws)?;
                display.text_styles(
                    1,
                    &vec![if turn % 4 < 2 { 32. } else { 48. }; mesh.draws.len() / 3],
                )?;
                display.text_views(1, &views)?;
                display.draw_slots(surface.projection(), &[1])?;
                let fresh = display.pixels()?;
                if retained != fresh {
                    return Err(format!(
                        "Cached/fresh pixel mismatch at rotation {rotation}, turn {turn}"
                    ));
                }
                cases += 1;
            }
            // A geometry-only upload must retire the prior glyph plan.
            let mesh = render_scene_with_font(&scene(false, false, 0)?, &font);
            display.upload(0, &mesh.vertices)?;
            display.retire_slot(1)?;
            display.upload(1, &mesh.vertices)?;
            display.draw_slots(surface.projection(), &[0])?;
            let cleared = display.pixels()?;
            display.draw_slots(surface.projection(), &[1])?;
            if cleared != display.pixels()? {
                return Err("Geometry-only upload retained stale glyphs".into());
            }
            cases += 1;
        }
        println!(
            "{{\"passed\":true,\"exact_pixel_cases\":{cases},\"checks\":[\"same glyph reuse\",\"changed glyph invalidation\",\"style changes\",\"geometry-only text retirement\",\"growing and shrinking viewport draw plans\",\"rotations 0 and 90\"]}}"
        );
        Ok(())
    }
}
fn main() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("This probe requires Linux GLES2".into())
    }
}
