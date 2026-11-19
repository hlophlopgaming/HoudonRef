//! Renders scenes without a window and checks what an artist would see.

use houdon_core::{Model, Scene, model::DEFAULT_FIGURE_HEIGHT};
use houdon_render::{Frame, Image, OffscreenRenderer};

const MALE: &[u8] = include_bytes!("../../../assets/models/test/male.glb");
const SIZE: u32 = 64;

/// A renderer, or `None` with a note when this machine has no GPU adapter.
///
/// CI sets `HOUDON_REQUIRE_GPU=1`, so there a missing adapter fails the test
/// instead of silently skipping it.
fn renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new_headless() {
        Ok(renderer) => Some(renderer),
        Err(error) if std::env::var_os("HOUDON_REQUIRE_GPU").is_some() => {
            panic!("HOUDON_REQUIRE_GPU is set but no renderer is available: {error}")
        }
        Err(error) => {
            eprintln!("skipping render test: {error}");
            None
        }
    }
}

fn figure_scene() -> Scene {
    let mut model = Model::from_gltf_slice("male", MALE).unwrap();
    model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
    let mut scene = Scene::new();
    // Most tests look at objects against a plain backdrop colour.
    scene.room.visible = false;
    let id = scene.add_model(model);
    scene.add_object(id).unwrap();
    scene.frame_camera(1.0);
    scene
}

fn render(renderer: &mut OffscreenRenderer, scene: &Scene) -> Image {
    let image = renderer.render(&Frame::from_scene(scene, 1.0), SIZE, SIZE);
    assert_eq!((image.width, image.height), (SIZE, SIZE));
    image
}

fn background_pixels(image: &Image, background: [u8; 3]) -> usize {
    image
        .pixels()
        .filter(|p| {
            p[..3]
                .iter()
                .zip(background)
                .all(|(a, b)| a.abs_diff(b) <= 1)
        })
        .count()
}

#[test]
fn an_empty_scene_renders_only_background() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.room.visible = false;
    let image = render(&mut renderer, &scene);
    assert_eq!(
        background_pixels(&image, scene.room.backdrop_srgb()),
        (SIZE * SIZE) as usize
    );
}

#[test]
fn a_figure_covers_part_of_the_picture() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = figure_scene();
    let image = render(&mut renderer, &scene);
    let covered = (SIZE * SIZE) as usize - background_pixels(&image, scene.room.backdrop_srgb());
    // A framed standing figure covers a narrow vertical band of the square.
    assert!(covered > 100, "only {covered} pixels show the figure");
    assert!(
        covered < (SIZE * SIZE / 2) as usize,
        "{covered} pixels show the figure"
    );
    // The centre of the picture is on the figure's body.
    let centre = image.pixel(SIZE / 2, SIZE / 2);
    assert_ne!(&centre[..3], &scene.room.backdrop_srgb()[..]);
}

fn two_figures_apart() -> (Scene, houdon_core::ObjectId, houdon_core::ObjectId) {
    let mut model = Model::from_gltf_slice("male", MALE).unwrap();
    model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
    let mut scene = Scene::new();
    scene.room.visible = false;
    let id = scene.add_model(model);
    let left = scene.add_object(id).unwrap();
    let right = scene.add_object(id).unwrap();
    scene.frame_camera(1.0);
    (scene, left, right)
}

fn column_has_figure(image: &Image, x: u32, background: [u8; 3]) -> bool {
    (0..image.height).any(|y| {
        let p = image.pixel(x, y);
        p[..3]
            .iter()
            .zip(background)
            .any(|(a, b)| a.abs_diff(b) > 1)
    })
}

#[test]
fn two_objects_of_one_model_stand_side_by_side() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (scene, _, _) = two_figures_apart();
    let image = render(&mut renderer, &scene);
    let bg = scene.room.backdrop_srgb();
    let figure_columns: Vec<bool> = (0..SIZE)
        .map(|x| column_has_figure(&image, x, bg))
        .collect();
    // Two separate bands of figure columns with background between them.
    let bands = figure_columns.windows(2).filter(|w| !w[0] && w[1]).count()
        + usize::from(figure_columns[0]);
    assert_eq!(bands, 2, "columns with figure: {figure_columns:?}");
}

#[test]
fn the_selected_object_is_highlighted() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (mut scene, left, _) = two_figures_apart();
    let plain = render(&mut renderer, &scene);
    scene.select(Some(left));
    let selected = render(&mut renderer, &scene);
    let half = SIZE / 2;
    let differs = |x0: u32, x1: u32| {
        (x0..x1).any(|x| (0..SIZE).any(|y| plain.pixel(x, y) != selected.pixel(x, y)))
    };
    assert!(differs(0, half), "the selected left figure looks different");
    assert!(!differs(half, SIZE), "the right figure is unchanged");
}

/// One framed figure lit only by `light` (no ambient light).
fn figure_lit_by(light: houdon_core::Light) -> Scene {
    let mut scene = figure_scene();
    scene.room.ambient_intensity = 0.0;
    let key = scene.lights()[0].id;
    scene.remove_light(key);
    let id = scene.add_light(light.kind).unwrap();
    scene.set_light(houdon_core::Light { id, ..light });
    scene
}

fn brightness(p: [u8; 4]) -> u32 {
    u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])
}

fn centre_brightness(image: &Image) -> u32 {
    brightness(image.pixel(SIZE / 2, SIZE / 2))
}

#[test]
fn a_surface_facing_a_light_is_brighter_than_one_facing_away() {
    use houdon_core::{Light, LightKind};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut front = Light::new(LightKind::Directional);
    front.azimuth = 0.0;
    front.elevation = 0.0;
    let back = Light {
        azimuth: std::f32::consts::PI,
        ..front.clone()
    };
    let lit = render(&mut renderer, &figure_lit_by(front));
    let unlit = render(&mut renderer, &figure_lit_by(back));
    assert!(centre_brightness(&lit) > centre_brightness(&unlit) + 60);
}

