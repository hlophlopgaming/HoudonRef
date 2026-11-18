//! The Studio scene: objects placed from shared models, the light and the camera.
//!
//! The scene is the single source of truth for the Studio. The interface
//! only reads it and changes it through the operations here.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Mat4, Quat, Vec3};

use crate::{Bounds, Model, OrbitCamera};

/// Identifies a model. Ids are unique for the whole run of the app, so the
/// renderer can cache GPU data by id across scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelId(u64);

impl ModelId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Identifies an object in a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u64);

/// Where an object is: position, rotation and uniform size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        }
    }
}

impl Transform {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            self.rotation,
            self.translation,
        )
    }
}

/// One placement of a model in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub id: ObjectId,
    pub model: ModelId,
    pub transform: Transform,
}

/// The light of the scene: one key light and ambient light, in linear colour.
///
/// This is the stage 0 light; stage 1a replaces it with a list of lights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Direction of the key light around the vertical axis, radians, 0 = front.
    pub key_azimuth: f32,
    /// Height of the key light above the horizon, radians.
    pub key_elevation: f32,
    pub key_color: Vec3,
    pub key_intensity: f32,
    pub sky_color: Vec3,
    pub ground_color: Vec3,
    pub ambient_intensity: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            // Classic three-quarter key from the upper left.
            key_azimuth: (-45f32).to_radians(),
            key_elevation: 45f32.to_radians(),
            key_color: Vec3::ONE,
            key_intensity: 1.0,
            sky_color: Vec3::new(0.85, 0.9, 1.0),
            ground_color: Vec3::new(0.35, 0.3, 0.25),
            ambient_intensity: 0.15,
        }
    }
}

impl Lighting {
    /// Unit vector pointing towards the key light.
    pub fn key_direction(&self) -> Vec3 {
        let (sin_az, cos_az) = self.key_azimuth.sin_cos();
        let (sin_el, cos_el) = self.key_elevation.sin_cos();
        Vec3::new(cos_el * sin_az, sin_el, cos_el * cos_az)
    }
}

pub struct Scene {
    models: BTreeMap<ModelId, Arc<Model>>,
    objects: Vec<Object>,
    next_object: u64,
    pub camera: OrbitCamera,
    pub lighting: Lighting,
    /// Background colour, sRGB.
    pub background: [u8; 3],
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    pub fn new() -> Self {
        Self {
            models: BTreeMap::new(),
            objects: Vec::new(),
            next_object: 1,
            camera: OrbitCamera::default(),
            lighting: Lighting::default(),
            background: [40, 40, 40],
        }
    }

    /// Adds a model to the scene's library. It is not visible until an
    /// object is placed from it.
    pub fn add_model(&mut self, model: Model) -> ModelId {
        let id = ModelId::next();
        self.models.insert(id, Arc::new(model));
        id
    }

    pub fn model(&self, id: ModelId) -> Option<&Arc<Model>> {
        self.models.get(&id)
    }

    /// Places a new object from a model already in the scene, at the origin.
    pub fn add_object(&mut self, model: ModelId) -> Option<ObjectId> {
        if !self.models.contains_key(&model) {
            return None;
        }
        let id = ObjectId(self.next_object);
        self.next_object += 1;
        self.objects.push(Object {
            id,
            model,
            transform: Transform::default(),
        });
        Some(id)
    }

    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Moves, turns or resizes an object. Returns `false` if there is no such object.
    pub fn set_transform(&mut self, id: ObjectId, transform: Transform) -> bool {
        match self.objects.iter_mut().find(|o| o.id == id) {
            Some(object) => {
                object.transform = transform;
                true
            }
            None => false,
        }
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Removes every object and model.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.models.clear();
    }

