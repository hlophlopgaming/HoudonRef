//! Exporting the view as a picture.

use houdon_render::ExportSize;

use crate::*;

#[test]
fn export_sizes_keep_the_view_proportions() {
    let window = [800, 600];
    assert_eq!(ExportSize::Window.pixels(window, 16384), [800, 600]);
    assert_eq!(ExportSize::Double.pixels(window, 16384), [1600, 1200]);
    assert_eq!(ExportSize::Large.pixels(window, 16384), [4096, 3072]);
    assert_eq!(ExportSize::Large.pixels([600, 800], 16384), [3072, 4096]);
    // A device that cannot go that large gets its largest, same proportions.
    assert_eq!(ExportSize::Large.pixels(window, 2048), [2048, 1536]);
}

/// The box around the figure's pixels, as fractions of the picture.
fn figure_box(image: &Image, background: [u8; 3]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for y in 0..image.height {
        for x in 0..image.width {
            if image.pixel(x, y)[..3] != background[..] {
                b = [
                    b[0].min(x as f32),
                    b[1].min(y as f32),
                    b[2].max(x as f32 + 1.0),
                    b[3].max(y as f32 + 1.0),
                ];
            }
        }
    }
    [
        b[0] / image.width as f32,
        b[1] / image.height as f32,
        b[2] / image.width as f32,
        b[3] / image.height as f32,
    ]
}

#[test]
fn an_export_has_the_size_asked_for_and_the_same_framing() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = figure_scene();
    let window = [96, 64];
    let frame = Frame::from_scene(&scene, 96.0 / 64.0);
    let background = frame.clear_color();
    let mut boxes = Vec::new();
    for size in [ExportSize::Window, ExportSize::Double] {
        let [w, h] = size.pixels(window, renderer.max_side());
        let image = renderer.render_with(&frame, w, h, AntiAliasing::OFF);
        assert_eq!((image.width, image.height), (w, h), "{size:?}");
        boxes.push(figure_box(&image, background));
    }
    for (a, b) in boxes[0].iter().zip(&boxes[1]) {
        assert!((a - b).abs() < 2.0 / 64.0, "same framing: {boxes:?}");
    }
    let [w, h] = ExportSize::Large.pixels(window, renderer.max_side());
    let image = renderer.render_with(&frame, w, h, AntiAliasing::EXPORT);
    assert_eq!((image.width, image.height), (w, h));
}
