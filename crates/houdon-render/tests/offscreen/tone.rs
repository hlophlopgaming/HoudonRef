//! Tone: bright planes keep their steps instead of clipping to white.

use houdon_core::{ShadingMode, SolidLighting};

use crate::*;

/// White clay under a strong key light, lit from the front.
fn bright_figure(mode: ShadingMode) -> Scene {
    let mut scene = figure_scene();
    scene.shading.mode = mode;
    scene.shading.solid = SolidLighting::Studio;
    scene.shading.clay_color = glam::Vec3::ONE;
    scene.shading.color = houdon_core::ColorSource::Clay;
    let mut key = scene.lights()[0].clone();
    key.intensity *= 2.0;
    key.azimuth = 0.3;
    scene.set_light(key);
    scene
}

fn white_pixels(image: &Image) -> usize {
    image
        .pixels()
        .filter(|p| p[..3].iter().all(|&c| c >= 254))
        .count()
}

#[test]
fn bright_light_on_white_clay_does_not_clip_to_white() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    for mode in [ShadingMode::Lit, ShadingMode::Solid] {
        let image = render(&mut renderer, &bright_figure(mode));
        assert_eq!(white_pixels(&image), 0, "{mode:?}: no clipped pixels");
        let brightest = image.pixels().map(brightness).max().unwrap();
        assert!(brightest > 3 * 200, "{mode:?}: still bright, {brightest}");
    }
}

#[test]
fn the_strongest_cavity_never_clips_ridges_to_white() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = bright_figure(ShadingMode::Solid);
    let cavity = &mut scene.shading.cavity;
    cavity.geometry = true;
    cavity.screen = true;
    cavity.geometry_ridge = houdon_core::Cavity::MAX_STRENGTH;
    cavity.screen_ridge = houdon_core::Cavity::MAX_STRENGTH;
    let image = render(&mut renderer, &scene);
    assert_eq!(white_pixels(&image), 0);
}
