use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    Light, MAX_LIGHTS, Model, ModelId, Object, ObjectId, OrbitCamera, Room, Scene, Shading,
    Transform,
};
use crate::ValueMode;

const VERSION: u32 = 1;

/// Where a saved model comes from. The platform decides how to resolve paths
/// and bundled asset names.
#[derive(Debug, Serialize, Deserialize)]
pub enum ModelSource {
    Builtin(String),
    Path(PathBuf),
    Embedded {
        builtin: Option<String>,
        #[serde(with = "base64_bytes")]
        bytes: std::sync::Arc<[u8]>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SceneFileError {
    #[error("{0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("{0}")]
    Write(#[from] ron::Error),
    #[error("unsupported scene file version {0}")]
    Version(u32),
    #[error("invalid scene file field: {0}")]
    Invalid(&'static str),
    #[error("model {0} has no file source")]
    NoSource(String),
}

pub struct LoadedScene<T> {
    pub scene: Scene,
    /// Resolved sources keyed by the new runtime model IDs.
    pub models: HashMap<ModelId, T>,
    /// A model name and load error for each skipped model.
    pub warnings: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize)]
struct SceneFile {
    version: u32,
    models: Vec<SavedModel>,
    objects: Vec<SavedObject>,
    selected: Option<ObjectId>,
    figure_height: f32,
    camera: OrbitCamera,
    room: Room,
    lights: Vec<Light>,
    shading: Shading,
}

#[derive(Serialize, Deserialize)]
struct SavedModel {
    name: String,
    source: ModelSource,
    unit_scale: f32,
}

#[derive(Serialize, Deserialize)]
struct SavedObject {
    id: ObjectId,
    model: usize,
    transform: Transform,
    figure: bool,
}

impl Scene {
    pub fn to_ron(
        &self,
        source: impl Fn(ModelId) -> Option<ModelSource>,
    ) -> Result<String, SceneFileError> {
        let mut model_indices = HashMap::new();
        let mut models = Vec::new();
        for (&id, entry) in &self.models {
            model_indices.insert(id, models.len());
            models.push(SavedModel {
                name: entry.model.name.clone(),
                source: source(id)
                    .ok_or_else(|| SceneFileError::NoSource(entry.model.name.clone()))?,
                unit_scale: entry.unit_scale,
            });
        }
        let file = SceneFile {
            version: VERSION,
            models,
            objects: self
                .objects
                .iter()
                .map(|object| SavedObject {
                    id: object.id,
                    model: model_indices[&object.model],
                    transform: object.transform,
                    figure: object.figure,
                })
                .collect(),
            selected: self.selected,
            figure_height: self.figure_height,
            camera: self.camera,
            room: self.room,
            lights: self.lights.clone(),
            shading: self.shading.clone(),
        };
        file.validate()?;
        Ok(ron::ser::to_string_pretty(
            &file,
            ron::ser::PrettyConfig::new().compact_arrays(true),
        )?)
    }

    pub fn from_ron<T>(
        bytes: &[u8],
        mut load: impl FnMut(&str, ModelSource) -> Result<(Model, T), String>,
    ) -> Result<LoadedScene<T>, SceneFileError> {
        let file: SceneFile = ron::de::from_bytes(bytes)?;
        file.validate()?;

        let mut scene = Self {
            next_light: file
                .lights
                .iter()
                .map(|light| light.id.0)
                .max()
                .unwrap_or(0)
                + 1,
            next_object: file
                .objects
                .iter()
                .map(|object| object.id.0)
                .max()
                .unwrap_or(0)
                + 1,
            figure_height: file.figure_height,
            camera: file.camera,
            room: file.room,
            lights: file.lights,
            shading: file.shading,
            ..Self::new()
        };
        let mut models = HashMap::new();
        let mut warnings = Vec::new();
        let mut model_ids = Vec::with_capacity(file.models.len());
        for entry in file.models {
            let id = match load(&entry.name, entry.source) {
                Ok((model, source)) => {
                    let id = scene.add_model(model);
                    scene.models.get_mut(&id).unwrap().unit_scale = entry.unit_scale;
                    models.insert(id, source);
                    Some(id)
                }
                Err(error) => {
                    warnings.push((entry.name, error));
                    None
                }
            };
            model_ids.push(id);
        }
        scene.objects = file
            .objects
            .into_iter()
            .filter_map(|object| {
                Some(Object {
                    id: object.id,
                    model: model_ids[object.model]?,
                    transform: object.transform,
                    figure: object.figure,
                })
            })
            .collect();
        scene.select(file.selected);
        Ok(LoadedScene {
            scene,
            models,
            warnings,
        })
    }
}

impl SceneFile {
    fn validate(&self) -> Result<(), SceneFileError> {
        if self.version != VERSION {
            return Err(SceneFileError::Version(self.version));
        }
        let camera = &self.camera;
        if !camera.target.is_finite()
            || !camera.yaw.is_finite()
            || !(OrbitCamera::MIN_PITCH..=OrbitCamera::MAX_PITCH).contains(&camera.pitch)
            || !positive(camera.distance)
            || !(OrbitCamera::MIN_FOCAL_LENGTH_MM..=OrbitCamera::MAX_FOCAL_LENGTH_MM)
                .contains(&camera.focal_length_mm)
        {
            return Err(SceneFileError::Invalid("camera"));
        }
        if !positive(self.figure_height) {
            return Err(SceneFileError::Invalid("figure_height"));
        }
        if self.models.iter().any(|model| !positive(model.unit_scale)) {
            return Err(SceneFileError::Invalid("models"));
        }
        let mut object_ids = BTreeSet::new();
        if self.objects.iter().any(|object| {
            object.model >= self.models.len()
                || !valid_id(object.id.0)
                || !object_ids.insert(object.id)
                || !object.transform.translation.is_finite()
                || !object.transform.rotation.is_finite()
                || !positive(object.transform.scale)
        }) || self.selected.is_some_and(|id| !object_ids.contains(&id))
        {
            return Err(SceneFileError::Invalid("objects"));
        }
        let mut light_ids = BTreeSet::new();
        if self.lights.len() > MAX_LIGHTS
            || self.lights.iter().any(|light| {
                !valid_id(light.id.0) || !light_ids.insert(light.id) || !light.is_valid()
            })
        {
            return Err(SceneFileError::Invalid("lights"));
        }
        if !self.room.floor_color.is_finite()
            || !self.room.backdrop_color.is_finite()
            || !self.room.ambient_intensity.is_finite()
        {
            return Err(SceneFileError::Invalid("room"));
        }
        let shading = &self.shading;
        let cavity = &shading.cavity;
        let lines = &shading.lines;
        if !shading.clay_color.is_finite() || !lines.color.is_finite() || !lines.wireframe_color.is_finite()
            || ![cavity.geometry_ridge, cavity.geometry_hollow, cavity.screen_ridge, cavity.screen_hollow, lines.thickness, lines.fold_sensitivity, lines.wireframe_opacity, shading.value.balance, shading.value.squint].into_iter().all(f32::is_finite)
            || [shading.value.mode, shading.value.last_mode].into_iter().any(|mode| matches!(mode, ValueMode::Tones(n) if !(ValueMode::MIN_TONES..=ValueMode::MAX_TONES).contains(&n)))
            || shading.matcap.as_ref().is_some_and(|matcap| {
                let image = &matcap.image;
                pixel_count(image.width, image.height).and_then(|n| n.checked_mul(4)) != Some(image.rgba.len())
                    || !image.rgba.iter().all(|channel| channel.is_finite())
            })
        {
            return Err(SceneFileError::Invalid("shading"));
        }
        Ok(())
    }
}

fn valid_id(id: u64) -> bool {
    (1..u64::from(u32::MAX)).contains(&id)
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn pixel_count(width: u32, height: u32) -> Option<usize> {
    (width > 0 && height > 0).then_some(())?;
    usize::try_from(width.checked_mul(height)?).ok()
}

mod base64_bytes {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::encode(bytes))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<[u8]>, D::Error> {
        let encoded = <std::borrow::Cow<'_, str>>::deserialize(deserializer)?;
        base64::decode(encoded.as_bytes())
            .map(Arc::from)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3;

    use super::*;
    use crate::{
        BuiltinRoom, ColorSource, LightKind, LightMask, MaskImage, Matcap, MatcapImage,
        ShadingMode, SolidLighting,
    };

    const MODEL: &[u8] = include_bytes!("../../../../assets/models/test/two_materials.glb");

    #[test]
    fn scene_round_trip_preserves_shared_models_and_render_settings() {
        let mut scene = Scene::new();
        scene.set_figure_height(1.9);
        scene.room = Room::builtin(BuiltinRoom::Dark);
        scene.camera.target = Vec3::new(0.2, 1.0, 0.5);
        scene.camera.yaw = 0.8;
        scene.camera.focal_length_mm = 85.0;
        scene.camera.distance = 4.7;
        scene.shading.mode = ShadingMode::Solid;
        scene.shading.solid = SolidLighting::Matcap;
        scene.shading.matcap = Some(Matcap {
            name: "custom".to_owned(),
            image: Arc::new(MatcapImage::from_fn(2, 2, |u, v| [u, v, 1.3])),
        });
        scene.shading.color = ColorSource::Random;
        scene.shading.cavity.screen = true;
        scene.shading.lines.contours = true;
        scene.shading.lines.wireframe = true;
        scene.shading.backface_culling = true;
        scene.shading.value.set_mode(ValueMode::Tones(4));
        scene.shading.value.squint = 0.3;

        let spot = scene.add_light(LightKind::Spot).unwrap();
        let mut light = scene.light(spot).unwrap().clone();
        light.mask = Some(LightMask {
            name: "window".to_owned(),
            image: Arc::new(MaskImage::from_fn(3, 2, |u, _| (u * 255.0) as u8)),
            pattern_size: 1.5,
        });
        light.shadow_softness = 0.7;
        scene.set_light(light);

        let model = scene.add_model(Model::from_gltf_slice("shared", MODEL).unwrap());
        scene.set_real_size(model, 0.4);
        let first = scene.add_object(model).unwrap();
        let removed = scene.duplicate_object(first).unwrap();
        let second = scene.duplicate_object(first).unwrap();
        scene.remove_object(removed);
        scene.set_figure(first, false);
        scene.set_figure(second, true);
        scene.set_transform(
            second,
            Transform {
                translation: Vec3::new(0.3, 0.2, -0.9),
                rotation: Vec3::new(0.4, -0.5, 0.6),
                scale: 1.1,
            },
        );

        let document = scene
            .to_ron(|_| {
                Some(ModelSource::Embedded {
                    builtin: None,
                    bytes: Arc::from(MODEL),
                })
            })
            .unwrap();
        let mut loaded = Scene::from_ron(document.as_bytes(), |name, source| {
            let ModelSource::Embedded { bytes, .. } = &source else {
                panic!("expected an embedded model");
            };
            assert_eq!(&**bytes, MODEL);
            Ok((Model::from_gltf_slice(name, bytes).unwrap(), source))
        })
        .unwrap();

        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.models.len(), 1);
        assert_eq!(loaded.scene.selected(), Some(second));
        let restored = loaded.scene.objects();
        assert_ne!(restored[0].model, model);
        assert_eq!(restored[0].model, restored[1].model);
        assert_eq!(
            loaded.scene.real_size(restored[0].model),
            scene.real_size(model)
        );
        for (original, restored) in scene.objects().iter().zip(restored) {
            assert_eq!(original.id, restored.id);
            assert_eq!(scene.surface(original), loaded.scene.surface(restored));
            assert_eq!(
                scene.object_matrix(original),
                loaded.scene.object_matrix(restored)
            );
        }

        // Read through the normal model resolver and compare every persisted field.
        assert_eq!(
            loaded
                .scene
                .to_ron(|_| Some(ModelSource::Embedded {
                    builtin: None,
                    bytes: Arc::from(MODEL),
                }))
                .unwrap(),
            document
        );
        assert!(loaded.scene.duplicate_object(second).unwrap().0 > second.0);
        assert!(loaded.scene.add_light(LightKind::Point).unwrap().0 > spot.0);
    }

    #[test]
    fn invalid_scene_structure_is_rejected_before_loading_models() {
        let mut scene = Scene::new();
        let model = scene.add_model(Model::from_gltf_slice("shared", MODEL).unwrap());
        scene.add_object(model).unwrap();
        let document = scene
            .to_ron(|_| Some(ModelSource::Builtin("shared".to_owned())))
            .unwrap();
        let mut file: SceneFile = ron::from_str(&document).unwrap();
        file.objects[0].model = 1;
        let malformed = ron::to_string(&file).unwrap();
        let result = Scene::from_ron::<()>(malformed.as_bytes(), |_, _| {
            panic!("invalid references must be checked before loading")
        });
        assert!(matches!(result, Err(SceneFileError::Invalid("objects"))));
    }
}
