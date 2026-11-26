//! Ambient occlusion in the Lit mode.

use crate::soft_shadows::cube;
use crate::*;

/// A 50 cm box on the floor of the room, seen from the front and above.
fn box_on_the_floor(ambient_occlusion: bool) -> Scene {
    let mut scene = Scene::new();
    let id = scene.add_model(cube(0.25));
    let object = scene.add_object(id).unwrap();
    scene.set_transform(object, houdon_core::Transform::default());
    scene.select(None);
    scene.shading.ambient_occlusion.enabled = ambient_occlusion;
    scene.camera.target = glam::Vec3::new(0.0, 0.2, 0.0);
    scene.camera.distance = 3.0;
    scene.camera.pitch = 0.6;
    scene.camera.yaw = 0.0;
    scene
}

#[test]
fn ambient_occlusion_darkens_the_corner_and_leaves_open_floor_alone() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let off = box_on_the_floor(false);
    let on = box_on_the_floor(true);
    let (plain, occluded) = (sharp(&mut renderer, &off), sharp(&mut renderer, &on));
    let at = |image: &Image, point: glam::Vec3| {
        let (x, y) = pixel_of(&off, point);
        assert!(
            x > 0 && x < SIZE - 1 && y > 0 && y < SIZE - 1,
            "{point} in view"
        );
        brightness(image.pixel(x, y))
    };
    // The floor right in front of the box, where it meets the floor.
    let corner = glam::Vec3::new(0.0, 0.0, 0.27);
    assert!(
        at(&occluded, corner) + 12 < at(&plain, corner),
        "the corner is darker: {} -> {}",
        at(&plain, corner),
        at(&occluded, corner)
    );
    // Open floor beyond the radius from the box.
    let open = glam::Vec3::new(0.0, 0.0, 0.6);
    assert!(
        at(&occluded, open).abs_diff(at(&plain, open)) <= 3,
        "open floor unchanged: {} -> {}",
        at(&plain, open),
        at(&occluded, open)
    );
}

#[test]
fn ambient_occlusion_is_for_the_lit_mode_only() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut off = box_on_the_floor(false);
    let mut on = box_on_the_floor(true);
    for scene in [&mut off, &mut on] {
        scene.shading.mode = houdon_core::ShadingMode::Solid;
    }
    assert!(
        sharp(&mut renderer, &off) == sharp(&mut renderer, &on),
        "Solid shows form without it"
    );
}

#[test]
fn open_floor_has_no_speckles_with_multisampling() {
    // With MSAA the picture of surfaces must not be averaged between
    // samples: that broke its distances into specks of false occlusion.
    let Some(mut renderer) = renderer() else {
        return;
    };
    let render = |renderer: &mut OffscreenRenderer, ambient_occlusion| {
        let scene = box_on_the_floor(ambient_occlusion);
        renderer.render_with(
            &Frame::from_scene(&scene, 1.0),
            SIZE,
            SIZE,
            AntiAliasing::VIEWPORT,
        )
    };
    let (plain, occluded) = (render(&mut renderer, false), render(&mut renderer, true));
    // The bottom rows: floor in front of the camera, far from the box.
    let speckles = (SIZE - 6..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            brightness(occluded.pixel(x, y)).abs_diff(brightness(plain.pixel(x, y))) > 6
        })
        .count();
    assert_eq!(speckles, 0, "open floor stays clean");
}
