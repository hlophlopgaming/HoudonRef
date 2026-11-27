//! The Atlas's body: layers and see-through muscles.

use std::sync::OnceLock;

use houdon_core::atlas::ATLAS_MANIFEST;
use houdon_core::{Atlas, AtlasColors, AtlasPack, AtlasView, Layer, LayerState};

use crate::*;

pub(crate) fn human() -> Atlas {
    static HUMAN: OnceLock<Atlas> = OnceLock::new();
    HUMAN
        .get_or_init(|| {
            let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/atlas/human/");
            let read = |name: &str| std::fs::read(format!("{directory}{name}")).ok();
            AtlasPack::parse(&read(ATLAS_MANIFEST).unwrap())
                .unwrap()
                .build(read)
                .unwrap()
        })
        .clone()
}

/// The front of the left thigh, close up: muscle over the femur.
fn thigh(muscles: LayerState) -> AtlasView {
    let mut view = AtlasView::new(human());
    view.colors = AtlasColors::Natural;
    view.set_layer(Layer::Muscles, muscles);
    let femur = view.atlas().structure("Femur.l").unwrap().model.bounds;
    view.scene.camera.target = femur.center();
    view.scene.camera.distance = 0.5;
    view.scene.camera.yaw = 0.0;
    view.scene.camera.pitch = 0.0;
    view
}

fn centre(renderer: &mut OffscreenRenderer, view: &AtlasView) -> [u8; 4] {
    let image = renderer.render(&Frame::from_atlas(view, 1.0), SIZE, SIZE);
    image.pixel(SIZE / 2, SIZE / 2)
}

#[test]
fn a_pose_updates_the_muscle_under_its_existing_gpu_key() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = AtlasView::new(human());
    assert!(view.enable_pose());
    let muscle = view.index_of("Long head of biceps brachii.l").unwrap();
    view.isolate([muscle]);
    view.frame(1.0);
    let elbow = view
        .pose_joints()
        .iter()
        .position(|joint| joint.name == "elbow" && joint.side == houdon_core::Side::Left)
        .unwrap();
    let rest = Frame::from_atlas(&view, 1.0);
    let rest_image = renderer.render(&rest, SIZE, SIZE);
    view.set_joint_angles(elbow, glam::Vec3::new(-1.25, 0.0, 0.0));
    let bent = Frame::from_atlas(&view, 1.0);
    assert_eq!(rest.objects[0].model_id, bent.objects[0].model_id);
    assert_eq!(rest.objects[0].transform, bent.objects[0].transform);
    assert_ne!(rest_image, renderer.render(&bent, SIZE, SIZE));
    view.reset_pose();
    assert_eq!(
        rest_image,
        renderer.render(&Frame::from_atlas(&view, 1.0), SIZE, SIZE)
    );
}

#[test]
fn see_through_muscles_show_the_bone_under_them() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let solid = centre(
        &mut renderer,
        &thigh(LayerState {
            visible: true,
            opacity: 1.0,
        }),
    );
    let faint = centre(
        &mut renderer,
        &thigh(LayerState {
            visible: true,
            opacity: 0.25,
        }),
    );
    let hidden = centre(
        &mut renderer,
        &thigh(LayerState {
            visible: false,
            opacity: 1.0,
        }),
    );
    // Muscle is red; bone is ivory, its green close to its red.
    let redness = |p: [u8; 4]| i32::from(p[0]) - i32::from(p[1]);
    assert!(redness(solid) > 40, "muscle in front: {solid:?}");
    assert!(redness(hidden) < 40, "the bone alone: {hidden:?}");
    assert!(
        redness(faint) < redness(solid) && redness(faint) > redness(hidden),
        "between the two: {solid:?} {faint:?} {hidden:?}"
    );
}

#[test]
fn selecting_the_bone_fades_the_muscle_in_front_and_hovering_tints_lightly() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = thigh(LayerState::SHOWN);
    let plain = centre(&mut renderer, &view);
    let femur = view.index_of("Femur.l").unwrap();
    let muscle = view
        .shown()
        .map(|s| s.index)
        .find(|&i| {
            let id = &view.atlas().structures()[i].id;
            id.starts_with("Rectus femoris") || id.starts_with("Vastus intermedius")
        })
        .unwrap();
    // The muscle under the pointer: tinted, a little.
    view.hovered = Some(muscle);
    let hovered = centre(&mut renderer, &view);
    view.hovered = None;
    // The bone selected: the muscle in front fades and the bone shows.
    view.select(Some(femur));
    let selected = centre(&mut renderer, &view);
    let redness = |p: [u8; 4]| i32::from(p[0]) - i32::from(p[1]);
    assert!(redness(plain) > 40, "muscle in front: {plain:?}");
    assert!(
        redness(selected) < redness(plain) - 30,
        "the bone shows: {plain:?} {selected:?}"
    );
    let distance = |a: [u8; 4], b: [u8; 4]| (0..3).map(|i| a[i].abs_diff(b[i]) as u32).sum::<u32>();
    assert!(
        distance(plain, hovered) > 3,
        "a tint: {plain:?} {hovered:?}"
    );
}

