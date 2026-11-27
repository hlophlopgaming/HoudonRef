//! What to draw in one picture, taken from a scene.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use houdon_core::atlas::Fibres;
use houdon_core::{AtlasView, Light, Model, ModelId, Room, Scene, Shading, Surface, Tissue};

use crate::RenderQuality;

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

/// What covers a drawn object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameSurface {
    /// Plain colours, as for Clay, Random or the room.
    Colors(ObjectColors),
    /// The model's materials, part by part; parts without one are Clay.
    Materials,
    /// A plain colour part by part, picked by the part's material index:
    /// the first for index 0 or none, the second otherwise. Fibre lines
    /// go only on the parts whose `fibres` flag is set the same way.
    Parts {
        colors: [Vec3; 2],
        fibres: [bool; 2],
    },
}

/// How strongly an object is marked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Highlight {
    #[default]
    None,
    /// Under the pointer: a faint tint, a hint of what a click selects.
    Hovered,
    Selected,
}

/// One object to draw.
#[derive(Clone)]
pub struct FrameObject {
    /// Which model this is; GPU data is cached per model.
    pub model_id: ModelId,
    pub model: Arc<Model>,
    pub transform: Mat4,
    pub surface: FrameSurface,
    /// How strongly it is marked out: selected, under the pointer or not.
    pub highlight: Highlight,
    /// Drawn only from the side its normals face, like the room's inside.
    pub one_sided: bool,
    /// Casts shadows; the room only receives them.
    pub casts_shadows: bool,
    /// 1 for a solid object; less to see through it. See-through objects
    /// are drawn after the solid ones and leave no mark on the overlays.
    pub opacity: f32,
    /// Fine lines along a muscle's fibres, drawn on its surface.
    pub fibres: Option<Fibres>,
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
    /// Where the camera is, for highlights.
    pub eye: Vec3,
    /// How the picture is shaded and what is drawn over it: the scene's
    /// own settings (cheap to clone: images are shared).
    pub shading: Shading,
    /// The camera's right and up directions, for the Solid mode's light.
    pub camera_right: Vec3,
    pub camera_up: Vec3,
    /// The tangent of half the vertical field of view, and the picture's
    /// width over its height: to turn pixels back into points in space.
    pub tan_half_fov: f32,
    pub aspect: f32,
    /// How much the renderer may spend on shadows and ambient occlusion.
    pub quality: RenderQuality,
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
                        surface: FrameSurface::Colors(ObjectColors {
                            below: room.floor_color,
                            above: room.backdrop_color,
                            from_y: cove * 0.2,
                            to_y: cove,
                        }),
                        highlight: Highlight::None,
                        one_sided: true,
                        casts_shadows: false,
                        opacity: 1.0,
                        fibres: None,
                    }
                });
        let objects = scene.objects().iter().filter_map(|object| {
            let model = scene.model(object.model)?;
            Some(FrameObject {
                model_id: object.model,
                model: Arc::clone(model),
                transform: scene.object_matrix(object),
                surface: match scene.surface(object) {
                    Surface::Materials => FrameSurface::Materials,
                    Surface::Plain(color) => FrameSurface::Colors(ObjectColors::uniform(color)),
                },
                highlight: if scene.selected() == Some(object.id) {
                    Highlight::Selected
                } else {
                    Highlight::None
                },
                one_sided: false,
                casts_shadows: true,
                opacity: 1.0,
                fibres: None,
            })
        });
        let view = scene.camera.view();
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
            eye: scene.camera.eye(),
            shading: scene.shading.clone(),
            camera_right: view.row(0).truncate(),
            camera_up: view.row(1).truncate(),
            tan_half_fov: (scene.camera.vertical_fov() * 0.5).tan(),
            aspect,
            quality: RenderQuality::default(),
        }
    }

    /// Takes a picture of what the Atlas shows, in place, with the view's
    /// camera, lights and shading, each structure in the view's colour.
    pub fn from_atlas(view: &AtlasView, aspect: f32) -> Self {
        let mut frame = Self::from_scene(&view.scene, aspect);
        frame.light_centre = view.shown_bounds().center();
        frame.objects.extend(view.shown().map(|shown| {
            // A muscle's belly in its colour, its tendon pale.
            let structure = shown.structure;
            let tissue = |k: usize| {
                structure
                    .tissues
                    .get(k)
                    .copied()
                    .unwrap_or(structure.tissue)
            };
            let surface = FrameSurface::Parts {
                colors: [0, 1].map(|k| view.structure_tissue_color(shown.index, tissue(k))),
                fibres: [0, 1].map(|k| tissue(k) == Tissue::Muscle),
            };
            FrameObject {
                model_id: shown.model_id,
                model: Arc::clone(shown.model),
                transform: shown.transform,
                surface,
                highlight: if shown.selected {
                    Highlight::Selected
                } else if shown.hovered {
                    Highlight::Hovered
                } else {
                    Highlight::None
                },
                one_sided: false,
                // A see-through layer would darken what it shows.
                casts_shadows: shown.opacity >= 1.0,
                opacity: shown.opacity,
                fibres: shown.fibres,
            }
        }));
        frame
    }

    /// The colour behind everything, 8-bit sRGB: the backdrop's.
    pub fn clear_color(&self) -> [u8; 3] {
        self.room.backdrop_srgb()
    }
}
