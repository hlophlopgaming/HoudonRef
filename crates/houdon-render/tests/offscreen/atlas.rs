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
