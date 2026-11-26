//! Overlays: cavity, contour lines, wireframe and backface culling.

use crate::*;

/// Ground with a ridge at x = -0.4 and a trench at x = +0.4, seen from the
/// front and above, with the room hidden.
pub(crate) fn ridge_and_trench(mode: houdon_core::ShadingMode) -> Scene {
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
pub(crate) fn ridge_and_trench_brightness(
    renderer: &mut OffscreenRenderer,
    scene: &Scene,
) -> (u32, u32) {
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

pub(crate) const RED_LINES: glam::Vec3 = glam::Vec3::new(1.0, 0.0, 0.0);

pub(crate) fn is_red(p: [u8; 4]) -> bool {
    i32::from(p[0]) > i32::from(p[1]) + 80 && i32::from(p[0]) > i32::from(p[2]) + 80
}

#[test]
fn contour_lines_draw_outlines_and_leave_flat_parts_alone() {
    use houdon_core::{Lines, ShadingMode};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let plain_scene = ridge_and_trench(ShadingMode::Solid);
    let plain = sharp(&mut renderer, &plain_scene);
    let mut scene = ridge_and_trench(ShadingMode::Solid);
    scene.shading.lines = Lines {
        contours: true,
        color: RED_LINES,
        ..Lines::default()
    };
    let lined = sharp(&mut renderer, &scene);

    let background = Frame::from_scene(&scene, 1.0).clear_color();
    let is_ground = |x: u32, y: u32| plain.pixel(x, y)[..3] != background[..];
    let mut outline = 0;
    let mut drawn = 0;
    for y in 1..SIZE - 1 {
        for x in 1..SIZE - 1 {
            let at_edge = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .any(|&(nx, ny)| !is_ground(nx, ny));
            if is_ground(x, y) && at_edge {
                outline += 1;
                drawn += usize::from(is_red(lined.pixel(x, y)));
            }
        }
    }
    assert!(outline > 20, "the ground has an outline");
    assert!(
        drawn * 10 >= outline * 8,
        "{drawn} of {outline} outline pixels are drawn"
    );

    // Flat ground between the ridge and the trench.
    let to_world = scene.object_matrix(&scene.objects()[0]);
    let (fx, fy) = pixel_of(
        &scene,
        to_world.transform_point3(glam::Vec3::new(0.0, 0.2, 0.0)),
    );
    assert_eq!(
        lined.pixel(fx, fy),
        plain.pixel(fx, fy),
        "flat ground is left alone"
    );
}

#[test]
fn fold_sensitivity_adds_lines_on_inner_folds() {
    use houdon_core::{Lines, SolidLighting};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let plain = sharp(&mut renderer, &solid_figure(SolidLighting::Studio));
    let background = Frame::from_scene(&figure_scene(), 1.0).clear_color();
    // Figure pixels at least 3 pixels from its outline.
    let inside = |x: u32, y: u32| {
        (x.saturating_sub(3)..=(x + 3).min(SIZE - 1)).all(|nx| {
            (y.saturating_sub(3)..=(y + 3).min(SIZE - 1))
                .all(|ny| plain.pixel(nx, ny)[..3] != background[..])
        })
    };
    let inner_lines = |renderer: &mut OffscreenRenderer, sensitivity: f32| {
        let mut scene = solid_figure(SolidLighting::Studio);
        scene.shading.lines = Lines {
            contours: true,
            color: RED_LINES,
            fold_sensitivity: sensitivity,
            ..Lines::default()
        };
        let image = sharp(renderer, &scene);
        (0..SIZE * SIZE)
            .map(|i| (i % SIZE, i / SIZE))
            .filter(|&(x, y)| inside(x, y) && is_red(image.pixel(x, y)))
            .count()
    };
    let few = inner_lines(&mut renderer, 0.0);
    let many = inner_lines(&mut renderer, 1.0);
    assert!(many > few + 10, "more fold lines: {few} then {many}");
}

#[test]
fn a_wireframe_draws_the_edges_inside_a_model() {
    use houdon_core::{Lines, ShadingMode};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let count_red = |image: &Image| image.pixels().filter(|p| is_red(*p)).count();
    let plain = sharp(&mut renderer, &ridge_and_trench(ShadingMode::Solid));
    let mut scene = ridge_and_trench(ShadingMode::Solid);
    scene.shading.lines = Lines {
        wireframe: true,
        wireframe_color: RED_LINES,
        wireframe_opacity: 1.0,
        ..Lines::default()
    };
    let wired = sharp(&mut renderer, &scene);
    assert_eq!(count_red(&plain), 0);
    assert!(
        count_red(&wired) > 200,
        "edges drawn: {}",
        count_red(&wired)
    );
}

#[test]
fn backface_culling_hides_the_back_of_an_open_mesh() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    // A wall facing away from the camera: only its back is seen.
    let positions = vec![
        glam::Vec3::new(-0.5, 0.0, 0.0),
        glam::Vec3::new(0.5, 0.0, 0.0),
        glam::Vec3::new(0.5, 1.0, 0.0),
        glam::Vec3::new(-0.5, 1.0, 0.0),
    ];
    let make = |culling: bool| {
        let mut scene = Scene::new();
        scene.room.visible = false;
        let id = scene.add_model(Model::from_mesh(
            "wall",
            positions.clone(),
            vec![0, 2, 1, 0, 3, 2],
        ));
        scene.add_object(id).unwrap();
        scene.select(None);
        scene.frame_camera(1.0);
        scene.shading.backface_culling = culling;
        scene
    };
    let shown = make(false);
    let background = Frame::from_scene(&shown, 1.0).clear_color();
    let centre = sharp(&mut renderer, &shown).pixel(SIZE / 2, SIZE / 2);
    assert_ne!(
        centre[..3],
        background[..],
        "without culling the back is drawn"
    );
    let culled = make(true);
    let centre = sharp(&mut renderer, &culled).pixel(SIZE / 2, SIZE / 2);
    assert_eq!(centre[..3], background[..], "with culling it is hidden");
}

#[test]
fn contour_lines_have_soft_edges() {
    // A line's edge shades off over a pixel instead of a staircase: along
    // the outline there are pixels between the line's colour and the ground.
    use houdon_core::{Lines, ShadingMode};
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = ridge_and_trench(ShadingMode::Solid);
    scene.shading.lines = Lines {
        contours: true,
        color: RED_LINES,
        ..Lines::default()
    };
    let lined = sharp(&mut renderer, &scene);
    // The line's own colour, and a mix of it with what lies under it.
    let is_full = |p: [u8; 4]| p[0] >= 250 && p[1] <= 5 && p[2] <= 5;
    let full = lined.pixels().filter(|&p| is_full(p)).count();
    let partly = lined
        .pixels()
        .filter(|&p| !is_full(p) && i32::from(p[0]) > i32::from(p[1].max(p[2])) + 25)
        .count();
    assert!(full > 20, "{full} line pixels");
    assert!(
        partly * 10 > full,
        "{partly} soft edge pixels for {full} line pixels"
    );
}
