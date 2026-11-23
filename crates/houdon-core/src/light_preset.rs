//! Named arrangements of Studio lights, portable as RON.

use std::io;
use std::sync::Arc;

use image::ImageEncoder;
use serde::{Deserialize, Serialize};

use crate::{Light, LightId, LightMask, MAX_LIGHTS, MaskImage, Scene};

const BUILT_IN: &str = include_str!("../../../assets/presets/lights.ron");

#[derive(Debug, thiserror::Error)]
pub enum LightPresetError {
    #[error("{0}")]
    Read(#[from] ron::error::SpannedError),
    #[error("{0}")]
    Write(#[from] ron::Error),
    #[error("invalid light preset name")]
    Name,
    #[error("invalid lights in preset")]
    Lights,
    #[error("invalid light mask in preset")]
    Mask,
    #[error("{0}")]
    MaskFile(#[from] io::Error),
    #[error("{0}")]
    MaskRead(#[from] crate::MaskError),
    #[error("{0}")]
    MaskWrite(#[from] image::ImageError),
}

#[derive(Debug, Clone)]
pub struct LightPreset {
    name: String,
    lights: Vec<Light>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LightPresetFile {
    name: String,
    lights: Vec<Light>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    masks: Vec<MaskFile>,
}

#[derive(Serialize, Deserialize)]
struct MaskFile {
    index: usize,
    name: String,
    pattern_size: f32,
    png_base64: Option<String>,
}

impl LightPreset {
    pub const MAX_NAME_BYTES: usize = 80;

    pub fn new(name: impl Into<String>, mut lights: Vec<Light>) -> Result<Self, LightPresetError> {
        let name = name.into().trim().to_owned();
        if name.is_empty()
            || name.len() > Self::MAX_NAME_BYTES
            || name.chars().any(char::is_control)
        {
            return Err(LightPresetError::Name);
        }
        if lights.len() > MAX_LIGHTS || lights.iter().any(|light| !light.is_valid()) {
            return Err(LightPresetError::Lights);
        }
        for light in &mut lights {
            light.id = LightId(0);
        }
        Ok(Self { name, lights })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn lights(&self) -> &[Light] {
        &self.lights
    }

    pub fn to_ron(&self) -> Result<String, LightPresetError> {
        self.encode(|_, png| Ok(Some(base64::encode(png))))
    }

    /// Saves mask PNGs alongside the RON file, one per light index.
    pub fn to_ron_with_masks(
        &self,
        mut write_mask: impl FnMut(usize, &[u8]) -> io::Result<()>,
    ) -> Result<String, LightPresetError> {
        self.encode(|index, png| {
            write_mask(index, png)?;
            Ok(None)
        })
    }

    fn encode(
        &self,
        mut store_mask: impl FnMut(usize, &[u8]) -> io::Result<Option<String>>,
    ) -> Result<String, LightPresetError> {
        let mut lights = self.lights.clone();
        let mut masks = Vec::new();
        for (index, light) in lights.iter_mut().enumerate() {
            if let Some(mask) = light.mask.take() {
                let mut png = Vec::new();
                let image = mask.image;
                image::codecs::png::PngEncoder::new(&mut png).write_image(
                    &image.luminance,
                    image.width,
                    image.height,
                    image::ExtendedColorType::L8,
                )?;
                masks.push(MaskFile {
                    index,
                    name: mask.name,
                    pattern_size: mask.pattern_size,
                    png_base64: store_mask(index, &png)?,
                });
            }
        }
        let file = LightPresetFile {
            name: self.name.clone(),
            lights,
            masks,
        };
        Ok(ron::ser::to_string_pretty(
            &file,
            ron::ser::PrettyConfig::new(),
        )?)
    }

    pub fn from_ron(bytes: &[u8]) -> Result<Self, LightPresetError> {
        Self::from_ron_with_masks(bytes, |_| {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected an embedded light mask",
            ))
        })
    }

    pub fn from_ron_with_masks(
        bytes: &[u8],
        read_mask: impl FnMut(usize) -> io::Result<Vec<u8>>,
    ) -> Result<Self, LightPresetError> {
        let file: LightPresetFile = ron::de::from_bytes(bytes)?;
        file.decode(read_mask)
    }

    pub fn built_ins() -> Vec<Self> {
        ron::from_str::<Vec<LightPresetFile>>(BUILT_IN)
            .expect("built-in light presets must parse")
            .into_iter()
            .map(|file| {
                file.decode(|_| unreachable!())
                    .expect("valid built-in light preset")
            })
            .collect()
    }

    pub fn apply(&self, scene: &mut Scene) {
        scene.replace_lights(&self.lights);
    }
}

impl LightPresetFile {
    fn decode(
        self,
        mut read_mask: impl FnMut(usize) -> io::Result<Vec<u8>>,
    ) -> Result<LightPreset, LightPresetError> {
        let mut preset = LightPreset::new(self.name, self.lights)?;
        for mask in self.masks {
            let light = preset
                .lights
                .get_mut(mask.index)
                .ok_or(LightPresetError::Mask)?;
            if light.mask.is_some() {
                return Err(LightPresetError::Mask);
            }
            let png = match mask.png_base64 {
                Some(encoded) => base64::decode(encoded).map_err(|_| LightPresetError::Mask)?,
                None => read_mask(mask.index)?,
            };
            light.mask = Some(LightMask {
                name: mask.name,
                pattern_size: mask.pattern_size,
                image: Arc::new(MaskImage::decode(&png)?),
            });
            if !light.is_valid() {
                return Err(LightPresetError::Mask);
            }
        }
        Ok(preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuiltinRoom, LightKind, Model, Room};

    #[test]
    fn every_builtin_parses_and_applies_to_lights_only() {
        let presets = LightPreset::built_ins();
        assert_eq!(presets[0].name(), "three-point");
        assert_eq!(presets[0].lights().len(), 3);

        let mut scene = Scene::new();
        let glb = include_bytes!("../../../assets/models/test/two_materials.glb");
        let model = scene.add_model(Model::from_gltf_slice("figure", glb).unwrap());
        scene.add_object(model).unwrap();
        let objects = scene.objects().to_vec();
        scene.room = Room::builtin(BuiltinRoom::Dark);
        scene.camera.yaw = 1.4;
        let camera = scene.camera;
        let room = scene.room;
        let shading = scene.shading.clone();

        for preset in presets {
            assert!(!preset.lights().is_empty());
            let old_lights: Vec<_> = scene.lights().iter().map(|light| light.id).collect();
            preset.apply(&mut scene);
            assert_eq!(scene.lights().len(), preset.lights().len());
            for (light, expected) in scene.lights().iter().zip(preset.lights()) {
                assert!(!old_lights.contains(&light.id));
                assert_eq!(
                    &Light {
                        id: LightId(0),
                        ..light.clone()
                    },
                    expected
                );
            }
            assert_eq!(scene.objects(), objects);
            assert_eq!(scene.room, room);
            assert_eq!(scene.camera, camera);
            assert_eq!(scene.shading, shading);
        }
    }

    #[test]
    fn portable_preset_round_trips_light_settings_and_the_custom_mask() {
        let mut light = Light::new(LightKind::Spot);
        light.shadow_softness = 0.6;
        light.mask = Some(LightMask {
            name: "Custom window.jpg".into(),
            image: Arc::new(MaskImage::from_fn(3, 2, |x, y| (x * y * 255.0) as u8)),
            pattern_size: 4.1,
        });
        let preset = LightPreset::new("Window study", vec![light]).unwrap();
        let document = preset.to_ron().unwrap();
        let decoded = LightPreset::from_ron(document.as_bytes()).unwrap();
        assert_eq!(decoded.to_ron().unwrap(), document);
        assert_eq!(
            decoded.lights()[0].mask.as_ref().unwrap().image,
            preset.lights()[0].mask.as_ref().unwrap().image,
        );
    }

    #[test]
    fn a_preset_rejects_invalid_light_data() {
        let mut light = Light::new(LightKind::Directional);
        assert!(matches!(
            LightPreset::new("Too many", vec![light.clone(); MAX_LIGHTS + 1]),
            Err(LightPresetError::Lights)
        ));
        light.distance = f32::NAN;
        assert!(matches!(
            LightPreset::new("Bad distance", vec![light]),
            Err(LightPresetError::Lights)
        ));

        let preset = LightPreset::built_ins().remove(0);
        let mut file: LightPresetFile = ron::from_str(&preset.to_ron().unwrap()).unwrap();
        file.masks.push(MaskFile {
            index: 50,
            name: "out of bounds.png".into(),
            pattern_size: 1.0,
            png_base64: None,
        });
        assert!(matches!(
            file.decode(|_| panic!("invalid index must not read a sidecar")),
            Err(LightPresetError::Mask)
        ));
    }
}
