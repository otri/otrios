// SPDX-License-Identifier: LGPL-3.0-only
//! CPU-only coordinate probe. Compare identical inputs and inspect generated code.
use otrios_core::dashboard::fixed::{Translation, add4};
use otrios_core::dashboard::layout::resizing::{resolve_fixed, resolve_float32};
use otrios_interfaces::layout::{AxisResize, ResizeRule};
use std::{hint::black_box, time::Instant};
#[inline(never)]
fn float_batch(input: &[[f32; 4]], delta: [f32; 4], output: &mut [[f32; 4]]) {
    for (a, out) in input.iter().zip(output) {
        *out = std::array::from_fn(|i| a[i] + delta[i]);
    }
}
#[inline(never)]
fn fixed_scalar(input: &[[i32; 4]], delta: [i32; 4], output: &mut [[i32; 4]]) {
    for (a, out) in input.iter().zip(output) {
        for i in 0..4 {
            out[i] = a[i]
                .checked_add(delta[i])
                .expect("Admitted benchmark bounds");
        }
    }
}
#[inline(never)]
fn fixed_lanes(input: &[[i32; 4]], delta: [i32; 4], output: &mut [[i32; 4]]) {
    for (a, out) in input.iter().zip(output) {
        *out = add4(*a, delta).expect("Admitted benchmark bounds");
    }
}
#[inline(never)]
fn fixed_admitted(input: &Translation<'_>, output: &mut [[i32; 4]]) {
    input.write(output).unwrap();
}
fn main() {
    let input: Vec<[i32; 4]> = (0..2048)
        .map(|i| [i * 65536, -i * 65536, (i + 10) * 65536, (-i + 20) * 65536])
        .collect();
    let floats: Vec<_> = input.iter().map(|q| q.map(|v| v as f32 / 65536.)).collect();
    let delta = [
        7 * 65536 + 32768,
        -65536 - 16384,
        7 * 65536 + 32768,
        -65536 - 16384,
    ];
    let fd = delta.map(|v| v as f32 / 65536.);
    let mut integer = vec![[0; 4]; input.len()];
    let mut float = vec![[0.; 4]; input.len()];
    let admission = Instant::now();
    let admitted = Translation::admit(&input, delta).unwrap();
    println!(
        "Admission for 2048 quads: {} ns",
        admission.elapsed().as_nanos()
    );
    for mode in ["float32", "fixed-scalar", "fixed-lanes", "fixed-admitted"] {
        let mut samples = vec![];
        for _ in 0..9 {
            let begin = Instant::now();
            for _ in 0..1000 {
                match mode {
                    "float32" => {
                        float_batch(black_box(&floats), black_box(fd), black_box(&mut float))
                    }
                    "fixed-scalar" => {
                        fixed_scalar(black_box(&input), black_box(delta), black_box(&mut integer))
                    }
                    "fixed-lanes" => {
                        fixed_lanes(black_box(&input), black_box(delta), black_box(&mut integer))
                    }
                    _ => fixed_admitted(black_box(&admitted), black_box(&mut integer)),
                }
            }
            samples.push(begin.elapsed().as_nanos());
        }
        samples.sort_unstable();
        println!(
            "{{\"mode\":\"{mode}\",\"quads\":2048,\"iterations\":1000,\"samples\":9,\"median_ns\":{},\"ns_per_quad\":{}}}",
            samples[4],
            samples[4] as f64 / 2048000.
        );
    }
    for (a, b) in float.iter().zip(&integer) {
        for i in 0..4 {
            assert_eq!(a[i], b[i] as f32 / 65536.);
        }
    }
    println!(
        "Coordinate outputs agree; this is a CPU kernel benchmark, not layout or display FPS."
    );
    // Includes contract admission, both axes, checked wide products/divides and
    // presentation conversion. This prevents the add-only kernel hiding costs.
    for axis in [
        AxisResize::Pins {
            leading: Some(16.25),
            trailing: Some(24.75),
        },
        AxisResize::Springs {
            leading: true,
            length: true,
            trailing: true,
        },
    ] {
        let rule = ResizeRule {
            reference_size: [1280., 800.],
            horizontal: axis,
            vertical: axis,
        };
        for (name, run) in [
            ("float32", resolve_float32 as fn(_, _, _) -> _),
            ("fixed", resolve_fixed),
        ] {
            let mut times = vec![];
            for _ in 0..9 {
                let start = Instant::now();
                for i in 0..10000 {
                    black_box(
                        run(
                            black_box([16.25, 24.75, 320.125, 100.5]),
                            black_box([1280. + (i % 64) as f32, 800.]),
                            black_box(rule),
                        )
                        .unwrap(),
                    );
                }
                times.push(start.elapsed().as_nanos());
            }
            times.sort_unstable();
            println!(
                "{{\"layout\":\"{axis:?}\",\"mode\":\"{name}\",\"iterations\":10000,\"samples\":9,\"ns_per_frame\":{}}}",
                times[4] as f64 / 10000.
            );
        }
    }
    use otrios_core::dashboard::layout::composition;
    use otrios_interfaces::layout::{Alignment, Composition, Flow, FlowKind, FlowPlan};
    let plan = FlowPlan {
        insets: [0.125, 0.25, 0.375, 0.5],
        content: Flow {
            flexible: [true, false],
            kind: FlowKind::Group {
                axis: 0,
                gap: 10.125,
                alignment: Alignment::Center,
                children: (0..8)
                    .map(|node| Flow {
                        flexible: [true, false],
                        kind: FlowKind::Item {
                            node,
                            preferred: [120.25, 100.125],
                            minimum: [80.125, 100.125],
                        },
                    })
                    .collect(),
            },
        },
    };
    let plan = Composition {
        wide_min_width: 600.,
        wide: plan.clone(),
        tall: plan,
    };
    for (name, run) in [
        ("float32", composition::resolve_float32 as fn(_, _) -> _),
        ("fixed", composition::resolve_fixed),
    ] {
        let mut times = vec![];
        for _ in 0..9 {
            let start = Instant::now();
            for i in 0..10000 {
                black_box(
                    run(black_box(&plan), black_box([1280. + (i % 64) as f32, 800.])).unwrap(),
                );
            }
            times.push(start.elapsed().as_nanos());
        }
        times.sort_unstable();
        println!(
            "{{\"layout\":\"flow-eight-items\",\"mode\":\"{name}\",\"iterations\":10000,\"samples\":9,\"ns_per_frame\":{}}}",
            times[4] as f64 / 10000.
        );
    }
}
