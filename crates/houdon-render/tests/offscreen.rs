//! Renders scenes without a window and checks what an artist would see.

use std::sync::{Mutex, MutexGuard, OnceLock};

use houdon_core::{Model, Scene, model::DEFAULT_FIGURE_HEIGHT};
use houdon_render::{AntiAliasing, Frame, Image, OffscreenRenderer};

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
    // Sharp edges: the first figure pixel must be the figure, not a blend
    // with the background.
    let image = renderer.render_with(
        &Frame::from_scene(&scene, 1.0),
        SIZE,
        SIZE,
        AntiAliasing::OFF,
    );
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

const TWO_MATERIALS: &[u8] = include_bytes!("../../../assets/models/test/two_materials.glb");

/// The red cube and the checker cube, front on, lit from the front.
fn two_material_cubes(color: houdon_core::ColorSource) -> Scene {
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

fn distinct_colours(image: &Image) -> usize {
    let mut seen: Vec<[u8; 3]> = image.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

#[test]
fn tones_reduce_the_picture_to_that_many_values() {
    use houdon_core::ValueMode;
    let Some(mut renderer) = renderer() else {
        return;
    };
    for tones in ValueMode::MIN_TONES..=ValueMode::MAX_TONES {
        let mut scene = figure_scene();
        scene.shading.value.mode = ValueMode::Tones(tones);
        let count = distinct_colours(&render(&mut renderer, &scene));
        assert!(
            count <= usize::from(tones),
            "{tones} tones gave {count} values"
        );
        if tones == 2 {
            assert_eq!(count, 2, "two tones: light and shadow");
        }
    }
}

#[test]
fn two_tones_split_the_lit_side_from_the_shadow_side() {
    use houdon_core::{Light, LightKind, ValueMode};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.room.visible = false;
    let id = scene.add_model(sphere(0.5, 32));
    scene.add_object(id).unwrap();
    let key = scene.lights()[0].id;
    scene.set_light(Light {
        id: key,
        azimuth: std::f32::consts::FRAC_PI_2,
        elevation: 0.0,
        ..Light::new(LightKind::Directional)
    });
    scene.frame_camera(1.0);
    scene.shading.value.mode = ValueMode::Tones(2);
    let image = render(&mut renderer, &scene);
    let lit = pixel_of(&scene, glam::Vec3::new(0.3, 0.5, 0.3));
    let shadow = pixel_of(&scene, glam::Vec3::new(-0.3, 0.5, 0.3));
    let (a, b) = (image.pixel(lit.0, lit.1), image.pixel(shadow.0, shadow.1));
    assert!(brightness(a) > brightness(b), "lit {a:?}, shadow {b:?}");
}

#[test]
fn grey_keeps_lightness_and_drops_colour() {
    use houdon_core::{ColorSource, ValueMode};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = two_material_cubes(ColorSource::Material);
    let colour = render(&mut renderer, &scene);
    scene.shading.value.mode = ValueMode::Grey;
    let grey = render(&mut renderer, &scene);
    for (c, g) in colour.pixels().zip(grey.pixels()) {
        assert!(
            g[0].abs_diff(g[1]) <= 1 && g[1].abs_diff(g[2]) <= 1,
            "grey pixel {g:?}"
        );
        let luma = 0.2126 * f32::from(c[0]) + 0.7152 * f32::from(c[1]) + 0.0722 * f32::from(c[2]);
        assert!((f32::from(g[1]) - luma).abs() <= 3.0, "{c:?} → {g:?}");
    }
}

#[test]
fn squinting_softens_neighbouring_contrast() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let contrast = |image: &Image| -> u64 {
        (0..SIZE)
            .flat_map(|y| (0..SIZE - 1).map(move |x| (x, y)))
            .map(|(x, y)| {
                u64::from(brightness(image.pixel(x, y)).abs_diff(brightness(image.pixel(x + 1, y))))
            })
            .sum()
    };
    let mut scene = figure_scene();
    let sharp = contrast(&render(&mut renderer, &scene));
    scene.shading.value.squint = 1.0;
    let squinted = contrast(&render(&mut renderer, &scene));
    assert!(
        squinted * 10 < sharp * 8,
        "squint {squinted} vs sharp {sharp}"
    );
}

/// The figure as a black silhouette: no lights and no ambient light, so
/// without anti-aliasing the picture holds exactly two colours.
fn black_silhouette() -> Scene {
    let mut scene = figure_scene();
    let ids: Vec<_> = scene.lights().iter().map(|light| light.id).collect();
    for id in ids {
        scene.remove_light(id);
    }
    scene.room.ambient_intensity = 0.0;
    scene
}

/// Pixels that are clearly neither the silhouette nor the background.
fn in_between_pixels(image: &Image, background: [u8; 3]) -> usize {
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

/// Brightness along the pixel row through `point`, without anti-aliasing,
/// so only the shadow's own filtering shows.
fn row_through(renderer: &mut OffscreenRenderer, scene: &Scene, point: glam::Vec3) -> Vec<u32> {
    let image = renderer.render_with(
        &Frame::from_scene(scene, 1.0),
        SIZE,
        SIZE,
        AntiAliasing::OFF,
    );
    let (_, y) = pixel_of(scene, point);
    (0..SIZE).map(|x| brightness(image.pixel(x, y))).collect()
}

/// How many pixels of `row` sit clearly between its darkest and lightest.
fn penumbra_pixels(row: &[u32]) -> usize {
    let (lo, hi) = (*row.iter().min().unwrap(), *row.iter().max().unwrap());
    let span = (hi - lo) as f32;
    row.iter()
        .filter(|&&b| {
            let t = (b - lo) as f32 / span;
            (0.15..=0.85).contains(&t)
        })
        .count()
}

/// The floor around the edge of the shadow of the sphere over the floor,
/// lit from straight above.
fn shadow_edge_scene(kind: houdon_core::LightKind, shadow_softness: f32) -> Scene {
    let mut scene = sphere_over_floor(kind, true);
    let mut light = scene.lights()[0].clone();
    light.shadow_softness = shadow_softness;
    scene.set_light(light);
    // Close enough that a pixel is well under a centimetre of floor.
    scene.camera.target = glam::Vec3::new(0.3, 0.0, 0.0);
    scene.camera.distance = 0.9;
    scene.camera.pitch = 1.0;
    scene.camera.yaw = 0.0;
    scene
}

#[test]
fn shadow_edges_are_smooth_ramps_for_every_light_kind() {
    use houdon_core::{Light, LightKind};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let default_softness = Light::new(LightKind::Directional).shadow_softness;
    for kind in LightKind::ALL {
        let scene = shadow_edge_scene(kind, default_softness);
        let row = row_through(&mut renderer, &scene, glam::Vec3::new(0.3, 0.0, 0.0));
        let (lo, hi) = (*row.iter().min().unwrap(), *row.iter().max().unwrap());
        assert!(hi > lo + 60, "{kind:?}: a shadow edge in view: {row:?}");
        let steepest = row.windows(2).map(|w| w[0].abs_diff(w[1])).max().unwrap();
        assert!(
            steepest * 100 <= (hi - lo) * 40,
            "{kind:?}: no jump from shadow to light in one pixel: {row:?}"
        );
        assert!(
            penumbra_pixels(&row) >= 3,
            "{kind:?}: the edge is a ramp over several pixels: {row:?}"
        );
    }
}

#[test]
fn a_softer_light_widens_the_penumbra_but_not_at_the_contact() {
    use houdon_core::{LightKind, Transform};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let crisp = shadow_edge_scene(LightKind::Directional, 0.0);
    let soft = shadow_edge_scene(LightKind::Directional, 1.0);
    let edge = glam::Vec3::new(0.3, 0.0, 0.0);
    let crisp_width = penumbra_pixels(&row_through(&mut renderer, &crisp, edge));
    let soft_width = penumbra_pixels(&row_through(&mut renderer, &soft, edge));
    assert!(
        soft_width > crisp_width + 2,
        "softer light, wider penumbra: crisp {crisp_width}, soft {soft_width}"
    );

    // A box resting on the floor, lit from the side: right next to its
    // foot, the shadow stays dark however soft the light.
    for softness in [0.0, 1.0] {
        let mut scene = shadow_edge_scene(LightKind::Directional, softness);
        let object = scene.objects()[0].id;
        let box_model = scene.add_model(cube(0.15));
        let resting = scene.add_object(box_model).unwrap();
        scene.remove_object(object);
        scene.set_transform(resting, Transform::default());
        let mut light = scene.lights()[0].clone();
        light.azimuth = std::f32::consts::FRAC_PI_2;
        light.elevation = 45f32.to_radians();
        scene.set_light(light);
        scene.camera.target = glam::Vec3::ZERO;
        scene.camera.distance = 1.8;
        let image = renderer.render_with(
            &Frame::from_scene(&scene, 1.0),
            SIZE,
            SIZE,
            AntiAliasing::OFF,
        );
        let at = |point| {
            let (x, y) = pixel_of(&scene, point);
            assert!(
                x > 0 && x < SIZE - 1 && y > 0 && y < SIZE - 1,
                "{point} in view"
            );
            brightness(image.pixel(x, y))
        };
        let near_contact = at(glam::Vec3::new(-0.24, 0.0, 0.0));
        let lit = at(glam::Vec3::new(0.0, 0.0, 0.3));
        assert!(
            near_contact * 100 < lit * 25,
            "softness {softness}: next to the foot {near_contact}, lit floor {lit}"
        );
    }
}

/// A cube with sides `2 * half`.
fn cube(half: f32) -> Model {
    let positions = (0..8)
        .map(|i| {
            glam::Vec3::new(
                if i & 1 == 0 { -half } else { half },
                if i & 2 == 0 { -half } else { half },
                if i & 4 == 0 { -half } else { half },
            )
        })
        .collect();
    let indices = vec![
        0, 2, 1, 1, 2, 3, // -Z
        4, 5, 6, 5, 7, 6, // +Z
        0, 1, 4, 1, 5, 4, // -Y
        2, 6, 3, 3, 6, 7, // +Y
        0, 4, 2, 2, 4, 6, // -X
        1, 3, 5, 3, 7, 5, // +X
    ];
    Model::from_mesh("cube", positions, indices)
}

/// Vertical stripes, eight light and eight dark.
fn striped_mask() -> std::sync::Arc<houdon_core::MaskImage> {
    std::sync::Arc::new(houdon_core::MaskImage::from_fn(64, 64, |u, _| {
        if (u * 8.0).fract() < 0.5 { 255 } else { 0 }
    }))
}

/// The empty floor seen from above, lit straight down by one `kind` light
/// through `mask`, if any.
fn floor_under(kind: houdon_core::LightKind, mask: Option<houdon_core::LightMask>) -> Scene {
    use houdon_core::Light;
    let mut scene = Scene::new();
    let key = scene.lights()[0].id;
    scene.remove_light(key);
    let id = scene.add_light(kind).unwrap();
    scene.set_light(Light {
        id,
        azimuth: 0.0,
        elevation: std::f32::consts::FRAC_PI_2,
        distance: 3.0,
        cone_angle: 70f32.to_radians(),
        softness: 0.0,
        intensity: if kind.is_positional() { 9.0 } else { 1.0 },
        mask,
        ..Light::new(kind)
    });
    scene.room.ambient_intensity = 0.0;
    scene.camera.target = glam::Vec3::ZERO;
    scene.camera.distance = 6.0;
    scene.camera.pitch = 1.4;
    scene.camera.yaw = 0.0;
    scene
}

/// Brightness along the middle row of the floor, the middle half of it.
fn middle_row(renderer: &mut OffscreenRenderer, scene: &Scene) -> Vec<u32> {
    let image = renderer.render_with(
        &Frame::from_scene(scene, 1.0),
        SIZE,
        SIZE,
        AntiAliasing::OFF,
    );
    (SIZE / 4..SIZE * 3 / 4)
        .map(|x| brightness(image.pixel(x, SIZE / 2)))
        .collect()
}

/// How often the row crosses its own mean brightness.
fn crossings(row: &[u32]) -> usize {
    let mean = row.iter().sum::<u32>() / row.len() as u32;
    row.windows(2)
        .filter(|w| (w[0] > mean) != (w[1] > mean))
        .count()
}

/// How much the row changes from pixel to pixel.
fn roughness(row: &[u32]) -> u32 {
    row.windows(2).map(|w| w[0].abs_diff(w[1])).sum()
}

#[test]
fn a_mask_breaks_up_the_light_of_spot_and_directional_lights() {
    use houdon_core::{LightKind, LightMask};
    let Some(mut renderer) = renderer() else {
        return;
    };
    for kind in [LightKind::Spot, LightKind::Directional] {
        let plain = middle_row(&mut renderer, &floor_under(kind, None));
        let mut mask = LightMask::new("stripes", striped_mask());
        mask.pattern_size = 1.0;
        let masked = middle_row(&mut renderer, &floor_under(kind, Some(mask)));
        assert!(
            *plain.iter().min().unwrap() > 20,
            "{kind:?}: the floor is lit: {plain:?}"
        );
        assert!(
            roughness(&masked) > roughness(&plain) * 3 + 100,
            "{kind:?}: the mask shows on the floor: plain {plain:?}, masked {masked:?}"
        );
        assert!(
            crossings(&masked) >= 4,
            "{kind:?}: stripes of light and shadow: {masked:?}"
        );
    }
}

#[test]
fn a_smaller_pattern_repeats_the_mask_more_often() {
    use houdon_core::{LightKind, LightMask};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let with_size = |renderer: &mut OffscreenRenderer, size: f32| {
        let mut mask = LightMask::new("stripes", striped_mask());
        mask.pattern_size = size;
        crossings(&middle_row(
            renderer,
            &floor_under(LightKind::Directional, Some(mask)),
        ))
    };
    let large = with_size(&mut renderer, 2.0);
    let small = with_size(&mut renderer, 1.0);
    assert!(large >= 2, "the large pattern shows: {large}");
    assert!(
        small * 10 >= large * 15,
        "half the size, about twice the stripes: {large} then {small}"
    );
}

/// The figure in the Solid mode with `lighting`.
fn solid_figure(lighting: houdon_core::SolidLighting) -> Scene {
    let mut scene = figure_scene();
    scene.shading.mode = houdon_core::ShadingMode::Solid;
    scene.shading.solid = lighting;
    scene
}

fn sharp(renderer: &mut OffscreenRenderer, scene: &Scene) -> Image {
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

/// Ground with a ridge at x = -0.4 and a trench at x = +0.4, seen from the
/// front and above, with the room hidden.
fn ridge_and_trench(mode: houdon_core::ShadingMode) -> Scene {
    let height = |x: f32| {
        0.12 * (1.0 - (x + 0.4).abs() / 0.15).max(0.0)
            - 0.12 * (1.0 - (x - 0.4).abs() / 0.15).max(0.0)
    };
    let n = 60;
    let mut positions = Vec::new();
    for j in 0..=n / 2 {
        for i in 0..=n {
            let x = -1.0 + 2.0 * i as f32 / n as f32;
            let z = -0.5 + j as f32 / (n / 2) as f32;
            positions.push(glam::Vec3::new(x, height(x) + 0.2, z));
        }
    }
    let mut indices = Vec::new();
    for j in 0..n / 2 {
        for i in 0..n {
            let a = (j * (n + 1) + i) as u32;
            let (b, c, d) = (a + 1, a + n as u32 + 1, a + n as u32 + 2);
            indices.extend([a, c, b, b, c, d]);
        }
    }
    let mut scene = Scene::new();
    scene.room.visible = false;
    let id = scene.add_model(Model::from_mesh("ground", positions, indices));
    let object = scene.add_object(id).unwrap();
    scene.set_transform(object, houdon_core::Transform::default());
    scene.select(None);
    scene.shading.mode = mode;
    scene.shading.solid = houdon_core::SolidLighting::Flat;
    scene.camera.target = glam::Vec3::new(0.0, 0.2, 0.0);
    scene.camera.distance = 3.0;
    scene.camera.pitch = 1.1;
    scene.camera.yaw = 0.0;
    scene
}

/// Brightness on the ridge and in the trench.
fn ridge_and_trench_brightness(renderer: &mut OffscreenRenderer, scene: &Scene) -> (u32, u32) {
    let image = sharp(renderer, scene);
    let to_world = scene.object_matrix(&scene.objects()[0]);
    let at = |x: f32, y: f32| {
        let (px, py) = pixel_of(scene, to_world.transform_point3(glam::Vec3::new(x, y, 0.0)));
        brightness(image.pixel(px, py))
    };
    (at(-0.4, 0.32), at(0.4, 0.08))
}

#[test]
fn cavity_brightens_ridges_and_darkens_hollows() {
    use houdon_core::{Cavity, ShadingMode};
    let Some(mut renderer) = renderer() else {
        return;
    };
    for mode in [ShadingMode::Solid, ShadingMode::Lit] {
        let plain_scene = ridge_and_trench(mode);
        let plain = ridge_and_trench_brightness(&mut renderer, &plain_scene);
        for (kind, cavity) in [
            (
                "geometry",
                Cavity {
                    geometry: true,
                    ..Cavity::default()
                },
            ),
            (
                "screen",
                Cavity {
                    screen: true,
                    ..Cavity::default()
                },
            ),
        ] {
            let mut scene = ridge_and_trench(mode);
            scene.shading.cavity = cavity;
            let (ridge, trench) = ridge_and_trench_brightness(&mut renderer, &scene);
            assert!(
                ridge > plain.0 + 10,
                "{mode:?} {kind}: ridge brighter ({} -> {ridge})",
                plain.0
            );
            assert!(
                trench + 10 < plain.1,
                "{mode:?} {kind}: trench darker ({} -> {trench})",
                plain.1
            );
        }
    }
}
