//! What to draw in one picture, taken from a scene.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use houdon_core::{Light, Model, ModelId, Room, Scene};

use crate::CLAY_COLOR;

/// The surface colour of an object, linear RGB. It changes with height
/// from `below` to `above` between `from_y` and `to_y` (world space); the
/// room uses this to sweep from floor to backdrop colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectColors {
    pub below: Vec3,
    pub above: Vec3,
    pub from_y: f32,
    pub to_y: f32,
}

impl ObjectColors {
    /// One colour everywhere.
    pub fn uniform(color: Vec3) -> Self {
        Self {
            below: color,
            above: color,
            from_y: 0.0,
            to_y: 1.0,
        }
    }
}

/// One object to draw.
#[derive(Clone)]
pub struct FrameObject {
    /// Which model this is; GPU data is cached per model.
    pub model_id: ModelId,
    pub model: Arc<Model>,
    pub transform: Mat4,
    pub colors: ObjectColors,
    /// Drawn with the selection highlight.
    pub selected: bool,
}

/// Everything the renderer needs for one picture. Cheap to build every
/// frame: models are shared, not copied.
#[derive(Clone)]
pub struct Frame {
    /// The room first, when it is shown, then the scene's objects.
    pub objects: Vec<FrameObject>,
    pub view_proj: Mat4,
    /// Enabled lights only.
    pub lights: Vec<Light>,
    /// Where point and spot lights are placed around and spot lights aim.
    pub light_centre: Vec3,
    /// The room's colours give the ambient light.
    pub room: Room,
}

impl Frame {
    /// Takes a picture of `scene` through its camera at the given aspect ratio.
    pub fn from_scene(scene: &Scene, aspect: f32) -> Self {
        let room = scene.room;
        let room_object =
            room.visible
                .then(|| scene.room_model())
                .flatten()
                .map(|(model_id, model, shape)| {
                    let cove = shape.cove_radius;
                    FrameObject {
                        model_id,
                        model,
                        transform: Mat4::IDENTITY,
                        colors: ObjectColors {
                            below: room.floor_color,
                            above: room.backdrop_color,
                            from_y: cove * 0.2,
                            to_y: cove,
                        },
                        selected: false,
                    }
                });
        let objects = scene.objects().iter().filter_map(|object| {
            let model = scene.model(object.model)?;
            Some(FrameObject {
                model_id: object.model,
                model: Arc::clone(model),
                transform: scene.object_matrix(object),
                colors: ObjectColors::uniform(CLAY_COLOR),
                selected: scene.selected() == Some(object.id),
            })
        });
        Self {
            objects: room_object.into_iter().chain(objects).collect(),
            view_proj: scene.camera.view_projection(aspect),
            lights: scene
                .lights()
                .iter()
                .filter(|l| l.enabled)
                .cloned()
                .collect(),
            light_centre: scene.light_centre(),
            room,
        }
    }

    /// The colour behind everything, 8-bit sRGB: the backdrop's.
    pub fn clear_color(&self) -> [u8; 3] {
        self.room.backdrop_srgb()
    }
}
