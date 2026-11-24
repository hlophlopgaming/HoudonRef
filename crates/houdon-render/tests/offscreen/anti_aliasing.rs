//! Anti-aliasing of silhouettes.

use crate::*;

/// The figure as a black silhouette: no lights and no ambient light, so
/// without anti-aliasing the picture holds exactly two colours.
pub(crate) fn black_silhouette() -> Scene {
    let mut scene = figure_scene();
    let ids: Vec<_> = scene.lights().iter().map(|light| light.id).collect();
    for id in ids {
        scene.remove_light(id);
    }
    scene.room.ambient_intensity = 0.0;
    scene
}

/// Pixels that are clearly neither the silhouette nor the background.
pub(crate) fn in_between_pixels(image: &Image, background: [u8; 3]) -> usize {
    image
        .pixels()
        .filter(|p| {
            let off_background = p[..3]
                .iter()
                .zip(background)
                .any(|(a, b)| a.abs_diff(b) > 8);
            let off_black = p[..3].iter().any(|&c| c > 8);
            off_background && off_black
        })
        .count()
}

#[test]
fn anti_aliasing_softens_the_silhouette_edge() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = black_silhouette();
    let frame = Frame::from_scene(&scene, 1.0);
    let background = frame.clear_color();

    let sharp = renderer.render_with(&frame, SIZE, SIZE, AntiAliasing::OFF);
    assert_eq!(
        in_between_pixels(&sharp, background),
        0,
        "without anti-aliasing every pixel is figure or background"
    );
    assert!(
        background_pixels(&sharp, background) < (SIZE * SIZE) as usize,
        "the figure is in the picture"
    );

    for quality in [AntiAliasing::VIEWPORT, AntiAliasing::EXPORT] {
        let smooth = renderer.render_with(&frame, SIZE, SIZE, quality);
        let blended = in_between_pixels(&smooth, background);
        assert!(
            blended > 20,
            "{quality:?} blends the edge: {blended} in-between pixels"
        );
    }
}
