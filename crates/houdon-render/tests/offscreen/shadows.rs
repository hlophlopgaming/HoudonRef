//! Shadows of every light kind, the atlas and its cache.

use crate::*;

/// A 30 cm sphere floating 30 cm above the floor in a dark room, lit only
/// by one `kind` light straight above it, seen from the front and above.
pub(crate) fn sphere_over_floor(kind: houdon_core::LightKind, casts_shadows: bool) -> Scene {
    sphere_lit_from(kind, casts_shadows, 0.0, std::f32::consts::FRAC_PI_2)
}

/// The same sphere lit by one `kind` light at `azimuth` and `elevation`.
pub(crate) fn sphere_lit_from(
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
pub(crate) fn pixel_of(scene: &Scene, point: glam::Vec3) -> (u32, u32) {
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
