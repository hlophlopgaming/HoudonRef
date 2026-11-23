use houdon_core::LightPreset;

#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::{load, save};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::LightPreset;
    use std::error::Error;
    use std::fs;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};

    fn directory() -> Result<PathBuf, String> {
        eframe::storage_dir("HoudonRef")
            .map(|directory| directory.join("light-presets"))
            .ok_or_else(|| "app settings directory is unavailable".to_owned())
    }

    pub(in super::super) fn load() -> Result<Vec<LightPreset>, String> {
        load_from(&directory()?).map_err(|error| error.to_string())
    }

    pub(in super::super) fn save(preset: &LightPreset) -> Result<(), String> {
        save_into(&directory()?, preset).map_err(|error| error.to_string())
    }

    fn load_from(directory: &Path) -> Result<Vec<LightPreset>, Box<dyn Error>> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut presets = Vec::new();
        for entry in entries {
            let entry = entry?;
            // Published folders have a hex name. A staging directory begins with a dot.
            if entry.file_type()?.is_dir()
                && entry
                    .file_name()
                    .as_encoded_bytes()
                    .iter()
                    .all(u8::is_ascii_hexdigit)
            {
                match read_preset(&entry.path()) {
                    Ok(preset) => presets.push(preset),
                    Err(error) => log::warn!(
                        "could not load light preset from {}: {error}",
                        entry.path().display()
                    ),
                }
            }
        }
        Ok(presets)
    }

    fn read_preset(directory: &Path) -> Result<LightPreset, Box<dyn Error>> {
        let document = fs::read(directory.join("preset.ron"))?;
        Ok(LightPreset::from_ron_with_masks(&document, |index| {
            fs::read(directory.join(format!("mask-{index}.png")))
        })?)
    }

    fn save_into(directory: &Path, preset: &LightPreset) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(directory)?;
        let name: String = preset
            .name()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let staging = tempfile::Builder::new()
            .prefix(".preset-")
            .tempdir_in(directory)?;
        let document = preset.to_ron_with_masks(|index, png| {
            write_synced(&staging.path().join(format!("mask-{index}.png")), png)
        })?;
        write_synced(&staging.path().join("preset.ron"), document.as_bytes())?;
        fs::rename(staging.path(), directory.join(name))?;
        Ok(())
    }

    fn write_synced(path: &Path, data: &[u8]) -> io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(data)?;
        file.sync_all()
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;

        use houdon_core::{Light, LightKind, LightMask, MaskImage};

        use super::*;

        #[test]
        fn desktop_preset_round_trips_a_mask_next_to_the_ron_file() {
            let directory = tempfile::tempdir().unwrap();
            let mut light = Light::new(LightKind::Spot);
            light.mask = Some(LightMask {
                name: "window.png".into(),
                pattern_size: 3.2,
                image: Arc::new(MaskImage::from_fn(2, 3, |u, v| (u * v * 255.0) as u8)),
            });
            let preset = LightPreset::new("Window", vec![light]).unwrap();
            save_into(directory.path(), &preset).unwrap();
            let stored = directory
                .path()
                .read_dir()
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert!(stored.join("mask-0.png").is_file());
            assert!(stored.join("preset.ron").is_file());
            assert!(
                save_into(directory.path(), &preset).is_err(),
                "keep the existing preset"
            );
            let loaded = load_from(directory.path()).unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].to_ron().unwrap(), preset.to_ron().unwrap());
        }
    }
}

#[cfg(target_arch = "wasm32")]
const PREFIX: &str = "houdonref-light-preset:";

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or_else(|| "browser window is unavailable".to_owned())?
        .local_storage()
        .map_err(js_error)?
        .ok_or_else(|| "browser storage is unavailable".to_owned())
}

#[cfg(target_arch = "wasm32")]
fn js_error(error: wasm_bindgen::JsValue) -> String {
    format!("{error:?}")
}

#[cfg(target_arch = "wasm32")]
pub(super) fn load() -> Result<Vec<LightPreset>, String> {
    let storage = storage()?;
    let mut presets = Vec::new();
    for index in 0..storage.length().map_err(js_error)? {
        if let Some(key) = storage.key(index).map_err(js_error)?
            && key.starts_with(PREFIX)
            && let Some(data) = storage.get_item(&key).map_err(js_error)?
        {
            match LightPreset::from_ron(data.as_bytes()) {
                Ok(preset) => presets.push(preset),
                Err(error) => log::warn!("could not load light preset {key}: {error}"),
            }
        }
    }
    Ok(presets)
}

#[cfg(target_arch = "wasm32")]
pub(super) fn save(preset: &LightPreset) -> Result<(), String> {
    let storage = storage()?;
    let key = format!("{PREFIX}{}", preset.name());
    if storage.get_item(&key).map_err(js_error)?.is_some() {
        return Err("a light preset with this name already exists".into());
    }
    let data = preset.to_ron().map_err(|error| error.to_string())?;
    storage.set_item(&key, &data).map_err(js_error)
}
