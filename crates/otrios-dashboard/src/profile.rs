// SPDX-License-Identifier: Apache-2.0
//! Headless, read-only timing probe. No display, network or state-file writes.
use otrios_core::dashboard::{Replica, display::cached_regions, render_scene, shell::Shell};
use otrios_interfaces::scene::{Action, PointerEvent, PointerPhase, Rect, TouchResponder};
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() > 2 {
        return Err("Usage: dashboard-profile [STATE_FILE [PRESENTATION_FILE]]".into());
    }
    let mut replica = if let Some(path) = args.first() {
        Replica::load(&std::fs::read(path).map_err(|e| e.to_string())?)?
    } else {
        Replica::new()?
    };
    if let Some(path) = args.get(1) {
        replica.set_presentation(Some(&std::fs::read(path).map_err(|e| e.to_string())?))?;
    }
    let mut shell = Shell::default();
    shell.open_name("controls");
    let scene = shell.scene(&replica)?;
    let control = scene
        .controls()
        .into_iter()
        .find(|c| c.node.responder == Some(TouchResponder::Drag(Action::Control(0))))
        .ok_or("Missing slider")?;
    let mut totals = [0_f64; 5];
    let mut vertices = 0;
    for i in 0..30 {
        let now = i * 60;
        let start = Instant::now();
        shell.pointer(
            &mut replica,
            PointerEvent {
                id: 1,
                phase: if i == 0 {
                    PointerPhase::Down
                } else if i == 29 {
                    PointerPhase::Up
                } else {
                    PointerPhase::Move
                },
                position: [
                    control.origin[0] + (20. + i as f32 * 9.) * control.scale,
                    control.origin[1] + 40. * control.scale,
                ],
            },
            now,
        )?;
        shell.flush_controls(&mut replica, now)?;
        totals[0] += start.elapsed().as_secs_f64();
        let start = Instant::now();
        let scene = shell.scene(&replica)?;
        totals[1] += start.elapsed().as_secs_f64();
        let start = Instant::now();
        let mesh = render_scene(&scene);
        totals[2] += start.elapsed().as_secs_f64();
        vertices = mesh.vertices.len() / 6;
        let start = Instant::now();
        black_box(cached_regions(
            &mesh.vertices,
            Rect::new(0., 0., 1280., 800.),
            [16, 28, 41, 255],
        ));
        totals[3] += start.elapsed().as_secs_f64();
        let start = Instant::now();
        black_box(replica.save());
        totals[4] += start.elapsed().as_secs_f64();
    }
    if replica.controls()?.level != 926 {
        return Err("Replay did not update the slider to the expected endpoint".into());
    }
    println!("30 disposable slider updates; {vertices} vertices; mean milliseconds:");
    for (name, seconds) in ["input/project", "scene", "mesh", "SDL regions", "serialize"]
        .into_iter()
        .zip(totals)
    {
        println!("{name}: {:.3}", seconds * 1000. / 30.);
    }
    Ok(())
}
