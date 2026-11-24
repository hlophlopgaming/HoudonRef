//! Colour sources: materials, textures, Clay and Random.

use crate::*;

pub(crate) const TWO_MATERIALS: &[u8] =
    include_bytes!("../../../../assets/models/test/two_materials.glb");

/// The red cube and the checker cube, front on, lit from the front.
pub(crate) fn two_material_cubes(color: houdon_core::ColorSource) -> Scene {
    let mut scene = Scene::new();
    scene.room.visible = false;
    scene.shading.color = color;
    let id = scene.add_model(Model::from_gltf_slice("two", TWO_MATERIALS).unwrap());
    scene.add_object(id).unwrap();
    let mut key = scene.lights()[0].clone();
    key.azimuth = 0.0;
    key.elevation = 0.3;
    scene.set_light(key);
    scene.frame_camera(1.0);
    scene
}

#[test]
fn materials_colour_their_parts_and_clay_covers_them() {
    use houdon_core::ColorSource;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let red_face = glam::Vec3::new(-0.3, 0.2, 0.2);
    let checker_face = glam::Vec3::new(0.3, 0.2, 0.2);
    let material = two_material_cubes(ColorSource::Material);
    let image = render(&mut renderer, &material);
    let (rx, ry) = pixel_of(&material, red_face);
    let red = image.pixel(rx, ry);
    assert!(
        red[0] > red[1] + 60 && red[0] > red[2] + 60,
        "the red material is red: {red:?}"
    );

    let clay = two_material_cubes(ColorSource::Clay);
    let image = render(&mut renderer, &clay);
    let (cx, cy) = pixel_of(&clay, checker_face);
    let left = image.pixel(rx, ry);
    let right = image.pixel(cx, cy);
    let spread = |p: [u8; 4]| p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap();
    assert!(
        spread(left) < 12 && spread(right) < 12,
        "clay is grey: {left:?} {right:?}"
    );
}

#[test]
fn a_base_colour_texture_shows_on_the_surface() {
    use houdon_core::ColorSource;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = two_material_cubes(ColorSource::Material);
    let image = render(&mut renderer, &scene);
    // Across the checker cube's front face: green squares and white squares.
    let (x0, y0) = pixel_of(&scene, glam::Vec3::new(0.15, 0.35, 0.2));
    let (x1, y1) = pixel_of(&scene, glam::Vec3::new(0.45, 0.05, 0.2));
    let pixels: Vec<[u8; 4]> = (x0..=x1)
        .flat_map(|x| (y0..=y1).map(move |y| (x, y)))
        .map(|(x, y)| image.pixel(x, y))
        .collect();
    let [r, g, b] = [0, 1, 2];
    let greenish = pixels
        .iter()
        .filter(|p| {
            i32::from(p[g]) > i32::from(p[r]) + 30 && i32::from(p[g]) > i32::from(p[b]) + 30
        })
        .count();
    let whitish = pixels
        .iter()
        .filter(|p| p[0].abs_diff(p[1]) < 15 && p[0] > 120)
        .count();
    assert!(
        greenish > 0 && whitish > 0,
        "green {greenish}, white {whitish} of {}",
        pixels.len()
    );
}

#[test]
fn random_colours_tell_objects_apart() {
    use houdon_core::ColorSource;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (mut scene, left, right) = two_figures_apart();
    scene.shading.color = ColorSource::Random;
    let image = render(&mut renderer, &scene);
    let centre = |id| {
        let object = scene.object(id).unwrap();
        pixel_of(&scene, scene.object_bounds(object).center())
    };
    let (a, b) = (centre(left), centre(right));
    let (pa, pb) = (image.pixel(a.0, a.1), image.pixel(b.0, b.1));
    // Coloured, not grey clay, and of different hues: compare the colours
    // with their brightness divided out.
    let spread = |p: [u8; 4]| p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap();
    assert!(
        spread(pa) > 20 && spread(pb) > 20,
        "coloured: {pa:?} {pb:?}"
    );
    let hue = |p: [u8; 4]| {
        let sum = f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2]);
        [
            f32::from(p[0]) / sum,
            f32::from(p[1]) / sum,
            f32::from(p[2]) / sum,
        ]
    };
    let (ha, hb) = (hue(pa), hue(pb));
    let difference: f32 = ha.iter().zip(&hb).map(|(x, y)| (x - y).abs()).sum();
    assert!(difference > 0.1, "different hues: {pa:?} vs {pb:?}");
}

#[test]
fn a_model_without_materials_looks_like_clay() {
    use houdon_core::ColorSource;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = figure_scene();
    scene.shading.color = ColorSource::Material;
    let as_material = render(&mut renderer, &scene);
    scene.shading.color = ColorSource::Clay;
    assert_eq!(render(&mut renderer, &scene), as_material);
}
