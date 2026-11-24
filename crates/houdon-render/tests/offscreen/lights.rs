//! Lights: kinds, distance, cones, many lights.

use crate::*;

/// One framed figure lit only by `light` (no ambient light).
pub(crate) fn figure_lit_by(light: houdon_core::Light) -> Scene {
    let mut scene = figure_scene();
    scene.room.ambient_intensity = 0.0;
    let key = scene.lights()[0].id;
    scene.remove_light(key);
    let id = scene.add_light(light.kind).unwrap();
    scene.set_light(houdon_core::Light { id, ..light });
    scene
}

pub(crate) fn brightness(p: [u8; 4]) -> u32 {
    u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])
}

pub(crate) fn centre_brightness(image: &Image) -> u32 {
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
