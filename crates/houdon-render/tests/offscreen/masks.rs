//! Light masks on spot and directional lights.

use crate::*;

/// Vertical stripes, eight light and eight dark.
pub(crate) fn striped_mask() -> std::sync::Arc<houdon_core::MaskImage> {
    std::sync::Arc::new(houdon_core::MaskImage::from_fn(64, 64, |u, _| {
        if (u * 8.0).fract() < 0.5 { 255 } else { 0 }
    }))
}

/// The empty floor seen from above, lit straight down by one `kind` light
/// through `mask`, if any.
pub(crate) fn floor_under(
    kind: houdon_core::LightKind,
    mask: Option<houdon_core::LightMask>,
) -> Scene {
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
pub(crate) fn middle_row(renderer: &mut OffscreenRenderer, scene: &Scene) -> Vec<u32> {
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
pub(crate) fn crossings(row: &[u32]) -> usize {
    let mean = row.iter().sum::<u32>() / row.len() as u32;
    row.windows(2)
        .filter(|w| (w[0] > mean) != (w[1] > mean))
        .count()
}

/// How much the row changes from pixel to pixel.
pub(crate) fn roughness(row: &[u32]) -> u32 {
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
