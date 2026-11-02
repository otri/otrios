// SPDX-License-Identifier: Apache-2.0
//! Attended, temporary native touch measurement. Never applies device configuration.
#[cfg(target_os = "linux")]
fn main() -> Result<(), String> {
    use otrios_core::dashboard::{
        components::{canvas, node, text},
        display::{Calibration, Surface},
        font::Font,
        font_reference, render_scene_with_font,
        scene::Scene,
    };
    use otrios_interfaces::scene::{Primitive, Rect};
    use otrios_linux_display::{Display, TouchInput};
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let output = std::env::args()
        .nth(1)
        .ok_or("Supply candidate JSON output path")?;
    let surface = Surface::new(90)?;
    let font = Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?;
    let mut display = Display::open_graphics("/dev/dri/card0", surface.extent(), true)?;
    let atlas = font.atlas();
    display.set_font(
        atlas.curve_width,
        atlas.curve_height,
        &atlas.curves,
        atlas.band_width,
        atlas.band_height,
        &atlas.bands,
    )?;
    let mut input = TouchInput::open("/dev/input/by-path/platform-2005a000.i2c-event")?;
    let targets = [
        [128., 80.],
        [1152., 80.],
        [1152., 720.],
        [128., 720.],
        [640., 400.],
        [48., 400.],
        [1232., 400.],
    ];
    let mut points = Vec::new();
    let mut verification = Vec::new();
    let mut coefficients = None;
    for (index, target) in targets.iter().enumerate() {
        let label = if index < 5 {
            format!("Calibration {} of 5: hold the cross, then lift", index + 1)
        } else {
            format!("Edge check {} of 2: hold the cross, then lift", index - 4)
        };
        let cross = [255, 190, 64, 255];
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            vec![
                node(
                    1,
                    Rect::new(0., 0., 1280., 800.),
                    Primitive::Superellipse {
                        radius: 0.,
                        exponent: 2.,
                        color: [21, 35, 51, 255],
                    },
                ),
                text(2, Rect::new(220., 180., 900., 48.), &label, 2, [255; 4]),
                text(
                    3,
                    Rect::new(220., 235., 900., 48.),
                    "Touch the visible cross with your finger, not a pointer marker.",
                    1,
                    [220, 230, 240, 255],
                ),
                node(
                    4,
                    Rect::new(target[0] - 32., target[1] - 1., 64., 2.),
                    Primitive::Superellipse {
                        radius: 0.,
                        exponent: 2.,
                        color: cross,
                    },
                ),
                node(
                    5,
                    Rect::new(target[0] - 1., target[1] - 32., 2., 64.),
                    Primitive::Superellipse {
                        radius: 0.,
                        exponent: 2.,
                        color: cross,
                    },
                ),
                text(
                    6,
                    Rect::new(target[0] - 8., target[1] + 36., 64., 24.),
                    &(index + 1).to_string(),
                    2,
                    cross,
                ),
            ],
        ))?;
        let mesh = render_scene_with_font(&scene, &font);
        display.upload_scene(0, &mesh.vertices, &mesh.glyph_vertices, &mesh.draws)?;
        display.draw(surface.projection(), false)?;
        display.present()?;
        println!("TARGET {}", index + 1);
        let start = Instant::now();
        let mut contact = Vec::new();
        let raw = loop {
            if start.elapsed() > Duration::from_secs(120) {
                return Err("Target timed out; original calibration remains unchanged".into());
            }
            let mut completed = None;
            for e in input.read()? {
                match e.phase {
                    0 => {
                        contact.clear();
                        contact.push((
                            start.elapsed().as_millis() as u64,
                            [f64::from(e.x), f64::from(e.y)],
                        ));
                    }
                    1 => {
                        if contact.len() == 1024 {
                            contact.remove(0);
                        }
                        contact.push((
                            start.elapsed().as_millis() as u64,
                            [f64::from(e.x), f64::from(e.y)],
                        ));
                    }
                    2 if !contact.is_empty() => {
                        // Exclude acquisition and pressure collapse around lift.
                        // Median held samples resist a finger rolling off the cross.
                        completed = Some(held_position(&contact)?);
                        contact.clear();
                    }
                    3 => contact.clear(),
                    _ => {}
                }
            }
            if let Some(p) = completed {
                break p;
            }
            thread::sleep(Duration::from_millis(2));
        };
        if index < 5 {
            points.push(raw);
            save(&output, &points, &targets[..5], coefficients, &verification)?;
            if index == 4 {
                let physical: Vec<_> = targets[..5]
                    .iter()
                    .map(|p| {
                        let q = surface.project(*p);
                        [f64::from(q[0]) / 800., f64::from(q[1]) / 1280.]
                    })
                    .collect();
                let matrix = fit(&points, &physical)?;
                Calibration::new(matrix.map(|v| v as f32))?;
                let holdout = fit(&points[..4], &physical[..4])?;
                let center = surface
                    .touch(
                        Calibration::new(holdout.map(|v| v as f32))?,
                        points[4].map(|v| v as f32),
                    )
                    .ok_or("Invalid center holdout")?;
                if (center[0] - 640.).hypot(center[1] - 400.) > 15. {
                    return Err("Center holdout exceeds 15px; calibration unchanged".into());
                }
                let max_error = points
                    .iter()
                    .zip(&targets[..5])
                    .map(|(p, t)| {
                        let q = surface
                            .touch(
                                Calibration::new(matrix.map(|v| v as f32)).expect("validated"),
                                p.map(|v| v as f32),
                            )
                            .expect("finite");
                        (q[0] - t[0]).hypot(q[1] - t[1])
                    })
                    .fold(0_f32, f32::max);
                if max_error > 15. {
                    return Err(format!(
                        "Fit residual {max_error:.1}px exceeds 15px; calibration unchanged"
                    ));
                }
                coefficients = Some(matrix);
                save(&output, &points, &targets[..5], coefficients, &verification)?;
                println!("FIT residual {max_error:.1}px; checking edges");
            }
        } else {
            let matrix = coefficients.ok_or("Missing fit")?;
            let q = surface
                .touch(
                    Calibration::new(matrix.map(|v| v as f32))?,
                    raw.map(|v| v as f32),
                )
                .ok_or("Invalid verification sample")?;
            let error = (q[0] - target[0]).hypot(q[1] - target[1]);
            verification.push(
                serde_json::json!({"target":target,"raw":raw,"mapped":q,"error_pixels":error}),
            );
            save(&output, &points, &targets[..5], coefficients, &verification)?;
            println!("EDGE error {error:.1}px; recording both checks without applying");
        }
        thread::sleep(Duration::from_millis(300));
    }
    let passed = verification
        .iter()
        .all(|v| v["error_pixels"].as_f64().is_some_and(|e| e <= 24.));
    println!("MEASURED; edge checks passed: {passed}; device configuration unchanged");
    Ok(())
}
#[cfg(target_os = "linux")]
fn save(
    output: &str,
    points: &[[f64; 2]],
    targets: &[[f32; 2]],
    matrix: Option<[f64; 6]>,
    verification: &[serde_json::Value],
) -> Result<(), String> {
    let candidate = serde_json::json!({"format":"otrios-touch-calibration-measurement-v1","matrix":matrix,"raw":points,"targets":targets,"verification":verification,"applied":false});
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&candidate).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This attended display measurement runs on Linux Frame.");
}

