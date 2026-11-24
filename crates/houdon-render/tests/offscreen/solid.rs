//! The Solid shading mode: Studio, Matcap and Flat light.

use crate::*;

/// The figure in the Solid mode with `lighting`.
pub(crate) fn solid_figure(lighting: houdon_core::SolidLighting) -> Scene {
    let mut scene = figure_scene();
    scene.shading.mode = houdon_core::ShadingMode::Solid;
    scene.shading.solid = lighting;
    scene
}

pub(crate) fn sharp(renderer: &mut OffscreenRenderer, scene: &Scene) -> Image {
    renderer.render_with(
        &Frame::from_scene(scene, 1.0),
        SIZE,
        SIZE,
        AntiAliasing::OFF,
    )
}

#[test]
fn the_solid_mode_ignores_the_scene_lights() {
    use houdon_core::SolidLighting;
    let Some(mut renderer) = renderer() else {
        return;
    };
    for lighting in SolidLighting::ALL {
        let mut scene = solid_figure(lighting);
        if lighting == SolidLighting::Matcap {
            scene.shading.matcap = Some(houdon_core::Matcap {
                name: "test".into(),
                image: std::sync::Arc::new(houdon_core::MatcapImage::from_fn(32, 32, |u, v| {
                    [u, v, 0.5]
                })),
            });
        }
        let before = sharp(&mut renderer, &scene);
        let mut key = scene.lights()[0].clone();
        key.azimuth += 1.5;
        key.intensity *= 3.0;
        scene.set_light(key.clone());
        assert!(
            sharp(&mut renderer, &scene) == before,
            "{lighting:?}: moving a light"
        );
        key.enabled = false;
        scene.set_light(key);
        assert!(
            sharp(&mut renderer, &scene) == before,
            "{lighting:?}: turning a light off"
        );
        scene.shading.mode = houdon_core::ShadingMode::Lit;
        let mut key = scene.lights()[0].clone();
        key.enabled = true;
        scene.set_light(key);
        assert!(
            sharp(&mut renderer, &scene) != before,
            "{lighting:?}: Solid differs from Lit"
        );
    }
}

#[test]
fn two_matcaps_give_two_pictures() {
    use houdon_core::{Matcap, MatcapImage, SolidLighting};
    use std::sync::Arc;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let with = |renderer: &mut OffscreenRenderer, color: [f32; 3]| {
        let mut scene = solid_figure(SolidLighting::Matcap);
        scene.shading.matcap = Some(Matcap {
            name: "test".into(),
            image: Arc::new(MatcapImage::from_fn(16, 16, |_, v| color.map(|c| c * v))),
        });
        sharp(renderer, &scene)
    };
    let red = with(&mut renderer, [1.0, 0.1, 0.1]);
    let blue = with(&mut renderer, [0.1, 0.1, 1.0]);
    assert!(red != blue, "different matcaps, different pictures");
    let centre = red.pixel(SIZE / 2, SIZE / 2);
    assert!(
        centre[0] > centre[2] + 40,
        "the red matcap colours the figure: {centre:?}"
    );
    let centre = blue.pixel(SIZE / 2, SIZE / 2);
    assert!(
        centre[2] > centre[0] + 40,
        "the blue matcap colours the figure: {centre:?}"
    );
}

#[test]
fn flat_light_gives_one_colour_per_object() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = solid_figure(houdon_core::SolidLighting::Flat);
    let image = sharp(&mut renderer, &scene);
    let background = Frame::from_scene(&scene, 1.0).clear_color();
    let mut figure: Vec<[u8; 3]> = image
        .pixels()
        .map(|p| [p[0], p[1], p[2]])
        .filter(|p| *p != background)
        .collect();
    assert!(figure.len() > 50, "the figure is in the picture");
    figure.sort_unstable();
    figure.dedup();
    assert_eq!(figure.len(), 1, "one flat colour: {figure:?}");
}
