//! Renders scenes without a window and checks what an artist would see.

use std::sync::{Mutex, MutexGuard, OnceLock};

use houdon_core::{Model, Scene, model::DEFAULT_FIGURE_HEIGHT};
use houdon_render::{Frame, Image, OffscreenRenderer};

const MALE: &[u8] = include_bytes!("../../../assets/models/test/male.glb");
const SIZE: u32 = 64;

/// The one renderer all tests share, like the app has one GPU device for
/// its whole run. Tests take turns with it. Opening and dropping a device
/// per test, from parallel test threads, once crashed Mesa's software
/// Vulkan driver (lavapipe) on the CI runner.
static RENDERER: OnceLock<Result<Mutex<OffscreenRenderer>, String>> = OnceLock::new();

/// The shared renderer, or `None` with a note when this machine has no GPU
/// adapter.
///
/// CI sets `HOUDON_REQUIRE_GPU=1`, so there a missing adapter fails the test
/// instead of silently skipping it.
fn renderer() -> Option<MutexGuard<'static, OffscreenRenderer>> {
    let shared = RENDERER.get_or_init(|| {
        OffscreenRenderer::new_headless()
            .map(Mutex::new)
            .map_err(|error| error.to_string())
    });
    match shared {
        // A test that failed while holding the renderer leaves it usable.
        Ok(renderer) => Some(
            renderer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        ),
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

#[test]
fn from_behind_the_backdrop_the_objects_stay_visible() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = figure_scene();
    // Look at the figure from far behind the backdrop.
    scene.camera.yaw = std::f32::consts::PI;
    scene.camera.pitch = 0.0;
    scene.camera.distance = 60.0;
    scene.camera.focal_length_mm = 300.0;
    let hidden = render(&mut renderer, &scene);
    scene.room.visible = true;
    let shown = render(&mut renderer, &scene);
    let centre = (SIZE / 2, SIZE / 2);
    assert_eq!(
        shown.pixel(centre.0, centre.1),
        hidden.pixel(centre.0, centre.1),
        "the backdrop seen from behind does not hide the figure"
    );
}

/// A 30 cm sphere floating 30 cm above the floor in a dark room, lit only
/// by one `kind` light straight above it, seen from the front and above.
fn sphere_over_floor(kind: houdon_core::LightKind, casts_shadows: bool) -> Scene {
    sphere_lit_from(kind, casts_shadows, 0.0, std::f32::consts::FRAC_PI_2)
}

/// The same sphere lit by one `kind` light at `azimuth` and `elevation`.
fn sphere_lit_from(
    kind: houdon_core::LightKind,
    casts_shadows: bool,
    azimuth: f32,
    elevation: f32,
) -> Scene {
    use houdon_core::{Light, Transform};
    let mut scene = Scene::new();
    scene.room.ambient_intensity = 0.0;
    let id = scene.add_model(sphere(0.3, 24));
    let object = scene.add_object(id).unwrap();
    scene.set_transform(
        object,
        Transform {
            translation: glam::Vec3::new(0.0, 0.3, 0.0),
            ..Transform::default()
        },
    );
    let key = scene.lights()[0].id;
    scene.remove_light(key);
    let light_id = scene.add_light(kind).unwrap();
    scene.set_light(Light {
        id: light_id,
        azimuth,
        elevation,
        distance: 3.0,
        intensity: if kind.is_positional() { 9.0 } else { 1.0 },
        casts_shadows,
        ..Light::new(kind)
    });
    scene.frame_camera(1.0);
    scene.camera.distance *= 2.5;
    scene.camera.pitch = 0.5;
    scene
}

/// The pixel where a world point appears.
fn pixel_of(scene: &Scene, point: glam::Vec3) -> (u32, u32) {
    let ndc = scene.camera.view_projection(1.0).project_point3(point);
    let x = ((ndc.x + 1.0) * 0.5 * SIZE as f32) as u32;
    let y = ((1.0 - ndc.y) * 0.5 * SIZE as f32) as u32;
    (x.min(SIZE - 1), y.min(SIZE - 1))
}

