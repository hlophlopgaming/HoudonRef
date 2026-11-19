//! What to draw in one picture, taken from a scene.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use houdon_core::{Ambient, Light, Model, ModelId, Scene};

/// One object to draw.
#[derive(Clone)]
pub struct FrameObject {
    /// Which model this is; GPU data is cached per model.
    pub model_id: ModelId,
    pub model: Arc<Model>,
    pub transform: Mat4,
    /// Drawn with the selection highlight.
    pub selected: bool,
}

/// Everything the renderer needs for one picture. Cheap to build every
/// frame: models are shared, not copied.
#[derive(Clone)]
pub struct Frame {
    pub objects: Vec<FrameObject>,
    pub view_proj: Mat4,
    /// Enabled lights only.
    pub lights: Vec<Light>,
    /// Where point and spot lights are placed around and spot lights aim.
    pub light_centre: Vec3,
    pub ambient: Ambient,
    /// Background colour, sRGB.
    pub background: [u8; 3],
}

impl Frame {
    /// Takes a picture of `scene` through its camera at the given aspect ratio.
    pub fn from_scene(scene: &Scene, aspect: f32) -> Self {
        let objects = scene
            .objects()
            .iter()
            .filter_map(|object| {
                let model = scene.model(object.model)?;
                Some(FrameObject {
                    model_id: object.model,
                    model: Arc::clone(model),
                    transform: scene.object_matrix(object),
                    selected: scene.selected() == Some(object.id),
                })
            })
            .collect();
        Self {
            objects,
            view_proj: scene.camera.view_projection(aspect),
            lights: scene
                .lights()
                .iter()
                .filter(|l| l.enabled)
                .cloned()
                .collect(),
            light_centre: scene.light_centre(),
            ambient: scene.ambient,
            background: scene.background,
        }
    }
}
