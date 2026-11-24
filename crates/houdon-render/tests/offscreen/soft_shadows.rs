//! Soft shadows: smooth edges, and penumbras that widen with distance.

use crate::*;

/// Brightness along the pixel row through `point`, without anti-aliasing,
/// so only the shadow's own filtering shows.
pub(crate) fn row_through(
    renderer: &mut OffscreenRenderer,
    scene: &Scene,
    point: glam::Vec3,
) -> Vec<u32> {
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
pub(crate) fn penumbra_pixels(row: &[u32]) -> usize {
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
pub(crate) fn shadow_edge_scene(kind: houdon_core::LightKind, shadow_softness: f32) -> Scene {
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
pub(crate) fn cube(half: f32) -> Model {
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
