// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;

fn launch_card(id: &str, title: &str, detail: &str, caption: &str) -> Element {
    let mut children = vec![
        rounded_rect(CARD).pin_edges(0.).corner_radius(20.0.into()),
        rectangle(if id == "monitoring" { MUTED } else { ACCENT }).frame([24., 24., 36., 4.]),
        text(title)
            .frame([24., 52., 336., 40.])
            .font_scale(2.)
            .pin_left(24.)
            .pin_right(24.),
        text(detail)
            .frame([24., 110., 336., 30.])
            .color(WHITE)
            .pin_left(24.)
            .pin_right(24.),
        text(caption)
            .frame([24., 146., 336., 30.])
            .font_scale(2.)
            .color(MUTED)
            .pin_left(24.)
            .pin_right(24.),
        text("OPEN  ->")
            .frame([24., 192., 240., 24.])
            .font_scale(2.)
            .color(ACCENT),
    ];
    if id == "monitoring" {
        children.push(
            canvas(vec![
                circle(MUTED).frame([4., 0., 24., 24.]),
                circle(CARD).frame([9., 5., 14., 14.]),
                rounded_rect(MUTED)
                    .frame([0., 14., 32., 24.])
                    .corner_radius(6.0.into()),
                circle(CARD).frame([13., 21., 6., 6.]),
            ])
            .frame([328., 20., 32., 38.])
            .pin_right(24.),
        );
    }
    component([384., 232.], children)
        .key(id)
        .semantic_label(title)
        .navigate_to(Destination::app(id))
}

pub fn description() -> Ui {
    let mut children = vec![
        text("A shared runtime. A place to explore.")
            .position([52., 126.])
            .font_scale(2.)
            .color(MUTED)
            .key("intro"),
    ];
    children.extend(
        [
            (
                "component-playground",
                "Component Playground",
                "Touch + shared state",
                "Changes follow across devices",
            ),
            (
                "layout-lab",
                "Layout Lab",
                "Reusable components",
                "A five minute toggle signal",
            ),
            (
                "input-lab",
                "Input Lab",
                "Reusable keyboard",
                "Typing, focus and editing",
            ),
            (
                "animation-lab",
                "Animation Lab",
                "Local animation clock",
                "Keep moving as state changes",
            ),
            (
                "three-d-lab",
                "3D Lab",
                "Tetrahedra + rotation",
                "Shared intent, local playback",
            ),
            (
                "monitoring",
                "Monitoring",
                "Live service data",
                "Seven days of service history",
            ),
        ]
        .into_iter()
        .map(|(id, title, detail, caption)| launch_card(id, title, detail, caption)),
    );
    let card = |key| place(key, [384., 232.]).minimum([336., 232.]).flex_width();
    let wide = column(
        24.,
        vec![
            heading(),
            place("intro", [1184., 60.])
                .minimum([600., 60.])
                .flex_width(),
            column(
                24.,
                vec![
                    row(
                        16.,
                        vec![
                            card("component-playground"),
                            card("layout-lab"),
                            card("input-lab"),
                        ],
                    ),
                    row(
                        16.,
                        vec![
                            card("animation-lab"),
                            card("three-d-lab"),
                            card("monitoring"),
                        ],
                    ),
                ],
            ),
        ],
    );
    let tall = column(
        24.,
        vec![
            heading(),
            place("intro", [752., 60.]).flex_width(),
            column(
                24.,
                vec![
                    row(16., vec![card("component-playground"), card("layout-lab")]),
                    row(16., vec![card("input-lab"), card("animation-lab")]),
                    row(16., vec![card("three-d-lab"), card("monitoring")]),
                ],
            ),
        ],
    );
    AppUi::new("home", "Home", schema("home-state-v2", &[]))
        .with_composition(wide, tall)
        .build("OTriOS", children)
}
