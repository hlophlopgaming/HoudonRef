//! Camera and Room presets: named, portable as RON, and applied to any
//! Scene without touching anything else in it. (Light presets live in
//! `light_preset`, as they also carry mask images.)

use serde::{Deserialize, Serialize};

use crate::{Bounds, BuiltinRoom, OrbitCamera, Room, Scene};

const BUILT_IN_CAMERAS: &str = include_str!("../../../assets/presets/cameras.ron");

/// The longest preset name, bytes.
pub const MAX_PRESET_NAME_BYTES: usize = 80;

#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error("{0}")]
    Read(#[from] ron::error::SpannedError),
    #[error("{0}")]
    Write(#[from] ron::Error),
    #[error("invalid preset name")]
    Name,
    #[error("invalid values in preset")]
    Values,
}

/// A name the artist typed, trimmed; `None` if it is empty, too long or has
/// control characters.
fn checked_name(name: &str) -> Result<String, PresetError> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_PRESET_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(PresetError::Name);
    }
    Ok(name.to_owned())
}

/// What a Camera preset frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraTarget {
    /// Every object.
    #[default]
    Scene,
    /// The selected object, or every object when none is selected.
    Selected,
}

/// A viewpoint independent of what is framed: the same preset frames a
/// figure and a skull the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraPreset {
    name: String,
    /// Around the vertical axis, radians; 0 is the front.
    yaw: f32,
    /// Above the horizon, radians.
    pitch: f32,
    focal_length_mm: f32,
    /// A multiple of the distance at which the framed objects just fit.
    distance: f32,
}

impl CameraPreset {
    pub fn new(
        name: &str,
        yaw: f32,
        pitch: f32,
        focal_length_mm: f32,
        distance: f32,
    ) -> Result<Self, PresetError> {
        let preset = Self {
            name: checked_name(name)?,
            yaw,
            pitch,
            focal_length_mm,
            distance,
        };
        preset.check()?;
        Ok(preset)
    }

    /// The viewpoint of `camera`, relative to `bounds` seen at `aspect`.
    pub fn from_camera(
        name: &str,
        camera: &OrbitCamera,
        bounds: &Bounds,
        aspect: f32,
    ) -> Result<Self, PresetError> {
        let fit =
            fit_distance(camera.focal_length_mm, bounds, aspect).ok_or(PresetError::Values)?;
        Self::new(
            name,
            camera.yaw,
            camera.pitch,
            camera.focal_length_mm,
            camera.distance / fit,
        )
    }

    fn check(&self) -> Result<(), PresetError> {
        let finite = [self.yaw, self.pitch, self.focal_length_mm, self.distance]
            .iter()
            .all(|value| value.is_finite());
        let ok = finite
            && (OrbitCamera::MIN_PITCH..=OrbitCamera::MAX_PITCH).contains(&self.pitch)
            && (OrbitCamera::MIN_FOCAL_LENGTH_MM..=OrbitCamera::MAX_FOCAL_LENGTH_MM)
                .contains(&self.focal_length_mm)
            && (0.05..=100.0).contains(&self.distance);
        if ok { Ok(()) } else { Err(PresetError::Values) }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn focal_length_mm(&self) -> f32 {
        self.focal_length_mm
    }

    /// Moves `scene`'s camera to this viewpoint around `target`, for a view
    /// of `aspect`. Nothing else in the scene changes.
    pub fn apply(&self, scene: &mut Scene, target: CameraTarget, aspect: f32) {
        let bounds = scene.framing_bounds(target);
        self.aim(&mut scene.camera, &bounds, aspect);
    }

    /// Moves `camera` to this viewpoint around `bounds`, for a view of
    /// `aspect`.
    pub fn aim(&self, camera: &mut OrbitCamera, bounds: &crate::Bounds, aspect: f32) {
        if bounds.is_empty() {
            return;
        }
        camera.yaw = self.yaw;
        camera.pitch = self.pitch;
        camera.focal_length_mm = self.focal_length_mm;
        camera.frame(bounds, aspect);
        camera.distance = (camera.distance * self.distance)
            .clamp(OrbitCamera::MIN_DISTANCE, OrbitCamera::MAX_DISTANCE);
    }

    pub fn built_ins() -> Vec<Self> {
        let presets: Vec<Self> =
            ron::from_str(BUILT_IN_CAMERAS).expect("built-in camera presets must parse");
        assert!(presets.iter().all(|preset| preset.check().is_ok()));
        presets
    }

    pub fn to_ron(&self) -> Result<String, PresetError> {
        Ok(ron::ser::to_string_pretty(
            self,
            ron::ser::PrettyConfig::new(),
        )?)
    }

    pub fn from_ron(bytes: &[u8]) -> Result<Self, PresetError> {
        let preset: Self = ron::de::from_bytes(bytes)?;
        checked_name(&preset.name)?;
        preset.check()?;
        Ok(preset)
    }
}

/// The distance at which `bounds` just fit in a view of `aspect` through a
/// lens of `focal_length_mm`.
fn fit_distance(focal_length_mm: f32, bounds: &Bounds, aspect: f32) -> Option<f32> {
    if bounds.is_empty() {
        return None;
    }
    let mut camera = OrbitCamera {
        focal_length_mm,
        ..OrbitCamera::default()
    };
    camera.frame(bounds, aspect);
    (camera.distance > 0.0).then_some(camera.distance)
}

/// A Room under a name: the artist's own, or one of the built-in ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomPreset {
    name: String,
    room: Room,
}