#[test]
fn every_light_kind_casts_a_shadow_on_the_floor() {
    use houdon_core::LightKind;
    let Some(mut renderer) = renderer() else {
        return;
    };
    for kind in LightKind::ALL {
        let shadowed = sphere_over_floor(kind, true);
        let (x, y) = pixel_of(&shadowed, glam::Vec3::ZERO);
        let dark = brightness(render(&mut renderer, &shadowed).pixel(x, y));
        let lit = brightness(render(&mut renderer, &sphere_over_floor(kind, false)).pixel(x, y));
        assert!(
            lit > dark + 60,
            "{kind:?}: floor under the sphere {dark} with shadow, {lit} without"
        );
    }
}

#[test]
fn moving_only_the_camera_does_not_rebuild_shadows() {
    use houdon_core::LightKind;
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = sphere_over_floor(LightKind::Spot, true);
    render(&mut renderer, &scene);
    let builds = renderer.shadow_map_builds();
    scene.camera.orbit(0.4, 0.1);
    render(&mut renderer, &scene);
    assert_eq!(
        renderer.shadow_map_builds(),
        builds,
        "orbiting reuses the shadow maps"
    );
    let mut light = scene.lights()[0].clone();
    light.azimuth += 0.3;
    scene.set_light(light);
    render(&mut renderer, &scene);
    assert_eq!(
        renderer.shadow_map_builds(),
        builds + 1,
        "moving a light rebuilds them"
    );
}

#[test]
fn sixteen_shadow_casting_point_lights_render() {
    use houdon_core::{LightKind, MAX_LIGHTS};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = sphere_over_floor(LightKind::Point, true);
    while scene.add_light(LightKind::Point).is_some() {}
    assert_eq!(scene.lights().len(), MAX_LIGHTS);
    assert!(scene.lights().iter().all(|l| l.casts_shadows));
    let image = render(&mut renderer, &scene);
    let (x, y) = pixel_of(&scene, glam::Vec3::ZERO);
    assert!(
        brightness(image.pixel(x, y)) > 0,
        "the floor is lit around the shadows"
    );
}

#[test]
fn a_low_side_light_casts_a_long_shadow() {
    use houdon_core::LightKind;
    let Some(mut renderer) = renderer() else {
        return;
    };
    // Light from +X, 10 degrees above the horizon: the shadow of the sphere
    // (centre 0.6 m up) falls on the floor about 3.4 m to the -X side,
    // seen through the +X face of a point light's cube.
    let elevation = 10f32.to_radians();
    let shadow_x = -0.6 / elevation.tan();
    for kind in LightKind::ALL {
        let make = |casts| {
            let mut scene = sphere_lit_from(kind, casts, std::f32::consts::FRAC_PI_2, elevation);
            scene.camera.target = glam::Vec3::new(shadow_x * 0.5, 0.3, 0.0);
            scene.camera.distance = 11.0;
            scene
        };
        let shadowed = make(true);
        let (x, y) = pixel_of(&shadowed, glam::Vec3::new(shadow_x, 0.0, 0.0));
        assert!(
            x > 0 && x < SIZE - 1 && y > 0 && y < SIZE - 1,
            "{kind:?}: shadow in view"
        );
        let dark = brightness(render(&mut renderer, &shadowed).pixel(x, y));
        let lit = brightness(render(&mut renderer, &make(false)).pixel(x, y));
        assert!(
            lit > dark + 40,
            "{kind:?}: long shadow {dark}, lit floor {lit}"
        );
    }
}

#[test]
fn shadows_stay_put_with_many_lights_sharing_the_atlas() {
    use houdon_core::{Light, LightKind};
    let Some(mut renderer) = renderer() else {
        return;
    };
    // The same overhead point light, alone and then with 15 more point
    // lights that are off for shadows... except they still take atlas
    // tiles: the shadow under the sphere must not move or fade.
    let alone = sphere_over_floor(LightKind::Point, true);
    let (x, y) = pixel_of(&alone, glam::Vec3::ZERO);
    let dark_alone = brightness(render(&mut renderer, &alone).pixel(x, y));
    let mut crowded = sphere_over_floor(LightKind::Point, true);
    while let Some(id) = crowded.add_light(LightKind::Point) {
        let light = crowded.light(id).unwrap().clone();
        // Far away and dim: they take tiles but hardly light the floor.
        crowded.set_light(Light {
            intensity: 0.0,
            ..light
        });
    }
    let dark_crowded = brightness(render(&mut renderer, &crowded).pixel(x, y));
    assert!(
        dark_crowded <= dark_alone + 3,
        "alone {dark_alone}, with 16 lights {dark_crowded}"
    );
}
