//! The room: floor and backdrop around the objects, and their reflex.

use crate::*;

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
pub(crate) fn sphere(r: f32, n: u32) -> Model {
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
