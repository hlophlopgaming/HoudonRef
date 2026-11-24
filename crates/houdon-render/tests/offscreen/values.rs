//! The value study: grey, tones and squint.

use crate::*;

pub(crate) fn distinct_colours(image: &Image) -> usize {
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