    /// Bounds of one object in scene space.
    pub fn object_bounds(&self, object: &Object) -> Bounds {
        let Some(model) = self.models.get(&object.model) else {
            return Bounds::EMPTY;
        };
        let b = model.bounds;
        if b.is_empty() {
            return b;
        }
        let m = object.transform.matrix();
        Bounds::from_points((0..8).map(|i| {
            let corner = Vec3::new(
                if i & 1 == 0 { b.min.x } else { b.max.x },
                if i & 2 == 0 { b.min.y } else { b.max.y },
                if i & 4 == 0 { b.min.z } else { b.max.z },
            );
            m.transform_point3(corner)
        }))
    }

    /// Bounds of all objects in scene space.
    pub fn bounds(&self) -> Bounds {
        self.objects
            .iter()
            .map(|o| self.object_bounds(o))
            .filter(|b| !b.is_empty())
            .fold(Bounds::EMPTY, |acc, b| acc.extended(b.min).extended(b.max))
    }

    /// Points the camera at the whole scene so it fits the view.
    pub fn frame_camera(&mut self, aspect: f32) {
        let bounds = self.bounds();
        self.camera.frame(&bounds, aspect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_FIGURE_HEIGHT;

    const MALE: &[u8] = include_bytes!("../../../assets/models/test/male.glb");

    fn figure() -> Model {
        let mut model = Model::from_gltf_slice("male", MALE).unwrap();
        model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
        model
    }

    #[test]
    fn a_scene_with_one_figure_has_the_figure_bounds() {
        let mut scene = Scene::new();
        let model = scene.add_model(figure());
        scene.add_object(model).expect("model is in the scene");
        assert_eq!(scene.object_count(), 1);
        let bounds = scene.bounds();
        assert!((bounds.size().y - DEFAULT_FIGURE_HEIGHT).abs() < 1e-4);
        assert!(bounds.min.y.abs() < 1e-4);
    }

    #[test]
    fn an_empty_scene_has_no_bounds_and_keeps_its_camera() {
        let mut scene = Scene::new();
        let camera = scene.camera;
        scene.frame_camera(1.5);
        assert!(scene.bounds().is_empty());
        assert_eq!(scene.camera, camera);
    }

    #[test]
    fn moving_an_object_moves_the_scene_bounds() {
        let mut scene = Scene::new();
        let model = scene.add_model(figure());
        let id = scene.add_object(model).unwrap();
        let before = scene.bounds().center();
        scene
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .unwrap()
            .transform
            .translation
            .x = 2.0;
        let after = scene.bounds().center();
        assert!((after.x - before.x - 2.0).abs() < 1e-4);
    }

    #[test]
    fn key_light_direction_follows_its_angles() {
        let mut lighting = Lighting {
            key_azimuth: 0.0,
            key_elevation: 0.0,
            ..Lighting::default()
        };
        assert!(
            (lighting.key_direction() - Vec3::Z).length() < 1e-6,
            "front"
        );
        lighting.key_elevation = std::f32::consts::FRAC_PI_2;
        assert!(
            (lighting.key_direction() - Vec3::Y).length() < 1e-6,
            "above"
        );
        lighting.key_elevation = 0.0;
        lighting.key_azimuth = std::f32::consts::FRAC_PI_2;
        assert!(
            (lighting.key_direction() - Vec3::X).length() < 1e-6,
            "right"
        );
    }

    #[test]
    fn objects_need_a_model_from_the_scene() {
        let mut other = Scene::new();
        let foreign = other.add_model(figure());
        let mut scene = Scene::new();
        assert_eq!(scene.add_object(foreign), None);
    }

    #[test]
    fn framing_puts_the_whole_figure_in_view() {
        let mut scene = Scene::new();
        let model = scene.add_model(figure());
        scene.add_object(model).unwrap();
        scene.frame_camera(1.5);
        let bounds = scene.bounds();
        assert_eq!(scene.camera.target, bounds.center());
        let half_angle = (bounds.radius() / scene.camera.distance).asin();
        assert!(half_angle <= scene.camera.vertical_fov() * 0.5);
    }
}