#[test]
fn a_disabled_light_does_not_change_the_picture() {
    use houdon_core::LightKind;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = figure_scene();
    let before = render(&mut renderer, &scene);
    let id = scene.add_light(LightKind::Point).unwrap();
    let mut extra = scene.light(id).unwrap().clone();
    extra.enabled = false;
    extra.intensity = 100.0;
    scene.set_light(extra);
    assert_eq!(render(&mut renderer, &scene), before);
}

#[test]
fn a_near_point_light_lights_more_than_a_far_one() {
    use houdon_core::{Light, LightKind};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut near = Light::new(LightKind::Point);
    near.azimuth = 0.0;
    near.elevation = 0.0;
    near.distance = 1.0;
    near.intensity = 1.0;
    let far = Light {
        distance: 4.0,
        ..near.clone()
    };
    let near_image = render(&mut renderer, &figure_lit_by(near));
    let far_image = render(&mut renderer, &figure_lit_by(far));
    assert!(centre_brightness(&near_image) > centre_brightness(&far_image) + 60);
}

#[test]
fn a_spot_light_leaves_surfaces_outside_its_cone_unlit() {
    use houdon_core::{Light, LightKind};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut spot = Light::new(LightKind::Spot);
    spot.azimuth = 0.0;
    spot.elevation = 0.0;
    spot.cone_angle = 8f32.to_radians();
    spot.softness = 0.0;
    let scene = figure_lit_by(spot);
    let image = render(&mut renderer, &scene);
    assert!(
        centre_brightness(&image) > 60,
        "the cone lights the middle of the body"
    );
    // The top of the head is far outside an 8 degree cone: black, not background.
    let bg = scene.room.backdrop_srgb();
    let top = (0..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
        .find(|&(x, y)| image.pixel(x, y)[..3] != bg[..])
        .expect("the figure is visible");
    assert!(
        brightness(image.pixel(top.0, top.1)) < 10,
        "{:?}",
        image.pixel(top.0, top.1)
    );
}

#[test]
fn sixteen_lights_light_the_figure() {
    use houdon_core::{LightKind, MAX_LIGHTS};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = figure_scene();
    scene.room.ambient_intensity = 0.0;
    let kinds = [LightKind::Directional, LightKind::Point, LightKind::Spot];
    for i in 0.. {
        if scene.add_light(kinds[i % 3]).is_none() {
            break;
        }
    }
    assert_eq!(scene.lights().len(), MAX_LIGHTS);
    let all = render(&mut renderer, &scene);
    // Turn the last light off: the picture must change, so it was drawn.
    let mut last = scene.lights()[MAX_LIGHTS - 1].clone();
    last.enabled = false;
    scene.set_light(last);
    assert_ne!(
        render(&mut renderer, &scene),
        all,
        "the 16th light is drawn"
    );
}

#[test]
fn the_floor_and_backdrop_stand_under_and_behind_the_figure() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = figure_scene();
    let hidden = render(&mut renderer, &scene);
    scene.room.visible = true;
    let shown = render(&mut renderer, &scene);
    let clear = scene.room.backdrop_srgb();
    let differs = |image: &Image, x: u32, y: u32| image.pixel(x, y)[..3] != clear[..];
    // Corners: backdrop above, floor below. Hidden, they show the plain colour.
    for (x, y) in [(1, 1), (SIZE - 2, 1), (1, SIZE - 2), (SIZE - 2, SIZE - 2)] {
        assert!(!differs(&hidden, x, y), "hidden room at ({x}, {y})");
        assert!(differs(&shown, x, y), "room drawn at ({x}, {y})");
    }
    assert!(
        differs(&shown, SIZE / 2, SIZE / 2),
        "the figure is still in front"
    );
}

/// A sphere of radius `r` made of `n` × `n` quads.
fn sphere(r: f32, n: u32) -> Model {
    use glam::Vec3;
    let mut positions = Vec::new();
    for i in 0..=n {
        let theta = std::f32::consts::PI * i as f32 / n as f32;
        for j in 0..=n {
            let phi = std::f32::consts::TAU * j as f32 / n as f32;
            positions.push(
                Vec3::new(
                    theta.sin() * phi.cos(),
                    theta.cos(),
                    theta.sin() * phi.sin(),
                ) * r,
            );
        }
    }
    let mut indices = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let a = i * (n + 1) + j;
            let b = a + n + 1;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Model::from_mesh("sphere", positions, indices)
}

#[test]
fn a_red_floor_warms_the_underside_of_a_form() {
    use glam::Vec3;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let warmth = |floor: Vec3, renderer: &mut OffscreenRenderer| {
        let mut scene = Scene::new();
        scene.room.visible = false;
        scene.room.floor_color = floor;
        let key = scene.lights()[0].id;
        scene.remove_light(key);
        let id = scene.add_model(sphere(0.5, 24));
        scene.add_object(id).unwrap();
        // Look at the sphere from below its equator.
        scene.frame_camera(1.0);
        scene.camera.orbit(0.0, -1.2);
        let image = render(renderer, &scene);
        let p = image.pixel(SIZE / 2, SIZE / 2 + SIZE / 6);
        i32::from(p[0]) - i32::from(p[2])
    };
    let grey = warmth(Vec3::splat(0.18), &mut renderer);
    let red = warmth(Vec3::new(0.6, 0.05, 0.05), &mut renderer);
    assert!(red > grey + 20, "red floor {red}, grey floor {grey}");
}
