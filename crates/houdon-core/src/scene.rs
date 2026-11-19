//! The Studio scene: objects placed from shared models, the light and the camera.
//!
//! The scene is the single source of truth for the Studio. The interface
//! only reads it and changes it through the operations here.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::light::{Light, LightId, LightKind};
use crate::model::DEFAULT_FIGURE_HEIGHT;
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
    /// Rotation as angles in radians around Y, then X, then Z, the order
    /// an artist turns an object on a stand. Kept as angles, not a
    /// quaternion, so the values the artist set read back unchanged.
    pub rotation: Vec3,
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: 1.0,
        }
    }
}

impl Transform {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            Quat::from_euler(
                EulerRot::YXZ,
                self.rotation.y,
                self.rotation.x,
                self.rotation.z,
            ),
            self.translation,
        )
    }
}

/// One placement of a model in the scene.
///
/// An object stands on its model's bottom centre: the transform moves,
/// turns and sizes the model around that point.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub id: ObjectId,
    pub model: ModelId,
    pub transform: Transform,
    /// Shown as a Figure: at the scene's figure height instead of the
    /// model's own size.
    pub figure: bool,
}

/// A model in the scene and how its file units map to metres.
struct ModelEntry {
    model: Arc<Model>,
    /// Metres per file unit, set when the artist gives a real size.
    unit_scale: f32,
}

/// Sizes outside this range, in metres, are taken for wrong file units.
pub const PLAUSIBLE_SIZE: std::ops::RangeInclusive<f32> = 0.01..=100.0;

/// Brightness of an added light relative to the key light.
const NEW_LIGHT_FILL: f32 = 0.5;

/// The most lights a scene can hold; the renderer's light array has this size.
pub const MAX_LIGHTS: usize = 16;

/// Light that comes from everywhere: a sky colour from above and a ground
/// colour from below, in linear RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ambient {
    pub sky_color: Vec3,
    pub ground_color: Vec3,
    pub intensity: f32,
}

impl Default for Ambient {
    fn default() -> Self {
        Self {
            sky_color: Vec3::new(0.85, 0.9, 1.0),
            ground_color: Vec3::new(0.35, 0.3, 0.25),
            intensity: 0.15,
        }
    }
}

/// Gap left between a new object and the objects already in the scene, metres.
const PLACEMENT_GAP: f32 = 0.1;

pub struct Scene {
    models: BTreeMap<ModelId, ModelEntry>,
    objects: Vec<Object>,
    figure_height: f32,
    next_object: u64,
    selected: Option<ObjectId>,
    lights: Vec<Light>,
    next_light: u64,
    pub camera: OrbitCamera,
    pub ambient: Ambient,
    /// Background colour, sRGB.
    pub background: [u8; 3],
}

