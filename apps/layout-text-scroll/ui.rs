// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;
pub fn description() -> Ui {
    let font = otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .expect("qualified demo font");
    let mut lines = vec![];
    let mut width = 0_f32;
    for row in 0..64 {
        let value = format!(
            "{:02}  {}",
            row + 1,
            "The quick brown fox jumps over the lazy dog. ".repeat(5)
        );
        let mut x = 0.;
        for chunk in value.as_bytes().chunks(80) {
            let value = std::str::from_utf8(chunk).expect("ASCII demo text");
            let w = font.text_width(value, 2.);
            lines.push(text(value).frame([x, 8. + row as f32 * 27., w.max(1.), 24.]));
            x += w;
        }
        width = width.max(x);
    }
    let composition = || {
        column(
            12.,
            vec![
                row(
                    12.,
                    vec![
                        place("$heading", [650., 64.])
                            .minimum([240., 64.])
                            .flex_width(),
                        place("back", [140., 64.]),
                        place("home", [140., 64.]),
                    ],
                ),
                place("hint", [1184., 24.]).minimum([1., 24.]).flex_width(),
                place("page", [1184., 600.])
                    .minimum([500., 350.])
                    .flex_width()
                    .flex_height(),
                place("footer", [1184., 24.])
                    .minimum([1., 24.])
                    .flex_width(),
            ],
        )
    };
    AppUi::new(
        "layout-text-scroll",
        "Text scrolling",
        schema("layout-text-scroll-state-v1", &[]),
    )
    .with_composition(composition(), composition())
    .with_text_scroll("page", [width + 16., 64. * 27. + 16.])
    .build(
        "Text scrolling",
        vec![
            button("Back").on_press(Event::back()).key("back"),
            button("Home")
                .navigate_to(Destination::app("home"))
                .key("home"),
            text("EXPERIMENT - Drag in any direction. Release to coast. Touch to stop.")
                .color(MUTED)
                .key("hint"),
            canvas(lines).frame([0., 0., 1184., 600.]).key("page"),
            text(" ").key("footer"),
        ],
    )
}
