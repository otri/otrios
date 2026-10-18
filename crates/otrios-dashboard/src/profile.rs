// SPDX-License-Identifier: Apache-2.0
//! Headless, read-only timing probe. No display, network or state-file writes.
use otrios_core::dashboard::presentation::{Model, Presentation};
use otrios_core::dashboard::{Replica, display::cached_regions, render_scene, shell::Shell};
use otrios_interfaces::scene::{Action, PointerEvent, PointerPhase, Rect, TouchResponder};
use std::{hint::black_box, sync::Arc, time::Instant};
fn main() -> Result<(), String> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let wire_only = args.first().is_some_and(|a| a == "--wire");
    if wire_only {
        args.remove(0);
    }
    let handoff = args.first().is_some_and(|a| a == "--handoff");
    if handoff {
        args.remove(0);
    }
    let deferred = args.first().is_some_and(|a| a == "--deferred");
    if deferred {
        args.remove(0);
    }
    if args.len() > 2 {
        return Err(
            "Usage: dashboard-profile [--deferred|--handoff|--wire] [STATE_FILE [PRESENTATION_FILE]]"
                .into(),
        );
    }
    let mut replica = if let Some(path) = args.first() {
        Replica::load(&std::fs::read(path).map_err(|e| e.to_string())?)?
    } else {
        Replica::new()?
    };
    if let Some(path) = args.get(1) {
        replica.set_presentation(Some(&std::fs::read(path).map_err(|e| e.to_string())?))?;
    }
    if handoff || wire_only {
        return measure_handoff(replica, wire_only);
    }
    if deferred {
        measure(
            &mut Presentation::new(Arc::new(replica)),
            |_| {},
            "publication queued (no IO)",
        )
    } else {
        measure(
            &mut replica,
            |r| {
                black_box(r.save());
            },
            "serialize",
        )
    }
}
fn measure<M: Model>(
    replica: &mut M,
    mut save: impl FnMut(&mut M),
    storage_label: &str,
) -> Result<(), String> {
    let mut shell = Shell::default();
    shell.open_name("controls");
    let scene = shell.scene(replica)?;
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
            replica,
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
        shell.flush_controls(replica, now)?;
        totals[0] += start.elapsed().as_secs_f64();
        let start = Instant::now();
        let scene = shell.scene(replica)?;
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
        save(replica);
        totals[4] += start.elapsed().as_secs_f64();
    }
    if replica.controls()?.level != 926 {
        return Err("Replay did not update the slider to the expected endpoint".into());
    }
    println!("30 disposable slider updates; {vertices} vertices; mean milliseconds:");
    for (name, seconds) in [
        "input/project",
        "scene",
        "mesh",
        "SDL regions",
        storage_label,
    ]
    .into_iter()
    .zip(totals)
    {
        println!("{name}: {:.3}", seconds * 1000. / 30.);
    }
    Ok(())
}

fn measure_handoff(mut app: Replica, wire_only: bool) -> Result<(), String> {
    use otrios_core::dashboard::SyncSession;
    let mut network = app.clone();
    let mut server = app.clone();
    let mut ns = SyncSession::default();
    let mut ss = SyncSession::default();
    fn exchange(
        a: &mut Replica,
        sa: &mut SyncSession,
        b: &mut Replica,
        sb: &mut SyncSession,
    ) -> Result<(), String> {
        for _ in 0..32 {
            let x = a.generate_sync(sa)?;
            let y = b.generate_sync(sb)?;
            if x.is_none() && y.is_none() {
                return Ok(());
            }
            if let Some(x) = x {
                b.receive_sync(sb, &x)?;
            }
            if let Some(y) = y {
                a.receive_sync(sa, &y)?;
            }
        }
        Err("Sync did not settle".into())
    }
    exchange(&mut network, &mut ns, &mut server, &mut ss)?;
    println!(
        "Disposable replicas; {} serialized bytes; 10 remote updates",
        app.save().len()
    );
    let mut times = [0.; 4];
    for i in 0..10 {
        server.set_control(0, [100 + i * 37, 0])?;
        let begin = Instant::now();
        exchange(&mut network, &mut ns, &mut server, &mut ss)?;
        times[0] += begin.elapsed().as_secs_f64();
        let begin = Instant::now();
        let bytes = network.save();
        times[1] += begin.elapsed().as_secs_f64();
        let begin = Instant::now();
        if !wire_only {
            app.merge(&bytes)?;
        }
        times[2] += begin.elapsed().as_secs_f64();
        let begin = Instant::now();
        black_box(app.clone());
        times[3] += begin.elapsed().as_secs_f64();
    }
    if wire_only {
        println!("Single owner: full-document handoff removed");
    }
    for (name, seconds) in [
        "wire receive/ack",
        "handoff serialize copy",
        "handoff full-document decode/merge",
        "published replica clone",
    ]
    .into_iter()
    .zip(times)
    {
        println!("{name}: {:.3} ms", seconds * 1000. / 10.);
    }
    Ok(())
}
