//! What the Atlas tab shows: which layers, how solid, and the camera, light
//! and shading it is seen with.

use glam::Vec3;

use super::{Atlas, BodySystem, Structure, Tissue};
use crate::{Bounds, ModelId, Scene};

/// A group of structures shown or hidden together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Bones, teeth and the cartilage of the skeleton.
    Bones,
    /// Muscles and their tendons.
    Muscles,
    /// The sheets around muscles and the bursae between them, which would
    /// hide the muscles' forms; off at first.
    Fasciae,
    /// Ligaments, joint capsules and the cartilage and fat of joints.
    Joints,
}

impl Layer {
    pub const ALL: [Self; 4] = [Self::Bones, Self::Muscles, Self::Fasciae, Self::Joints];

    /// The layer a structure is shown with.
    pub fn of(structure: &Structure) -> Self {
        match (structure.system, structure.tissue) {
            (BodySystem::Joints, _) => Self::Joints,
            (_, Tissue::Muscle | Tissue::Tendon) => Self::Muscles,
            (_, Tissue::Fascia | Tissue::Bursa) => Self::Fasciae,
            (_, Tissue::Ligament | Tissue::Capsule | Tissue::Fat) => Self::Joints,
            (BodySystem::Skeletal, _) => Self::Bones,
            (_, Tissue::Bone | Tissue::Tooth | Tissue::Cartilage) => Self::Joints,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// How a layer is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerState {
    pub visible: bool,
    /// How solid it is drawn: 1 solid, less to see what lies under it.
    pub opacity: f32,
}

impl LayerState {
    /// The least opacity a see-through layer gets, so it never vanishes.
    pub const MIN_OPACITY: f32 = 0.1;

    const SHOWN: Self = Self {
        visible: true,
        opacity: 1.0,
    };
    const HIDDEN: Self = Self {
        visible: false,
        opacity: 1.0,
    };
}

impl Tissue {
    /// The colour of the tissue as it looks, linear RGB.
    pub fn natural_color(self) -> Vec3 {
        match self {
            Self::Bone => Vec3::new(0.78, 0.72, 0.58),
            Self::Tooth => Vec3::new(0.86, 0.84, 0.76),
            Self::Cartilage => Vec3::new(0.62, 0.70, 0.74),
            Self::Muscle => Vec3::new(0.50, 0.10, 0.09),
            Self::Tendon => Vec3::new(0.78, 0.74, 0.66),
            Self::Fascia => Vec3::new(0.70, 0.66, 0.62),
            Self::Bursa => Vec3::new(0.66, 0.70, 0.74),
            Self::Ligament => Vec3::new(0.74, 0.70, 0.62),
            Self::Capsule => Vec3::new(0.68, 0.66, 0.62),
            Self::Fat => Vec3::new(0.80, 0.70, 0.40),
        }
    }
}

/// One structure to draw.
#[derive(Debug, Clone, Copy)]
pub struct ShownStructure<'a> {
    pub structure: &'a Structure,
    pub model_id: ModelId,
    pub opacity: f32,
}

/// The Atlas tab: the body, its layers and the view of it.
pub struct AtlasView {
    atlas: Atlas,
    /// The GPU cache key of each structure's model, in the atlas's order.
    model_ids: Vec<ModelId>,
    layers: [LayerState; 4],
    bounds: Bounds,
    /// The camera, lights, room and shading the body is seen with; it
    /// holds no objects of its own.
    pub scene: Scene,
}

impl AtlasView {
    pub fn new(atlas: Atlas) -> Self {
        let model_ids = atlas.structures().iter().map(|_| ModelId::next()).collect();
        let bounds = atlas
            .structures()
            .iter()
            .fold(Bounds::EMPTY, |b, s| b.union(&s.model.bounds));
        let mut scene = Scene::new();
        // A plain backdrop: the room is sized for the Studio's objects.
        scene.room.visible = false;
        let mut view = Self {
            atlas,
            model_ids,
            layers: [
                LayerState::SHOWN,
                LayerState::SHOWN,
                LayerState::HIDDEN,
                LayerState::HIDDEN,
            ],
            bounds,
            scene,
        };
        view.frame(1.0);
        view
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn layer(&self, layer: Layer) -> LayerState {
        self.layers[layer.index()]
    }

    pub fn set_layer(&mut self, layer: Layer, state: LayerState) {
        self.layers[layer.index()] = LayerState {
            visible: state.visible,
            opacity: state.opacity.clamp(LayerState::MIN_OPACITY, 1.0),
        };
    }

    /// Everything shown, with how solid each is.
    pub fn shown(&self) -> impl Iterator<Item = ShownStructure<'_>> {
        self.atlas
            .structures()
            .iter()
            .zip(&self.model_ids)
            .filter_map(|(structure, &model_id)| {
                let state = self.layers[Layer::of(structure).index()];
                state.visible.then_some(ShownStructure {
                    structure,
                    model_id,
                    opacity: state.opacity,
                })
            })
    }