/// An angle in radians brought into -π..π.
fn wrap_angle(radians: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (radians + PI).rem_euclid(TAU) - PI
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
            figure_height: DEFAULT_FIGURE_HEIGHT,
            selected: None,
            lights: vec![Light {
                id: LightId(1),
                ..Light::new(LightKind::Directional)
            }],
            next_light: 2,
            camera: OrbitCamera::default(),
            ambient: Ambient::default(),
            background: [40, 40, 40],
        }
    }

    /// Adds a model to the scene's library. It is not visible until an
    /// object is placed from it.
    pub fn add_model(&mut self, model: Model) -> ModelId {
        let id = ModelId::next();
        self.models.insert(
            id,
            ModelEntry {
                model: Arc::new(model),
                unit_scale: 1.0,
            },
        );
        id
    }

    pub fn model(&self, id: ModelId) -> Option<&Arc<Model>> {
        self.models.get(&id).map(|entry| &entry.model)
    }

    /// Height every Figure is shown at, metres.
    pub fn figure_height(&self) -> f32 {
        self.figure_height
    }

    pub fn set_figure_height(&mut self, metres: f32) {
        if metres > 0.0 {
            self.figure_height = metres;
        }
    }

    /// Shows an object as a Figure or at its model's own size. Returns
    /// `false` if there is no such object.
    pub fn set_figure(&mut self, id: ObjectId, figure: bool) -> bool {
        match self.objects.iter_mut().find(|o| o.id == id) {
            Some(object) => {
                object.figure = figure;
                true
            }
            None => false,
        }
    }

    /// Whether the model's size in file units looks wrong (below a
    /// centimetre or above 100 metres), so the artist should give its real
    /// size. Models shown as Figures never ask.
    pub fn needs_real_size(&self, id: ModelId) -> bool {
        let shown_as_figure = self.objects.iter().any(|o| o.model == id && o.figure);
        !shown_as_figure
            && self
                .real_size(id)
                .is_some_and(|size| !PLAUSIBLE_SIZE.contains(&size))
    }

    /// The model's largest side in metres, as shown when not a Figure.
    pub fn real_size(&self, id: ModelId) -> Option<f32> {
        let entry = self.models.get(&id)?;
        Some(entry.model.bounds.size().max_element() * entry.unit_scale)
    }

    /// Gives the model's real size as its largest side in metres; every
    /// object of the model that is not a Figure is resized.
    pub fn set_real_size(&mut self, id: ModelId, metres: f32) {
        if let Some(entry) = self.models.get_mut(&id) {
            let file_size = entry.model.bounds.size().max_element();
            if file_size > 0.0 && metres > 0.0 {
                entry.unit_scale = metres / file_size;
            }
        }
    }

    /// Model space to scene space for an object: the model is stood on its
    /// bottom centre, sized (as a Figure or by its real size), then the
    /// object's transform is applied.
    pub fn object_matrix(&self, object: &Object) -> Mat4 {
        let Some(entry) = self.models.get(&object.model) else {
            return object.transform.matrix();
        };
        let b = entry.model.bounds;
        if b.is_empty() {
            return object.transform.matrix();
        }
        let size = b.size();
        let scale = if object.figure && size.y > 0.0 {
            self.figure_height / size.y
        } else {
            entry.unit_scale
        };
        let to_bottom_centre =
            Mat4::from_translation(-Vec3::new(b.center().x, b.min.y, b.center().z));
        object.transform.matrix() * Mat4::from_scale(Vec3::splat(scale)) * to_bottom_centre
    }

    /// Places a new object from a model already in the scene. The first
    /// object stands at the origin; later ones stand on the ground beside
    /// the others, so nothing overlaps.
    pub fn add_object(&mut self, model: ModelId) -> Option<ObjectId> {
        let figure = self.models.get(&model)?.model.looks_like_figure();
        let scene_bounds = self.bounds();
        let mut transform = Transform::default();
        if !scene_bounds.is_empty() {
            transform.translation.z = scene_bounds.center().z;
        }
        let id = self.push_object(model, transform, figure);
        self.place_beside_others(id);
        Some(id)
    }

    fn push_object(&mut self, model: ModelId, transform: Transform, figure: bool) -> ObjectId {
        let id = ObjectId(self.next_object);
        self.next_object += 1;
        self.objects.push(Object {
            id,
            model,
            transform,
            figure,
        });
        id
    }

    /// Places a copy of an object, with the same rotation and size, beside the
    /// others and selects it.
    pub fn duplicate_object(&mut self, id: ObjectId) -> Option<ObjectId> {
        let original = self.object(id)?.clone();
        let copy = self.push_object(original.model, original.transform, original.figure);
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
        let Some(entry) = self.models.get(&object.model) else {
            return Bounds::EMPTY;
        };
        let b = entry.model.bounds;
        if b.is_empty() {
            return b;
        }
        let m = self.object_matrix(object);
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

    pub fn lights(&self) -> &[Light] {
        &self.lights
    }

    pub fn light(&self, id: LightId) -> Option<&Light> {
        self.lights.iter().find(|l| l.id == id)
    }

    /// Adds a light of `kind` in the default key position. Returns `None`
    /// when the scene already has [`MAX_LIGHTS`].
    pub fn add_light(&mut self, kind: LightKind) -> Option<LightId> {
        if self.lights.len() >= MAX_LIGHTS {
            return None;
        }
        let id = LightId(self.next_light);
        self.next_light += 1;
        // Each new light is a dimmer fill, a third of the way round from
        // the one before, so it does not sit on top of the others.
        let mut light = Light {
            id,
            ..Light::new(kind)
        };
        let step = 2.0 * std::f32::consts::PI / 3.0;
        light.azimuth = wrap_angle(light.azimuth + step * self.lights.len() as f32);
        light.elevation = 25f32.to_radians();
        light.intensity *= NEW_LIGHT_FILL;
        self.lights.push(light);
        Some(id)
    }

    /// Replaces the light with the same id. Returns `false` if there is none.
    pub fn set_light(&mut self, light: Light) -> bool {
        match self.lights.iter_mut().find(|l| l.id == light.id) {
            Some(slot) => {
                *slot = light;
                true
            }
            None => false,
        }
    }

    /// Returns `false` if there is no such light.
    pub fn remove_light(&mut self, id: LightId) -> bool {
        let before = self.lights.len();
        self.lights.retain(|l| l.id != id);
        self.lights.len() != before
    }

    /// The point lights are placed around and spot lights aim at: the
    /// centre of the objects, or the origin when there are none.
    pub fn light_centre(&self) -> Vec3 {
        let bounds = self.bounds();
        if bounds.is_empty() {
            Vec3::ZERO
        } else {
            bounds.center()
        }
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
            let Some(entry) = self.models.get(&object.model) else {
                continue;
            };
            let model = &entry.model;
            let world = self.object_matrix(object);
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
    use glam::Vec3;

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
        transform.rotation = Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2);
        scene.set_transform(first, transform);
        let lying = scene.object_bounds(scene.object(first).unwrap()).size();
        assert!(
            (lying.x - before.y).abs() < 1e-3,
            "a figure turned on its side is as wide as it was tall"
        );
    }

    #[test]
    fn rotation_angles_read_back_as_set() {
        let (mut scene, _, first, _) = two_figure_scene();
        let mut transform = scene.object(first).unwrap().transform;
        // Past 90 degrees around X, where Euler angles from a quaternion would flip.
        transform.rotation =
            Vec3::new(120f32.to_radians(), 30f32.to_radians(), -45f32.to_radians());
        scene.set_transform(first, transform);
        assert_eq!(
            scene.object(first).unwrap().transform.rotation,
            transform.rotation
        );
    }

    #[test]
    fn picking_prefers_the_nearer_of_two_overlapping_objects() {
        let (mut scene, _, first, second) = two_figure_scene();
        // Put the second figure straight behind the first, as seen from the front.
        let mut behind = scene.object(first).unwrap().transform;
        behind.translation.z -= 1.0;
        scene.set_transform(second, behind);
        let centre = scene.object_bounds(scene.object(first).unwrap()).center();
        let eye = centre + Vec3::new(0.0, 0.0, 5.0);
        assert_eq!(scene.pick(eye, -Vec3::Z), Some(first));
        let eye_behind = centre - Vec3::new(0.0, 0.0, 6.0);
        assert_eq!(scene.pick(eye_behind, Vec3::Z), Some(second));
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

    /// An axis-aligned box model, `size` metres per side in file units.
    fn cube(size: f32) -> Model {
        let h = size / 2.0;
        let corners: Vec<Vec3> = (0..8)
            .map(|i| {
                Vec3::new(
                    if i & 1 == 0 { -h } else { h },
                    if i & 2 == 0 { -h } else { h },
                    if i & 4 == 0 { -h } else { h },
                )
            })
            .collect();
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let indices = quads
            .iter()
            .flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]])
            .collect();
        Model::from_mesh("cube", corners, indices)
    }

    fn raw_figure(bytes: &[u8]) -> Model {
        Model::from_gltf_slice("figure", bytes).unwrap()
    }

    const FEMALE: &[u8] = include_bytes!("../../../assets/models/test/female.glb");

    #[test]
    fn test_figures_are_shown_at_figure_height() {
        for bytes in [MALE, FEMALE] {
            let mut scene = Scene::new();
            let model = scene.add_model(raw_figure(bytes));
            let id = scene.add_object(model).unwrap();
            assert!(scene.object(id).unwrap().figure, "detected as a figure");
            let b = scene.object_bounds(scene.object(id).unwrap());
            assert!(
                (b.size().y - DEFAULT_FIGURE_HEIGHT).abs() < 1e-3,
                "{:?}",
                b.size()
            );
            assert!(b.min.y.abs() < 1e-4, "stands on the ground");
        }
    }

    #[test]
    fn figure_height_applies_to_every_figure() {
        let mut scene = Scene::new();
        let model = scene.add_model(raw_figure(MALE));
        let id = scene.add_object(model).unwrap();
        scene.set_figure_height(1.2);
        let b = scene.object_bounds(scene.object(id).unwrap());
        assert!((b.size().y - 1.2).abs() < 1e-3);
    }

    #[test]
    fn a_squat_model_is_not_a_figure_and_keeps_its_size() {
        let mut scene = Scene::new();
        let model = scene.add_model(cube(0.3));
        let id = scene.add_object(model).unwrap();
        assert!(!scene.object(id).unwrap().figure);
        let b = scene.object_bounds(scene.object(id).unwrap());
        assert!((b.size().y - 0.3).abs() < 1e-4);
        assert!(b.min.y.abs() < 1e-4, "stands on the ground");
    }

    #[test]
    fn switching_the_figure_setting_off_restores_the_file_size() {
        let mut scene = Scene::new();
        let file_height = raw_figure(MALE).bounds.size().y;
        let model = scene.add_model(raw_figure(MALE));
        let id = scene.add_object(model).unwrap();
        assert!(scene.set_figure(id, false));
        let b = scene.object_bounds(scene.object(id).unwrap());
        assert!((b.size().y - file_height).abs() < 1e-3);
        scene.set_figure(id, true);
        let b = scene.object_bounds(scene.object(id).unwrap());
        assert!((b.size().y - DEFAULT_FIGURE_HEIGHT).abs() < 1e-3);
    }

    #[test]
    fn a_model_asks_for_its_real_size_outside_a_centimetre_to_100_metres() {
        let mut scene = Scene::new();
        let tiny = scene.add_model(cube(0.005));
        let fine = scene.add_model(cube(0.5));
        let huge = scene.add_model(cube(200.0));
        assert!(scene.needs_real_size(tiny));
        assert!(!scene.needs_real_size(fine));
        assert!(scene.needs_real_size(huge));
    }

    #[test]
    fn the_real_size_limits_themselves_are_plausible() {
        let mut scene = Scene::new();
        for (size, asks) in [(0.0099, true), (0.01, false), (100.0, false), (100.5, true)] {
            let model = scene.add_model(cube(size));
            assert_eq!(scene.needs_real_size(model), asks, "{size} m");
        }
    }

    #[test]
    fn setting_a_real_size_rescales_every_object_of_the_model() {
        let mut scene = Scene::new();
        let model = scene.add_model(cube(200.0));
        let a = scene.add_object(model).unwrap();
        let b = scene.add_object(model).unwrap();
        scene.set_real_size(model, 0.3);
        assert!(!scene.needs_real_size(model));
        for id in [a, b] {
            let size = scene.object_bounds(scene.object(id).unwrap()).size();
            assert!((size.max_element() - 0.3).abs() < 1e-4, "{size:?}");
        }
        assert!((scene.real_size(model).unwrap() - 0.3).abs() < 1e-4);
    }

    #[test]
    fn figures_do_not_ask_for_a_real_size() {
        let mut scene = Scene::new();
        // A 2 cm tall figure-shaped file: it becomes a figure, so no prompt.
        let mut tiny_figure = raw_figure(MALE);
        tiny_figure.normalize_to_height(0.02);
        let model = scene.add_model(tiny_figure);
        scene.add_object(model).unwrap();
        assert!(!scene.needs_real_size(model));
    }

    #[test]
    fn a_new_scene_has_one_key_light() {
        let scene = Scene::new();
        let lights = scene.lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].kind, crate::LightKind::Directional);
        assert!(lights[0].enabled);
    }

    #[test]
    fn a_scene_holds_at_most_16_lights() {
        let mut scene = Scene::new();
        let added = (0..20)
            .filter_map(|_| scene.add_light(crate::LightKind::Point))
            .count();
        assert_eq!(added, MAX_LIGHTS - 1);
        assert_eq!(scene.lights().len(), MAX_LIGHTS);
        assert_eq!(scene.add_light(crate::LightKind::Spot), None);
    }

    #[test]
    fn a_new_light_does_not_sit_on_top_of_the_others() {
        let mut scene = Scene::new();
        for _ in 0..3 {
            scene.add_light(crate::LightKind::Point).unwrap();
        }
        let lights = scene.lights();
        for (i, a) in lights.iter().enumerate() {
            for b in &lights[i + 1..] {
                assert!(
                    a.direction().dot(b.direction()) < 0.95,
                    "lights {:?} and {:?} overlap",
                    a.id,
                    b.id
                );
            }
        }
        assert!(
            lights
                .iter()
                .all(|l| l.azimuth.abs() <= std::f32::consts::PI + 1e-6),
            "angles stay within ±180°"
        );
        let key = lights[0].intensity_at_centre();
        assert!(
            lights[1..].iter().all(|l| l.intensity_at_centre() < key),
            "added lights are fills, dimmer than the key"
        );
    }

    #[test]
    fn lights_can_be_changed_and_removed() {
        let mut scene = Scene::new();
        let id = scene.add_light(crate::LightKind::Spot).unwrap();
        let mut light = scene.light(id).unwrap().clone();
        light.intensity = 42.0;
        assert!(scene.set_light(light));
        assert_eq!(scene.light(id).unwrap().intensity, 42.0);
        assert!(scene.remove_light(id));
        assert!(scene.light(id).is_none());
        assert!(!scene.remove_light(id));
    }

    #[test]
    fn lights_are_placed_around_the_scene_centre() {
        let (scene, _, _, _) = two_figure_scene();
        assert!((scene.light_centre() - scene.bounds().center()).length() < 1e-5);
        assert_eq!(Scene::new().light_centre(), Vec3::ZERO);
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
