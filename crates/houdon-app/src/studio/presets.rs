//! The artist's own Camera and Room presets: saved in the settings folder
//! on desktop and in browser storage on the web, imported and exported as
//! RON files. (Light presets carry mask images and have their own module.)

use houdon_core::{CameraPreset, RoomPreset};
use houdon_i18n::Localizer;

/// A preset kept as one RON document.
pub(super) trait PlainPreset: Sized + Clone {
    /// Names the storage folder or key and the file suffix: `KIND.ron`.
    const KIND: &'static str;
    fn name(&self) -> &str;
    fn to_ron(&self) -> Result<String, String>;
    fn from_ron(bytes: &[u8]) -> Result<Self, String>;

    fn file_name() -> String {
        format!("preset.{}.ron", Self::KIND)
    }
}

impl PlainPreset for CameraPreset {
    const KIND: &'static str = "camera";
    fn name(&self) -> &str {
        CameraPreset::name(self)
    }
    fn to_ron(&self) -> Result<String, String> {
        CameraPreset::to_ron(self).map_err(|e| e.to_string())
    }
    fn from_ron(bytes: &[u8]) -> Result<Self, String> {
        CameraPreset::from_ron(bytes).map_err(|e| e.to_string())
    }
}

impl PlainPreset for RoomPreset {
    const KIND: &'static str = "room";
    fn name(&self) -> &str {
        RoomPreset::name(self)
    }
    fn to_ron(&self) -> Result<String, String> {
        RoomPreset::to_ron(self).map_err(|e| e.to_string())
    }
    fn from_ron(bytes: &[u8]) -> Result<Self, String> {
        RoomPreset::from_ron(bytes).map_err(|e| e.to_string())
    }
}

/// What the artist asked of the saved presets' controls.
pub(super) enum PresetAction<P> {
    Apply(P),
    /// Save the scene's current state under this name.
    SaveCurrent(String),
    Import,
}

/// The artist's saved presets of one kind and the controls' state.
pub(super) struct SavedPresets<P> {
    saved: Vec<P>,
    selected: Option<usize>,
    name: String,
    /// The last outcome, and whether it is an error.
    status: Option<(String, bool)>,
}

impl<P> Default for SavedPresets<P> {
    fn default() -> Self {
        Self {
            saved: Vec::new(),
            selected: None,
            name: String::new(),
            status: None,
        }
    }
}

impl<P: PlainPreset> SavedPresets<P> {
    pub(super) fn load(&mut self, tr: &Localizer) {
        match storage::load::<P>() {
            Ok(mut presets) => {
                presets.sort_by(|a, b| a.name().cmp(b.name()));
                self.saved = presets;
            }
            Err(reason) => self.error("preset-load-error", reason, tr),
        }
    }

    fn error(&mut self, id: &str, reason: String, tr: &Localizer) {
        self.status = Some((tr.tr_args(id, &[("reason", reason.into())]), true));
    }

    /// Saves `preset` unless one of that name exists. Returns whether it did.
    pub(super) fn keep(&mut self, preset: P, tr: &Localizer) -> bool {
        if self.saved.iter().any(|saved| saved.name() == preset.name()) {
            self.status = Some((tr.tr("preset-duplicate"), true));
            return false;
        }
        if let Err(reason) = storage::save(&preset) {
            self.error("preset-save-error", reason, tr);
            return false;
        }
        self.saved.push(preset);
        self.selected = Some(self.saved.len() - 1);
        self.name.clear();
        self.status = Some((tr.tr("preset-saved"), false));
        true
    }

    pub(super) fn import(&mut self, bytes: Result<Vec<u8>, String>, tr: &Localizer) {
        match bytes.and_then(|bytes| P::from_ron(&bytes)) {
            Ok(preset) => {
                if self.keep(preset, tr) {
                    self.status = Some((tr.tr("preset-imported"), false));
                }
            }
            Err(reason) => self.error("preset-import-error", reason, tr),
        }
    }

    pub(super) fn report(&mut self, error: Result<(), String>, tr: &Localizer) {
        match error {
            Ok(()) => self.status = None,
            Err(reason) => self.error("preset-save-error", reason, tr),
        }
    }

    /// The saved presets' controls: pick one, apply or export it, import a
    /// file, or save the current state under a name.
    pub(super) fn ui(&mut self, ui: &mut egui::Ui, tr: &Localizer) -> Option<PresetAction<P>> {
        let mut action = None;
        if self.saved.is_empty() {
            ui.weak(tr.tr("preset-none-saved"));
        } else {
            let current = self
                .selected
                .and_then(|i| self.saved.get(i))
                .map_or_else(|| tr.tr("preset-choose"), |p| p.name().to_owned());
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(format!("{}-presets", P::KIND))
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (index, preset) in self.saved.iter().enumerate() {
                            ui.selectable_value(&mut self.selected, Some(index), preset.name());
                        }
                    });
                let chosen = self.selected.and_then(|i| self.saved.get(i)).cloned();
                if let Some(preset) = chosen {
                    if ui.button(tr.tr("preset-apply")).clicked() {
                        action = Some(PresetAction::Apply(preset.clone()));
                    }
                    if ui.button(tr.tr("preset-export")).clicked() {
                        let result = preset.to_ron().and_then(|document| {
                            super::scene_files::write_ron(
                                document.as_bytes(),
                                &tr.tr("preset-file-type"),
                                &P::file_name(),
                            )
                        });
                        if let Err(reason) = result {
                            self.error("preset-export-error", reason, tr);
                        }
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text(tr.tr("preset-name"))
                    .desired_width(130.0),
            );
            if ui
                .add_enabled(
                    !self.name.trim().is_empty(),
                    egui::Button::new(tr.tr("preset-save-current")),
                )
                .clicked()
            {
                action = Some(PresetAction::SaveCurrent(self.name.clone()));
            }
        });
        if ui.button(tr.tr("preset-import")).clicked() {
            action = Some(PresetAction::Import);
        }
        if let Some((message, error)) = &self.status {
            if *error {
                ui.colored_label(ui.visuals().warn_fg_color, message);
            } else {
                ui.weak(message);
            }
        }
        action
    }
}