    /// The bounds of what is shown, or of the whole body when nothing is.
    pub fn shown_bounds(&self) -> Bounds {
        let bounds = self
            .shown()
            .fold(Bounds::EMPTY, |b, s| b.union(&s.structure.model.bounds));
        if bounds.is_empty() {
            self.bounds
        } else {
            bounds
        }
    }

    /// Points the camera at what is shown.
    pub fn frame(&mut self, aspect: f32) {
        let bounds = self.shown_bounds();
        self.scene.camera.frame(&bounds, aspect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::tests::human;

    #[test]
    fn every_structure_has_one_layer_and_the_layers_split_sensibly() {
        let atlas = human();
        let count = |layer| {
            atlas
                .structures()
                .iter()
                .filter(|s| Layer::of(s) == layer)
                .count()
        };
        assert!(count(Layer::Bones) > 250);
        assert!(count(Layer::Muscles) > 400);
        assert!(count(Layer::Fasciae) > 100);
        assert!(count(Layer::Joints) > 300);
        let femur = atlas.structure("Femur.l").unwrap();
        assert_eq!(Layer::of(femur), Layer::Bones);
    }

    #[test]
    fn hidden_layers_are_not_shown_and_see_through_ones_carry_their_opacity() {
        let mut view = AtlasView::new(human());
        let shown = |view: &AtlasView, layer| {
            view.shown()
                .filter(|s| Layer::of(s.structure) == layer)
                .map(|s| s.opacity)
                .collect::<Vec<_>>()
        };
        assert!(!shown(&view, Layer::Muscles).is_empty(), "muscles at first");
        assert!(
            shown(&view, Layer::Fasciae).is_empty(),
            "fasciae hidden at first"
        );
        let see_through = |opacity| LayerState {
            visible: true,
            opacity,
        };
        view.set_layer(Layer::Muscles, see_through(0.3));
        assert!(shown(&view, Layer::Muscles).iter().all(|&o| o == 0.3));
        view.set_layer(Layer::Muscles, see_through(0.0));
        assert!(
            shown(&view, Layer::Muscles)
                .iter()
                .all(|&o| o == LayerState::MIN_OPACITY),
            "never quite invisible"
        );
        view.set_layer(Layer::Muscles, LayerState::HIDDEN);
        assert!(shown(&view, Layer::Muscles).is_empty());
        assert!(!shown(&view, Layer::Bones).is_empty());
    }

    #[test]
    fn the_camera_frames_the_whole_body_and_each_structure_keeps_its_gpu_key() {
        let view = AtlasView::new(human());
        let bounds = view.shown_bounds();
        assert!(
            (1.5..2.0).contains(&bounds.size().y),
            "a person: {}",
            bounds.size().y
        );
        let target = view.scene.camera.target;
        assert!(
            (target - bounds.center()).length() < 0.05,
            "aimed at the body: {target}"
        );
        let first: Vec<_> = view.shown().map(|s| s.model_id).collect();
        let again: Vec<_> = view.shown().map(|s| s.model_id).collect();
        assert_eq!(first, again);
    }
}