#[test]
fn the_low_back_shows_the_pale_aponeurosis_not_the_muscle_colour() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = AtlasView::new(human());
    // From behind, close on the low back, beside the spine.
    view.scene.camera.target = glam::Vec3::new(0.02, 0.975, 0.0);
    view.scene.camera.distance = 0.35;
    view.scene.camera.yaw = std::f32::consts::PI;
    view.scene.camera.pitch = 0.0;
    let latissimus = view.atlas().structure("Latissimus dorsi muscle.l").unwrap();
    let belly = view.tissue_color(latissimus, houdon_core::Tissue::Muscle);
    let pixel = centre(&mut renderer, &view);
    let channel = |p: [u8; 4], i: usize| i32::from(p[i]);
    // The latissimus's colour is saturated; the aponeurosis is near grey.
    let spread = |p: [u8; 4]| {
        (0..3).map(|i| channel(p, i)).max().unwrap() - (0..3).map(|i| channel(p, i)).min().unwrap()
    };
    assert!(
        belly.max_element() - belly.min_element() > 0.1,
        "a coloured mass: {belly}"
    );
    assert!(spread(pixel) < 40, "pale over the low back: {pixel:?}");
}

#[test]
fn clay_paints_every_structure_alike() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = thigh(LayerState {
        visible: true,
        opacity: 1.0,
    });
    view.colors = AtlasColors::Clay;
    let p = centre(&mut renderer, &view);
    let spread = p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap();
    assert!(spread < 16, "clay is grey: {p:?}");
}

/// The mean colour of what is drawn of `ids` alone, framed.
fn mean_colour(renderer: &mut OffscreenRenderer, view: &mut AtlasView, ids: &[&str]) -> [f32; 3] {
    let indices: Vec<_> = ids.iter().map(|id| view.index_of(id).unwrap()).collect();
    view.isolate(indices);
    view.frame(1.0);
    let frame = Frame::from_atlas(view, 1.0);
    let background = frame.clear_color();
    let image = renderer.render(&frame, SIZE, SIZE);
    let drawn: Vec<_> = image
        .pixels()
        .filter(|p| {
            p[..3]
                .iter()
                .zip(background)
                .any(|(a, b)| a.abs_diff(b) > 8)
        })
        .collect();
    assert!(drawn.len() > 50, "{ids:?} drawn");
    let mut sum = [0.0; 3];
    for p in &drawn {
        for c in 0..3 {
            sum[c] += f32::from(p[c]) / drawn.len() as f32;
        }
    }
    sum
}

#[test]
fn neighbouring_masses_render_in_different_colours() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let pectoral = ["Clavicular head of pectoralis major muscle.l"];
    let deltoid = ["Clavicular part of deltoid muscle.l"];
    let mut view = AtlasView::new(human());
    let difference = |a: [f32; 3], b: [f32; 3]| {
        // Compare hue, not brightness: each colour over its own sum.
        let norm = |c: [f32; 3]| c.map(|v| v / (c[0] + c[1] + c[2]).max(1.0));
        let (a, b) = (norm(a), norm(b));
        (0..3).map(|i| (a[i] - b[i]).abs()).sum::<f32>()
    };
    let masses = difference(
        mean_colour(&mut renderer, &mut view, &pectoral),
        mean_colour(&mut renderer, &mut view, &deltoid),
    );
    view.colors = AtlasColors::Natural;
    let natural = difference(
        mean_colour(&mut renderer, &mut view, &pectoral),
        mean_colour(&mut renderer, &mut view, &deltoid),
    );
    assert!(natural < 0.05, "one muscle colour: {natural}");
    assert!(masses > 0.2, "two masses: {masses}");
}

#[test]
fn the_bones_mode_colours_neighbouring_skull_bones_differently() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut view = AtlasView::new(human());
    view.colors = AtlasColors::Bones;
    let frontal = mean_colour(&mut renderer, &mut view, &["Frontal bone"]);
    let parietal = mean_colour(&mut renderer, &mut view, &["Parietal bone.l"]);
    let norm = |c: [f32; 3]| c.map(|v| v / (c[0] + c[1] + c[2]).max(1.0));
    let (a, b) = (norm(frontal), norm(parietal));
    let difference: f32 = (0..3).map(|i| (a[i] - b[i]).abs()).sum();
    assert!(
        difference > 0.15,
        "frontal {frontal:?}, parietal {parietal:?}"
    );
}
