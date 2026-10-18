// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;
pub fn description() -> Ui {
    let apps = [
        ("component-playground", "Component Playground"),
        ("layout-lab", "Layout Lab"),
        ("input-lab", "Input Lab"),
        ("animation-lab", "Animation Lab"),
        ("three-d-lab", "3D Lab"),
        ("monitoring", "Monitoring"),
    ];
    AppUi::new("home", "Home", schema("home-state-v2", &[])).build(
        "OTriOS / Apps",
        apps.into_iter()
            .enumerate()
            .map(|(i, (id, title))| {
                let mut children = vec![
                    rounded_rect(CARD).pin_edges(0.),
                    text(title).frame([24., 50., 490., 40.]).font_scale(3.),
                ];
                if id == "monitoring" {
                    children.push(
                        canvas(vec![
                            circle(WHITE).frame([4., 0., 24., 24.]),
                            circle(CARD).frame([9., 5., 14., 14.]),
                            rounded_rect(WHITE)
                                .frame([0., 14., 32., 24.])
                                .corner_radius(6.0.into()),
                            circle(BG).frame([13., 21., 6., 6.]),
                        ])
                        .frame([490., 16., 32., 38.]),
                    );
                }
                canvas(children)
                    .frame([
                        55. + (i % 2) as f32 * 605.,
                        160. + (i / 2) as f32 * 195.,
                        550.,
                        165.,
                    ])
                    .semantic_label(title)
                    .navigate_to(Destination::app(id))
            })
            .collect(),
    )
}
