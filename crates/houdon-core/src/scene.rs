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

/// Gap left between a new object and the objects already in the scene, metres.
const PLACEMENT_GAP: f32 = 0.1;

pub struct Scene {
    models: BTreeMap<ModelId, Arc<Model>>,
    objects: Vec<Object>,
    next_object: u64,
    selected: Option<ObjectId>,
    pub camera: OrbitCamera,
    pub lighting: Lighting,
    /// Background colour, sRGB.
    pub background: [u8; 3],
}

/// Distance along a ray to where it enters a box, or `None` if it misses.
fn ray_box_distance(origin: Vec3, direction: Vec3, bounds: &Bounds) -> Option<f32> {
    if bounds.is_empty() {
        return None;
    }
    let inverse = direction.recip();
    let t1 = (bounds.min - origin) * inverse;
    let t2 = (bounds.max - origin) * inverse;
    let near = t1.min(t2).max_element();
    let far = t1.max(t2).min_element();
    (far >= near.max(0.0)).then_some(near.max(0.0))
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
            selected: None,
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

    /// Places a new object from a model already in the scene. The first
    /// object stands at the origin; later ones stand on the ground beside
    /// the others, so nothing overlaps.
    pub fn add_object(&mut self, model: ModelId) -> Option<ObjectId> {
        let model_bounds = self.models.get(&model)?.bounds;
        let scene_bounds = self.bounds();
        let mut transform = Transform::default();
        if !scene_bounds.is_empty() && !model_bounds.is_empty() {
            transform.translation = Vec3::new(
                scene_bounds.max.x + PLACEMENT_GAP - model_bounds.min.x,
                -model_bounds.min.y,
                scene_bounds.center().z - model_bounds.center().z,
            );
        }
        Some(self.push_object(model, transform))
    }

    fn push_object(&mut self, model: ModelId, transform: Transform) -> ObjectId {
        let id = ObjectId(self.next_object);
        self.next_object += 1;
        self.objects.push(Object {
            id,
            model,
            transform,
        });
        id
    }

    /// Places a copy of an object, with the same turn and size, beside the
    /// others and selects it.
    pub fn duplicate_object(&mut self, id: ObjectId) -> Option<ObjectId> {
        let original = self.object(id)?.clone();
        let copy = self.push_object(original.model, original.transform);
        self.place_beside_others(copy);
        self.selected = Some(copy);
        Some(copy)
    }

    /// Moves an object along X so it stands just right of every other object.
    fn place_beside_others(&mut self, id: ObjectId) {
        let others = self
            .objects
            .iter()
            .filter(|o| o.id != id)
            .map(|o| self.object_bounds(o))
            .fold(Bounds::EMPTY, |acc, b| acc.extended(b.min).extended(b.max));
        let Some(object) = self.object(id) else {
            return;
        };
        let own = self.object_bounds(object);
        if others.is_empty() || own.is_empty() {
            return;
        }
        let shift = others.max.x + PLACEMENT_GAP - own.min.x;
        if let Some(object) = self.objects.iter_mut().find(|o| o.id == id) {
            object.transform.translation.x += shift;
        }
    }

    /// Deletes an object. A model no other object uses is released too.
    /// Returns `false` if there is no such object.
    pub fn remove_object(&mut self, id: ObjectId) -> bool {
        let Some(index) = self.objects.iter().position(|o| o.id == id) else {
            return false;
        };
        let removed = self.objects.remove(index);
        if !self.objects.iter().any(|o| o.model == removed.model) {
            self.models.remove(&removed.model);
        }
        if self.selected == Some(id) {
            self.selected = None;
        }
        true
    }

    /// Selects an object, or clears the selection with `None`. Selecting an
    /// object that is not in the scene clears the selection.
    pub fn select(&mut self, id: Option<ObjectId>) {
        self.selected = id.filter(|id| self.object(*id).is_some());
    }

    pub fn selected(&self) -> Option<ObjectId> {
        self.selected
    }

    pub fn model_count(&self) -> usize {
        self.models.len()
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
        self.selected = None;
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

    /// Points the camera at the selected object, or at the whole scene when
    /// nothing is selected.
    pub fn frame_selected(&mut self, aspect: f32) {
        let bounds = match self.selected.and_then(|id| self.object(id)) {
            Some(object) => self.object_bounds(object),
            None => self.bounds(),
        };
        self.camera.frame(&bounds, aspect);
    }

    /// The nearest object hit by a ray from `origin` along `direction`.
    pub fn pick(&self, origin: Vec3, direction: Vec3) -> Option<ObjectId> {
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        let mut nearest: Option<(f32, ObjectId)> = None;
        for object in &self.objects {
            let Some(model) = self.models.get(&object.model) else {
                continue;
            };
            let world = object.transform.matrix();
            if ray_box_distance(origin, direction, &self.object_bounds(object)).is_none() {
                continue;
            }
            // Test in model space: transform the ray instead of every vertex.
            let to_model = world.inverse();
            let local_origin = to_model.transform_point3(origin);
            let local_direction = to_model.transform_vector3(direction);
            let Some(t) = model.ray_hit(local_origin, local_direction) else {
                continue;
            };
            // Distance along the world ray, comparable between objects.
            let distance =
                (world.transform_point3(local_origin + local_direction * t) - origin).length();
            if nearest.is_none_or(|(d, _)| distance < d) {
                nearest = Some((distance, object.id));
            }
        }
        nearest.map(|(_, id)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_FIGURE_HEIGHT;
    use glam::{Quat, Vec3};

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

    fn two_figure_scene() -> (Scene, ModelId, ObjectId, ObjectId) {
        let mut scene = Scene::new();
        let model = scene.add_model(figure());
        let first = scene.add_object(model).unwrap();
        let second = scene.add_object(model).unwrap();
        (scene, model, first, second)
    }

    #[test]
    fn a_new_object_stands_beside_the_others() {
        let (scene, _, first, second) = two_figure_scene();
        let a = scene.object_bounds(scene.object(first).unwrap());
        let b = scene.object_bounds(scene.object(second).unwrap());
        assert!(b.min.x >= a.max.x, "objects overlap: {a:?} {b:?}");
        assert!(b.min.y.abs() < 1e-4, "the new object stands on the ground");
        let all = scene.bounds();
        assert!(all.min.x <= a.min.x && all.max.x >= b.max.x);
    }

    #[test]
    fn duplicating_an_object_shares_its_model() {
        let (mut scene, model, first, _) = two_figure_scene();
        let copy = scene.duplicate_object(first).unwrap();
        assert_ne!(copy, first);
        assert_eq!(scene.object(copy).unwrap().model, model);
        assert_eq!(scene.model_count(), 1);
        assert_eq!(scene.object_count(), 3);
        assert_eq!(scene.selected(), Some(copy), "the copy is selected");
    }

    #[test]
    fn deleting_the_last_object_of_a_model_releases_the_model() {
        let (mut scene, model, first, second) = two_figure_scene();
        assert!(scene.remove_object(first));
        assert!(
            scene.model(model).is_some(),
            "still used by the second object"
        );
        assert!(scene.remove_object(second));
        assert!(scene.model(model).is_none());
        assert!(!scene.remove_object(second), "already gone");
    }

    #[test]
    fn deleting_the_selected_object_clears_the_selection() {
        let (mut scene, _, first, _) = two_figure_scene();
        scene.select(Some(first));
        scene.remove_object(first);
        assert_eq!(scene.selected(), None);
    }

    #[test]
    fn only_existing_objects_can_be_selected() {
        let (mut scene, _, first, _) = two_figure_scene();
        scene.select(Some(first));
        assert_eq!(scene.selected(), Some(first));
        scene.remove_object(first);
        scene.select(Some(first));
        assert_eq!(scene.selected(), None);
    }

    #[test]
    fn turning_and_resizing_change_the_bounds() {
        let (mut scene, _, first, _) = two_figure_scene();
        let before = scene.object_bounds(scene.object(first).unwrap()).size();
        let mut transform = scene.object(first).unwrap().transform;
        transform.scale = 2.0;
        scene.set_transform(first, transform);
        let doubled = scene.object_bounds(scene.object(first).unwrap()).size();
        assert!((doubled.y - before.y * 2.0).abs() < 1e-3);
        transform.scale = 1.0;
        transform.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        scene.set_transform(first, transform);
        let lying = scene.object_bounds(scene.object(first).unwrap()).size();
        assert!(
            (lying.x - before.y).abs() < 1e-3,
            "a figure turned on its side is as wide as it was tall"
        );
    }

    #[test]
    fn frame_selected_targets_only_the_selected_object() {
        let (mut scene, _, _, second) = two_figure_scene();
        scene.select(Some(second));
        scene.frame_selected(1.5);
        let target = scene.object_bounds(scene.object(second).unwrap()).center();
        assert!((scene.camera.target - target).length() < 1e-4);
        scene.frame_camera(1.5);
        assert!((scene.camera.target - scene.bounds().center()).length() < 1e-4);
    }

    #[test]
    fn frame_selected_without_a_selection_frames_the_scene() {
        let (mut scene, _, _, _) = two_figure_scene();
        scene.frame_selected(1.5);
        assert!((scene.camera.target - scene.bounds().center()).length() < 1e-4);
    }

    #[test]
    fn picking_finds_the_object_under_a_ray() {
        let (mut scene, _, first, second) = two_figure_scene();
        scene.frame_camera(1.5);
        let centre_of = |id| scene.object_bounds(scene.object(id).unwrap()).center();
        for id in [first, second] {
            let eye = scene.camera.eye();
            let direction = (centre_of(id) - eye).normalize();
            assert_eq!(scene.pick(eye, direction), Some(id));
        }
        let up = Vec3::Y;
        assert_eq!(scene.pick(scene.camera.eye(), up), None, "nothing above");
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
