// SPDX-License-Identifier: Apache-2.0
//! Manual retained endpoint-buffer/grid shader check without app data or KMS ownership.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use otrios_linux_display::Display;
    let mut display = Display::headless([512, 512], false)?;
    let mut vertices = vec![];
    for c in [
        [0., -1.],
        [1., -1.],
        [0., 1.],
        [0., 1.],
        [1., -1.],
        [1., 1.],
    ] {
        vertices.extend([0., 0., 200., 160., c[0], c[1]]);
    }
    display.world_upload(0, &vertices)?;
    let projection = [2. / 512., 0., 0., 0., -2. / 512., 0., -1., 1., 1.];
    for (label, scale, origin) in [
        ("rest", 1., [100., 100.]),
        ("pan", 1., [150., 130.]),
        ("zoom", 2., [50., 50.]),
    ] {
        display.world_frame(&[0], &origin, [scale, 512., 512., 80., 0., 0.])?;
        display.draw_slots(projection, &[131])?;
        let rgba = display.pixels()?;
        let cyan = rgba
            .chunks_exact(4)
            .filter(|p| p[1] > 120 && p[2] > 120 && p[0] < 130)
            .count();
        assert!(cyan > 200, "missing retained stroke");
        let mut ppm = b"P6\n512 512\n255\n".to_vec();
        for row in rgba.chunks_exact(512 * 4).rev() {
            for p in row.chunks_exact(4) {
                ppm.extend_from_slice(&p[..3]);
            }
        }
        std::fs::write(format!("/tmp/world-{label}.ppm"), ppm)?;
        println!("{label}: {cyan} stroke pixels");
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux GLES2 probe");
}
