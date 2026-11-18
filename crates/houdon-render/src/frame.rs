//! What to draw in one picture, taken from a scene.

use std::sync::Arc;

use glam::Mat4;
use houdon_core::{Lighting, Model, Scene};

/// One object to draw.
#[derive(Clone)]
pub struct FrameObject {
    /// Cache key for the model's GPU data; see [`houdon_core::ModelId`].
    pub model_key: u64,
    pub model: Arc<Model>,
    pub transform: Mat4,
}

/// Everything the renderer needs for one picture. Cheap to build every
/// frame: models are shared, not copied.
#[derive(Clone)]
pub struct Frame {
    pub objects: Vec<FrameObject>,
    pub view_proj: Mat4,
    pub lighting: Lighting,
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
                    model_key: object.model.as_u64(),
                    model: Arc::clone(model),
                    transform: object.transform.matrix(),
                })
            })
            .collect();
        Self {
            objects,
            view_proj: scene.camera.view_projection(aspect),
            lighting: scene.lighting,
            background: scene.background,
        }
    }
}