mod storage {
    use super::PlainPreset;

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) use native::{load, save};
    #[cfg(target_arch = "wasm32")]
    pub(super) use web::{load, save};

    /// A file or key name for a preset name, safe on every file system.
    fn hex(name: &str) -> String {
        name.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod native {
        use std::fs;
        use std::io::Write;
        use std::path::{Path, PathBuf};

        use super::{PlainPreset, hex};

        fn directory<P: PlainPreset>() -> Result<PathBuf, String> {
            eframe::storage_dir("HoudonRef")
                .map(|directory| directory.join(format!("{}-presets", P::KIND)))
                .ok_or_else(|| "app settings directory is unavailable".to_owned())
        }

        pub(in super::super) fn load<P: PlainPreset>() -> Result<Vec<P>, String> {
            load_from(&directory::<P>()?)
        }

        pub(in super::super) fn save<P: PlainPreset>(preset: &P) -> Result<(), String> {
            save_into(&directory::<P>()?, preset)
        }

        pub(super) fn load_from<P: PlainPreset>(directory: &Path) -> Result<Vec<P>, String> {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Vec::new());
                }
                Err(error) => return Err(error.to_string()),
            };
            let mut presets = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "ron") {
                    match fs::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|b| P::from_ron(&b))
                    {
                        Ok(preset) => presets.push(preset),
                        Err(error) => log::warn!("skipped preset {}: {error}", path.display()),
                    }
                }
            }
            Ok(presets)
        }

        /// Writes the preset whole or not at all, and never over another.
        pub(super) fn save_into<P: PlainPreset>(
            directory: &Path,
            preset: &P,
        ) -> Result<(), String> {
            let document = preset.to_ron()?;
            fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
            file.write_all(document.as_bytes())
                .and_then(|()| file.as_file().sync_all())
                .map_err(|e| e.to_string())?;
            file.persist_noclobber(directory.join(format!("{}.ron", hex(preset.name()))))
                .map_err(|e| e.error.to_string())?;
            Ok(())
        }
    }

    #[cfg(target_arch = "wasm32")]
    mod web {
        use super::{PlainPreset, hex};

        fn storage() -> Result<web_sys::Storage, String> {
            web_sys::window()
                .ok_or_else(|| "browser window is unavailable".to_owned())?
                .local_storage()
                .map_err(|e| format!("{e:?}"))?
                .ok_or_else(|| "browser storage is unavailable".to_owned())
        }

        fn prefix<P: PlainPreset>() -> String {
            format!("houdonref-{}-preset:", P::KIND)
        }

        pub(in super::super) fn load<P: PlainPreset>() -> Result<Vec<P>, String> {
            let storage = storage()?;
            let prefix = prefix::<P>();
            let mut presets = Vec::new();
            for index in 0..storage.length().map_err(|e| format!("{e:?}"))? {
                if let Ok(Some(key)) = storage.key(index)
                    && key.starts_with(&prefix)
                    && let Ok(Some(data)) = storage.get_item(&key)
                {
                    match P::from_ron(data.as_bytes()) {
                        Ok(preset) => presets.push(preset),
                        Err(error) => log::warn!("skipped preset {key}: {error}"),
                    }
                }
            }
            Ok(presets)
        }

        pub(in super::super) fn save<P: PlainPreset>(preset: &P) -> Result<(), String> {
            let storage = storage()?;
            let key = format!("{}{}", prefix::<P>(), hex(preset.name()));
            if storage
                .get_item(&key)
                .map_err(|e| format!("{e:?}"))?
                .is_some()
            {
                return Err("a preset with this name already exists".into());
            }
            storage
                .set_item(&key, &preset.to_ron()?)
                .map_err(|e| format!("{e:?}"))
        }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    mod tests {
        use houdon_core::{BuiltinRoom, CameraPreset, Room, RoomPreset};

        use super::native::{load_from, save_into};

        #[test]
        fn saved_presets_load_back_and_are_never_overwritten() {
            let directory = tempfile::tempdir().unwrap();
            let camera = CameraPreset::new("Low / wide", 0.3, -0.2, 24.0, 1.2).unwrap();
            save_into(directory.path(), &camera).unwrap();
            assert!(
                save_into(directory.path(), &camera).is_err(),
                "keeps the first"
            );
            assert_eq!(
                load_from::<CameraPreset>(directory.path()).unwrap(),
                [camera]
            );

            let room = RoomPreset::new("Mine", Room::builtin(BuiltinRoom::Dark)).unwrap();
            let rooms = tempfile::tempdir().unwrap();
            save_into(rooms.path(), &room).unwrap();
            assert_eq!(load_from::<RoomPreset>(rooms.path()).unwrap(), [room]);
        }
    }
}