impl RoomPreset {
    pub fn new(name: &str, room: Room) -> Result<Self, PresetError> {
        let preset = Self {
            name: checked_name(name)?,
            room,
        };
        preset.check()?;
        Ok(preset)
    }

    fn check(&self) -> Result<(), PresetError> {
        let room = &self.room;
        let colors_ok = [room.floor_color, room.backdrop_color]
            .iter()
            .all(|c| c.is_finite() && c.min_element() >= 0.0 && c.max_element() <= 10.0);
        if colors_ok && (0.0..=10.0).contains(&room.ambient_intensity) {
            Ok(())
        } else {
            Err(PresetError::Values)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn room(&self) -> &Room {
        &self.room
    }

    /// Replaces `scene`'s Room; nothing else changes.
    pub fn apply(&self, scene: &mut Scene) {
        scene.room = self.room;
    }

    /// The built-in rooms, named "grey", "white" and "dark".
    pub fn built_ins() -> Vec<Self> {
        BuiltinRoom::ALL
            .into_iter()
            .map(|builtin| {
                let name = match builtin {
                    BuiltinRoom::Grey => "grey",
                    BuiltinRoom::White => "white",
                    BuiltinRoom::Dark => "dark",
                };
                Self::new(name, Room::builtin(builtin)).expect("valid built-in room")
            })
            .collect()
    }

    pub fn to_ron(&self) -> Result<String, PresetError> {
        Ok(ron::ser::to_string_pretty(
            self,
            ron::ser::PrettyConfig::new(),
        )?)
    }

    pub fn from_ron(bytes: &[u8]) -> Result<Self, PresetError> {
        let preset: Self = ron::de::from_bytes(bytes)?;
        checked_name(&preset.name)?;
        preset.check()?;
        Ok(preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_FIGURE_HEIGHT;
    use crate::{LightKind, Model, Transform};
    use glam::Vec3;

    const MALE: &[u8] = include_bytes!("../../../assets/models/test/male.glb");
    const FEMALE: &[u8] = include_bytes!("../../../assets/models/test/female.glb");

    fn scene_with(model: Model) -> Scene {
        let mut scene = Scene::new();
        let id = scene.add_model(model);
        scene.add_object(id).unwrap();
        scene
    }

    fn figure(bytes: &[u8]) -> Scene {
        let mut model = Model::from_gltf_slice("figure", bytes).unwrap();
        model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
        scene_with(model)
    }

    fn small_box() -> Scene {
        let positions = (0..8)
            .map(|i| {
                Vec3::new(
                    if i & 1 == 0 { -0.05 } else { 0.05 },
                    if i & 2 == 0 { 0.0 } else { 0.1 },
                    if i & 4 == 0 { -0.05 } else { 0.05 },
                )
            })
            .collect();
        let indices = vec![0, 1, 2, 1, 3, 2, 4, 6, 5, 5, 6, 7];
        scene_with(Model::from_mesh("box", positions, indices))
    }

    #[test]
    fn every_built_in_preset_parses_with_its_own_name() {
        let cameras = CameraPreset::built_ins();
        let names: Vec<_> = cameras.iter().map(CameraPreset::name).collect();
        assert_eq!(
            names,
            [
                "front",
                "three-quarter",
                "profile",
                "from-above",
                "from-below"
            ]
        );
        let rooms = RoomPreset::built_ins();
        assert_eq!(rooms.len(), 3);
        assert_eq!(rooms[0].room(), &Room::builtin(BuiltinRoom::Grey));
    }

    #[test]
    fn user_presets_round_trip_through_ron_unchanged() {
        let camera = CameraPreset::new("Low three-quarter", 0.9, -0.2, 35.0, 1.4).unwrap();
        let back = CameraPreset::from_ron(camera.to_ron().unwrap().as_bytes()).unwrap();
        assert_eq!(back, camera);

        let mut room = Room::builtin(BuiltinRoom::White);
        room.floor_color = Vec3::new(0.5, 0.2, 0.1);
        room.visible = false;
        let preset = RoomPreset::new("Warm floor", room).unwrap();
        let back = RoomPreset::from_ron(preset.to_ron().unwrap().as_bytes()).unwrap();
        assert_eq!(back, preset);
    }

    #[test]
    fn a_camera_preset_frames_any_subject_the_same_way() {
        let preset = &CameraPreset::built_ins()[1];
        let ratios: Vec<f32> = [figure(MALE), figure(FEMALE), small_box()]
            .into_iter()
            .map(|mut scene| {
                preset.apply(&mut scene, CameraTarget::Scene, 1.5);
                let bounds = scene.bounds();
                assert!((scene.camera.target - bounds.center()).length() < 1e-4);
                assert_eq!(scene.camera.focal_length_mm, 85.0);
                scene.camera.distance / bounds.radius()
            })
            .collect();
        assert!(
            ratios.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-3 * w[0]),
            "same distance relative to the subject: {ratios:?}"
        );
    }

    #[test]
    fn targeting_the_selection_frames_only_the_selected_object() {
        let mut scene = figure(MALE);
        let id = scene.objects()[0].model;
        let other = scene
            .add_object_with(
                id,
                Transform {
                    translation: Vec3::new(5.0, 0.0, 0.0),
                    ..Transform::default()
                },
            )
            .unwrap();
        scene.select(Some(other));
        let selected = scene.object_bounds(scene.object(other).unwrap());
        CameraPreset::built_ins()[0].apply(&mut scene, CameraTarget::Selected, 1.0);
        assert!((scene.camera.target - selected.center()).length() < 1e-4);

        CameraPreset::built_ins()[0].apply(&mut scene, CameraTarget::Scene, 1.0);
        assert!((scene.camera.target - scene.bounds().center()).length() < 1e-4);
    }

    #[test]
    fn a_preset_saved_from_the_camera_gives_that_camera_back() {
        let mut scene = figure(MALE);
        scene.camera.yaw = -0.6;
        scene.camera.pitch = 0.3;
        scene.camera.focal_length_mm = 70.0;
        scene.frame_camera(1.25);
        scene.camera.distance *= 1.7;
        let camera = scene.camera;
        let preset = CameraPreset::from_camera("Mine", &camera, &scene.bounds(), 1.25).unwrap();
        scene.camera = OrbitCamera::default();
        preset.apply(&mut scene, CameraTarget::Scene, 1.25);
        assert!((scene.camera.distance - camera.distance).abs() < 1e-4);
        assert_eq!(
            (
                scene.camera.yaw,
                scene.camera.pitch,
                scene.camera.focal_length_mm
            ),
            (camera.yaw, camera.pitch, camera.focal_length_mm)
        );
    }

    #[test]
    fn presets_change_only_their_own_part_of_the_scene() {
        let mut scene = figure(MALE);
        scene.add_light(LightKind::Spot).unwrap();
        let lights = scene.lights().to_vec();
        let objects = scene.objects().to_vec();
        let camera = scene.camera;

        RoomPreset::built_ins()[2].apply(&mut scene);
        assert_eq!(scene.room, Room::builtin(BuiltinRoom::Dark));
        assert_eq!(scene.camera, camera);
        assert_eq!(scene.lights(), lights);

        let room = scene.room;
        CameraPreset::built_ins()[2].apply(&mut scene, CameraTarget::Scene, 1.0);
        assert_eq!(scene.room, room);
        assert_eq!(scene.lights(), lights);
        assert_eq!(scene.objects(), objects);
    }

    #[test]
    fn invalid_presets_are_rejected() {
        assert!(matches!(
            CameraPreset::new("  ", 0.0, 0.0, 50.0, 1.0),
            Err(PresetError::Name)
        ));
        assert!(matches!(
            CameraPreset::new("Wide", 0.0, 0.0, 2.0, 1.0),
            Err(PresetError::Values)
        ));
        assert!(
            CameraPreset::from_ron(
                br#"(name: "x", yaw: 0.0, pitch: 9.0, focal_length_mm: 50.0, distance: 1.0)"#
            )
            .is_err()
        );
        let mut room = Room::builtin(BuiltinRoom::Grey);
        room.ambient_intensity = f32::NAN;
        assert!(matches!(
            RoomPreset::new("Broken", room),
            Err(PresetError::Values)
        ));
    }
}
