// SPDX-License-Identifier: LGPL-3.0-only
//! Sixty-four small clockwork emblems, generated as shared vector primitives.
use crate::dashboard::components::{canvas, node};
use otrios_interfaces::scene::*;
pub const BG: Color = [23, 27, 32, 255];
pub const CARD: Color = [35, 40, 46, 255];
pub const TEXT: Color = [239, 241, 237, 255];
pub const MUTED: Color = [172, 179, 187, 255];
pub const BRASS: Color = [216, 183, 117, 255];
pub const COPPER: Color = [208, 170, 137, 255];
pub const LINE: Color = [66, 73, 81, 255];
pub fn name(id: u8) -> String {
    let family = [
        "Clockwork bird",
        "Airship",
        "Diving helmet",
        "Brass goggles",
        "Piston teapot",
        "Automaton",
        "Winding beetle",
        "Pocket watch",
    ][(id / 8) as usize % 8];
    let variation = [
        "Courier",
        "Admiral",
        "Professor",
        "Midnight",
        "Workshop",
        "Explorer",
        "Copper",
        "Grand",
    ][(id % 8) as usize];
    format!("{variation} {family}")
}
pub fn avatar(id: u8, base: u32, size: f32) -> SceneNode {
    let v = f32::from(id % 8);
    let mut paths: Vec<Vec<[f32; 2]>> = vec![];
    let ring = |x: f32, y: f32, r: f32| {
        (0..=16)
            .map(|n| {
                let a = n as f32 * std::f32::consts::TAU / 16.;
                [x + a.cos() * r, y + a.sin() * r]
            })
            .collect::<Vec<_>>()
    };
    match id / 8 {
        0 => {
            paths.push(vec![
                [5., 17.],
                [7., 10. - v * 0.2],
                [13., 11.],
                [16., 6.],
                [20., 8.],
                [17., 10.],
                [16., 16.],
                [10., 19.],
                [5., 17.],
                [3., 19.],
            ]);
            paths.push(vec![[8., 12.], [12., 14. + v * 0.4], [8., 16.]]);
            paths.push(vec![[11., 18.], [10., 22.], [14., 22.]]);
        }
        1 => {
            paths.push(
                (0..=20)
                    .map(|n| {
                        let a = n as f32 * std::f32::consts::TAU / 20.;
                        [12. + a.cos() * 10., 9. + a.sin() * (4. + v * 0.25)]
                    })
                    .collect(),
            );
            paths.push(vec![[7., 13.], [9., 18.], [15., 18.], [17., 13.]]);
            paths.push(vec![[2., 9.], [5., 5.], [5., 13.], [2., 9.]]);
            paths.push(vec![[8., 8. + v * 0.2], [17., 8. + v * 0.2]]);
        }
        2 => {
            paths.push(ring(12., 11., 8.));
            paths.push(ring(12., 11., 4. + v * 0.2));
            paths.push(vec![[7., 18.], [5., 22.], [19., 22.], [17., 18.]]);
            paths.push(vec![[3., 9.], [1., 9.], [1., 14.], [4., 14.]]);
            paths.push(vec![[21., 9.], [23., 9.], [23., 14.], [20., 14.]]);
        }
        3 => {
            paths.push(ring(6.5, 12., 4. + v * 0.12));
            paths.push(ring(17.5, 12., 4. + v * 0.12));
            paths.push(vec![[10., 12.], [14., 12.]]);
            paths.push(vec![
                [2., 9.],
                [1., 7. - v * 0.4],
                [23., 7. - v * 0.4],
                [22., 9.],
            ]);
            paths.push(vec![[4., 11.], [7., 9.], [8., 13.]]);
        }
        4 => {
            paths.push(vec![
                [7., 9.],
                [18., 9.],
                [18., 18.],
                [16., 21.],
                [8., 21.],
                [5., 18.],
                [5., 12.],
                [2., 8.],
                [7., 12.],
            ]);
            paths.push(vec![
                [18., 10.],
                [22., 10.],
                [23., 14.],
                [21., 17.],
                [18., 17.],
            ]);
            paths.push(vec![[8., 7.], [16., 7.], [12., 4.], [12., 2. - v * 0.1]]);
            paths.push(vec![[9. + v * 0.5, 12.], [9. + v * 0.5, 18.]]);
        }
        5 => {
            paths.push(vec![[5., 7.], [19., 7.], [20., 19.], [4., 19.], [5., 7.]]);
            paths.push(vec![[12., 7.], [12., 3.], [8. + v, 3.]]);
            paths.push(ring(8., 12., 1.5));
            paths.push(ring(16., 12., 1.5));
            paths.push(vec![
                [8., 16.],
                [16., 16.],
                [16., 17. + v * 0.15],
                [8., 17. + v * 0.15],
                [8., 16.],
            ]);
        }
        6 => {
            paths.push(ring(12., 13., 6.));
            paths.push(vec![[12., 7.], [12., 19.]]);
            paths.push(vec![[7., 7.], [5., 3. + v * 0.3], [3., 2.]]);
            paths.push(vec![[17., 7.], [19., 3. + v * 0.3], [21., 2.]]);
            for y in [10., 14., 18.] {
                paths.push(vec![[6., y], [2., y + 2. + v * 0.12]]);
                paths.push(vec![[18., y], [22., y + 2. + v * 0.12]]);
            }
        }
        _ => {
            paths.push(ring(12., 13., 8.));
            paths.push(vec![[9., 5.], [9., 2.], [15., 2.], [15., 5.]]);
            let angle = v * std::f32::consts::TAU / 8.;
            paths.push(vec![
                [12., 7.],
                [12., 13.],
                [12. + angle.sin() * 5., 13. - angle.cos() * 5.],
            ]);
            paths.push(vec![[5., 5.], [3., 3.], [1., 5.]]);
        }
    }
    let factor = size / 28.;
    let mut children = vec![node(
        base + 1,
        Rect::new(0., 0., size, size),
        Primitive::Circle { color: CARD },
    )];
    for (i, points) in paths.into_iter().enumerate() {
        children.push(node(
            base + 2 + i as u32,
            Rect::new(0., 0., size, size),
            Primitive::Path {
                points: points
                    .into_iter()
                    .map(|[x, y]| [(x + 2.) * factor, (y + 2.) * factor])
                    .collect(),
                width: (1.25 * factor).max(0.5),
                color: if id.is_multiple_of(2) { BRASS } else { COPPER },
            },
        ));
    }
    canvas(base, Rect::new(0., 0., size, size), children)
}
