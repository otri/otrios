// SPDX-License-Identifier: Apache-2.0
//! Separate admitted layout evaluation from geometry construction on each host.
//! Repeats bounded scenes; never raises the runtime's node/coordinate limits.
use otrios_core::dashboard::{
    font::Font,
    layout::Layout,
    render_scene_with_font,
    retained_text::{ComposeProfile, TextPresentation},
    scene::Scene,
};
use otrios_interfaces::layout::*;
use std::{collections::BTreeMap, hint::black_box, time::Instant};

fn number(value: f32) -> Expr {
    Expr::Literal(Value::Number(value))
}
fn blueprint(nodes: usize, clipped: bool, kind: &str) -> Blueprint {
    let children = (1..nodes)
        .map(|index| {
            let x =
                ((index - 1) % 16) as f32 * 44. + if clipped && index > 16 { 2000. } else { 0. };
            let y = ((index - 1) / 16) as f32 * 24.;
            let shape = if kind == "mixed" && index % 2 == 0 {
                Shape::Text {
                    value: Expr::Literal(Value::Text("Abc 123".into())),
                    scale: number(1.),
                    color: Expr::State(Binding::Local(1)),
                }
            } else {
                Shape::Superellipse {
                    radius: number(if kind == "rectangles" { 0. } else { 6. }),
                    exponent: 4.,
                    color: Expr::State(Binding::Local(1)),
                }
            };
            TemplateNode {
                reference_root: false,
                layout: None,
                id: index as u16,
                frame: [
                    Expr::Add(
                        Box::new(number(x)),
                        Box::new(Expr::State(Binding::Local(2))),
                    ),
                    number(y),
                    number(40.),
                    number(20.),
                ],
                visible: Expr::Literal(Value::Boolean(true)),
                resize: None,
                shape,
                responder: None,
                semantics: None,
                children: vec![],
            }
        })
        .collect();
    Blueprint {
        schema: "otrios-layout-v2".into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "Throughput".into(),
            parameters: vec![],
            root: TemplateNode {
                reference_root: false,
                layout: None,
                id: 0,
                frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                visible: Expr::Literal(Value::Boolean(true)),
                resize: None,
                shape: Shape::Canvas { clip: true },
                responder: None,
                semantics: None,
                children,
            },
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    }
}
fn measure(
    stage: &str,
    kind: &str,
    nodes: usize,
    clipped: bool,
    iterations: usize,
    mut operation: impl FnMut(usize),
) {
    // For alternating-color workloads warm the opposite color, so every timed
    // operation actually changes color rather than beginning with a cache hit.
    operation(usize::MAX);
    let start = Instant::now();
    for index in 0..iterations {
        operation(index);
    }
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{{\"stage\":{stage:?},\"kind\":{kind:?},\"nodes\":{nodes},\"clipped\":{clipped},\"iterations\":{iterations},\"seconds\":{seconds},\"us_per_scene\":{},\"nodes_per_second\":{}}}",
        seconds * 1_000_000. / iterations as f64,
        nodes as f64 * iterations as f64 / seconds
    );
}
fn scene_counts(scene: &Scene) -> (usize, usize) {
    fn count(node: &otrios_interfaces::scene::SceneNode) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }
    let visible = scene
        .root()
        .children
        .iter()
        .filter(|node| node.frame.x < 1280. && node.frame.y < 800.)
        .count();
    (count(scene.root()), visible)
}
fn run(nodes: usize, iterations: usize, font: &Font) -> Result<(), String> {
    for kind in ["rectangles", "rounded", "mixed"] {
        for clipped in [false, true] {
            let blueprint = blueprint(nodes, clipped, kind);
            let bytes = Layout::encode(&blueprint)?;
            let layout = Layout::from_bytes(&bytes)?;
            let mut slots = BTreeMap::from([
                (1, Value::Color([52, 211, 153, 255])),
                (2, Value::Number(0.)),
            ]);
            let first = layout.resolve_viewport(&slots, [1280., 800.])?;
            slots.insert(1, Value::Color([85, 136, 255, 255]));
            let second = layout.resolve_viewport(&slots, [1280., 800.])?;
            let (resolved_nodes, visible_leaves) = scene_counts(&first);
            assert_eq!(resolved_nodes, nodes);
            println!(
                "{{\"stage\":\"admitted_case\",\"kind\":{kind:?},\"nodes\":{nodes},\"clipped\":{clipped},\"layout_bytes\":{},\"visible_leaf_nodes\":{visible_leaves}}}",
                bytes.len()
            );
            measure("encode_and_admit", kind, nodes, clipped, 5, |_| {
                black_box(Layout::encode(&blueprint).unwrap());
            });
            measure("decode_and_admit", kind, nodes, clipped, 5, |_| {
                black_box(Layout::from_bytes(&bytes).unwrap());
            });
            measure(
                "resolve_expressions_build_scene_validate",
                kind,
                nodes,
                clipped,
                iterations * 10,
                |index| {
                    slots.insert(2, Value::Number((index % 4) as f32));
                    black_box(layout.resolve_viewport(&slots, [1280., 800.]).unwrap());
                },
            );
            measure(
                "scene_clone_and_validate",
                kind,
                nodes,
                clipped,
                iterations * 10,
                |_| {
                    black_box(Scene::new(first.root().clone()).unwrap());
                },
            );
            measure(
                "fixed_clipped_paint",
                kind,
                nodes,
                clipped,
                iterations,
                |_| {
                    black_box(render_scene_with_font(&first, font));
                },
            );
            let mut retained = TextPresentation::default();
            black_box(retained.compose(&first, font));
            measure(
                "retained_unchanged",
                kind,
                nodes,
                clipped,
                iterations,
                |_| {
                    black_box(retained.compose(&first, font));
                },
            );
            measure(
                "retained_color_change",
                kind,
                nodes,
                clipped,
                iterations,
                |index| {
                    black_box(
                        retained.compose(if index % 2 == 0 { &second } else { &first }, font),
                    );
                },
            );
            if kind == "mixed" {
                let mut changed = first.root().clone();
                let text = changed
                    .children
                    .iter_mut()
                    .find_map(|node| {
                        if let otrios_interfaces::scene::Primitive::Text { value, .. } =
                            &mut node.primitive
                        {
                            Some(value)
                        } else {
                            None
                        }
                    })
                    .ok_or("Missing mixed-case text")?;
                *text = "Confirmed".into();
                let changed = Scene::new(changed)?;
                let mut presentation = TextPresentation::default();
                let before = presentation.compose(&first, font);
                let after = presentation.compose(&changed, font);
                let changed_resources: Vec<_> = after
                    .chunks
                    .iter()
                    .filter(|chunk| {
                        !before
                            .chunks
                            .iter()
                            .any(|old| std::sync::Arc::ptr_eq(&old.resource, &chunk.resource))
                    })
                    .collect();
                let upload_bytes: usize = changed_resources
                    .iter()
                    .map(|chunk| {
                        (chunk.resource.mesh.vertices.len()
                            + chunk.resource.mesh.glyph_vertices.len())
                            * 4
                    })
                    .sum();
                println!(
                    "{{\"stage\":\"single_label_resources\",\"kind\":{kind:?},\"nodes\":{nodes},\"clipped\":{clipped},\"resources\":{},\"changed_resources\":{},\"changed_vertex_bytes\":{upload_bytes}}}",
                    after.chunks.len(),
                    changed_resources.len()
                );
                measure(
                    "retained_single_label_change",
                    kind,
                    nodes,
                    clipped,
                    iterations,
                    |index| {
                        black_box(
                            presentation
                                .compose(if index % 2 == 0 { &first } else { &changed }, font),
                        );
                    },
                );
                let mut rebound = first.root().clone();
                for child in &mut rebound.children {
                    child.id += 10000;
                }
                let rebound = Scene::new(rebound)?;
                let mut owner = TextPresentation::default();
                let before = owner.compose(&first, font);
                let after = owner.compose(&rebound, font);
                let changed = after
                    .chunks
                    .iter()
                    .filter(|chunk| {
                        !before
                            .chunks
                            .iter()
                            .any(|old| std::sync::Arc::ptr_eq(&old.resource, &chunk.resource))
                    })
                    .count();
                println!(
                    "{{\"stage\":\"identity_rebind_resources\",\"kind\":{kind:?},\"nodes\":{nodes},\"clipped\":{clipped},\"resources\":{},\"changed_resources\":{changed}}}",
                    after.chunks.len()
                );
                measure(
                    "retained_identity_change",
                    kind,
                    nodes,
                    clipped,
                    iterations,
                    |index| {
                        black_box(
                            owner.compose(if index % 2 == 0 { &first } else { &rebound }, font),
                        );
                    },
                );
            }
            let mut retained = TextPresentation::default();
            let mut profile = ComposeProfile::default();
            let frame = retained.compose_profiled(&first, font, &mut profile);
            let visible_runs = frame
                .views
                .chunks_exact(8)
                .filter(|view| view[1] > 0. && view[6] > view[4] && view[7] > view[5])
                .count();
            let fixed = render_scene_with_font(&first, font);
            println!(
                "{{\"stage\":\"cold_retained_breakdown\",\"kind\":{kind:?},\"nodes\":{nodes},\"clipped\":{clipped},\"normalize_us\":{},\"rebuild_us\":{},\"cull_us\":{},\"triangle_vertices\":{},\"glyph_vertices\":{},\"draws\":{},\"visible_runs\":{visible_runs},\"fixed_triangle_vertices\":{},\"fixed_glyph_vertices\":{},\"mesh_bytes\":{},\"view_bytes\":{}}}",
                profile.placement_us,
                profile.rebuild_us,
                profile.cull_us,
                frame.mesh.vertices.len() / 6,
                frame.mesh.glyph_vertices.len() / 16,
                frame.mesh.draws.len() / 3,
                fixed.vertices.len() / 6,
                fixed.glyph_vertices.len() / 16,
                profile.mesh_payload_bytes,
                profile.viewport_bytes
            );
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let option = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| &pair[1])
    };
    let counts = option("--nodes").map_or_else(
        || Ok(vec![32, 128, 256]),
        |value| value.parse::<usize>().map(|count| vec![count]),
    )?;
    let iterations = option("--iterations").map_or(Ok(20), |value| value.parse::<usize>())?;
    if counts.iter().any(|count| !(2..=512).contains(count)) || !(2..=10000).contains(&iterations) {
        return Err(
            "Use 2–512 nodes and 2–10000 iterations; all admission limits remain active".into(),
        );
    }
    let font = Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )?;
    println!(
        "{{\"format\":\"otrios-layout-throughput-v1\",\"architecture\":{:?},\"os\":{:?},\"measurement\":\"bounded synthetic layouts; no OIB compilation, CRDT, IO, GPU upload or presentation in resolve stage\"}}",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    for count in counts {
        run(count, iterations, &font)?;
    }
    Ok(())
}