#[cfg(any(target_os = "linux", test))]
fn held_position(samples: &[(u64, [f64; 2])]) -> Result<[f64; 2], String> {
    let first = samples.first().ok_or("Empty contact")?.0;
    let last = samples.last().ok_or("Empty contact")?.0;
    // A stationary device may emit no move after Down. Its unchanged position
    // remains valid; the caller already excludes the Up sample.
    let stable: Vec<_> = samples
        .iter()
        .filter(|(t, _)| {
            last.saturating_sub(first) < 400 || (*t >= first + 100 && *t + 100 <= last)
        })
        .map(|(_, p)| *p)
        .collect();
    let mut out = [0.; 2];
    for axis in 0..2 {
        let mut values: Vec<_> = stable.iter().map(|p| p[axis]).collect();
        if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
            return Err("Invalid held touch samples".into());
        }
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        out[axis] = if values.len() % 2 == 0 {
            (values[middle - 1] + values[middle]) / 2.
        } else {
            values[middle]
        };
    }
    Ok(out)
}

#[cfg(any(target_os = "linux", test))]
fn fit(points: &[[f64; 2]], targets: &[[f64; 2]]) -> Result<[f64; 6], String> {
    if points.len() != targets.len()
        || points.len() < 3
        || points
            .iter()
            .chain(targets)
            .flatten()
            .any(|n| !n.is_finite())
    {
        return Err("Invalid measurement points".into());
    }
    let mut out = [0.; 6];
    for axis in 0..2 {
        let mut a = [[0.; 4]; 3];
        for (p, t) in points.iter().zip(targets) {
            let p = [p[0], p[1], 1.];
            for i in 0..3 {
                for j in 0..3 {
                    a[i][j] += p[i] * p[j];
                }
                a[i][3] += p[i] * t[axis];
            }
        }
        for col in 0..3 {
            let pivot = (col..3)
                .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
                .ok_or("Missing pivot")?;
            a.swap(col, pivot);
            let divisor = a[col][col];
            if divisor.abs() < 1e-10 {
                return Err("Degenerate calibration points".into());
            }
            for j in col..4 {
                a[col][j] /= divisor;
            }
            for i in 0..3 {
                if i != col {
                    let scale = a[i][col];
                    for j in col..4 {
                        a[i][j] -= scale * a[col][j];
                    }
                }
            }
        }
        for i in 0..3 {
            out[axis * 3 + i] = a[i][3];
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_samples_exclude_acquisition_and_lift_roll() {
        let mut samples = vec![(0, [0.1, 0.2])];
        for t in (100..=900).step_by(100) {
            samples.push((t, [0.4, 0.6]));
        }
        samples.push((1000, [0.8, 0.9]));
        assert_eq!(super::held_position(&samples).unwrap(), [0.4, 0.6]);
        assert_eq!(
            super::held_position(&[(0, [0.4, 0.6])]).unwrap(),
            [0.4, 0.6]
        );
        assert!(super::held_position(&[]).is_err());
    }
    #[test]
    fn fit_recovers_reversed_rotated_axes_and_rejects_degenerate_input() {
        let p = [[0., 0.], [1., 0.], [1., 1.], [0., 1.], [0.5, 0.5]];
        let t = p.map(|[x, y]| [0.1 + 1.5 * y, 0.98 - 1.4 * x]);
        let m = fit(&p, &t).unwrap();
        for (a, b) in m.into_iter().zip([0., 1.5, 0.1, -1.4, 0., 0.98]) {
            assert!((a - b).abs() < 1e-8);
        }
        assert!(fit(&[[0., 0.]; 5], &t).is_err());
    }
}
