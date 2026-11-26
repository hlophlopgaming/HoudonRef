//! Quality levels change what the renderer spends, and nothing else.

use houdon_render::{QualityLevel, RenderQuality};

use crate::shadows::sphere_over_floor;
use crate::*;

fn render_at(renderer: &mut OffscreenRenderer, scene: &Scene, quality: RenderQuality) -> Image {
    let mut frame = Frame::from_scene(scene, 1.0);
    frame.quality = quality;
    renderer.render(&frame, SIZE, SIZE)
}

#[test]
fn the_level_sets_the_shadow_atlas_and_switching_back_restores_the_picture() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = sphere_over_floor(houdon_core::LightKind::Spot, true);
    let low = render_at(&mut renderer, &scene, QualityLevel::Low.budget());
    assert_eq!(renderer.shadow_atlas_size(), 1024);
    let high = render_at(&mut renderer, &scene, QualityLevel::High.budget());
    assert_eq!(
        renderer.shadow_atlas_size(),
        4096.min(renderer.max_side()),
        "as large as the device allows"
    );
    let low_again = render_at(&mut renderer, &scene, QualityLevel::Low.budget());
    assert!(low == low_again, "the same level gives the same picture");
    if renderer.shadow_atlas_size() != 4096 {
        return;
    }
    assert!(
        low != high,
        "a finer shadow map gives a different shadow edge"
    );
}

#[test]
fn the_level_caps_ambient_occlusion_samples() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = figure_scene();
    scene.room.visible = true;
    scene.shading.ambient_occlusion.samples = 32;
    let fine = render_at(&mut renderer, &scene, QualityLevel::High.budget());
    let coarse = render_at(&mut renderer, &scene, QualityLevel::Low.budget());
    let medium = render_at(&mut renderer, &scene, QualityLevel::Medium.budget());
    assert!(
        fine != coarse && medium != coarse,
        "fewer samples on a lower level"
    );
    // A scene asking for fewer samples than the cap is unchanged by it.
    scene.shading.ambient_occlusion.samples = 4;
    let a = render_at(&mut renderer, &scene, QualityLevel::Low.budget());
    let b = render_at(&mut renderer, &scene, QualityLevel::High.budget());
    let differing = a
        .pixels()
        .zip(b.pixels())
        .filter(|(p, q)| p[..3].iter().zip(&q[..3]).any(|(x, y)| x.abs_diff(*y) > 2))
        .count();
    assert!(
        differing < (SIZE * SIZE / 50) as usize,
        "{differing} pixels differ"
    );
}
