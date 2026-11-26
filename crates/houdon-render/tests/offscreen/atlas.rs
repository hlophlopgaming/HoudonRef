//! The Atlas's body: layers and see-through muscles.

use std::sync::OnceLock;

use houdon_core::atlas::ATLAS_MANIFEST;
use houdon_core::{Atlas, AtlasPack, AtlasView, Layer, LayerState};

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
    view.scene.shading.color = houdon_core::ColorSource::Clay;
    let p = centre(&mut renderer, &view);
    let spread = p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap();
    assert!(spread < 16, "clay is grey: {p:?}");
}
