use houdon_core::LightPreset;
use houdon_i18n::Localizer;

use super::Studio;
use super::scene_files::{FileKind, write_ron};

mod storage;

#[derive(Default)]
pub(super) struct LightPresets {
    built_in: Vec<LightPreset>,
    saved: Vec<LightPreset>,
    selected: Option<usize>,
    name: String,
    status: Option<(String, bool)>,
}

impl LightPresets {
    fn get(&self, index: usize) -> Option<&LightPreset> {
        self.built_in.iter().chain(&self.saved).nth(index)
    }

    fn label(&self, index: usize, preset: &LightPreset, tr: &Localizer) -> String {
        if index < self.built_in.len() {
            tr.tr(&format!("light-preset-builtin-{}", preset.name()))
        } else {
            preset.name().to_owned()
        }
    }

    fn load(&mut self, tr: &Localizer) {
        self.built_in = LightPreset::built_ins();
        match storage::load() {
            Ok(mut presets) => {
                presets.sort_by(|left, right| left.name().cmp(right.name()));
                self.saved = presets;
            }
            Err(reason) => self.report_error("light-preset-load-error", reason, tr),
        }
    }

    fn report_error(&mut self, id: &str, reason: String, tr: &Localizer) {
        self.status = Some((tr.tr_args(id, &[("reason", reason.into())]), true));
    }

    fn keep(&mut self, preset: LightPreset, tr: &Localizer) -> bool {
        if self.saved.iter().any(|saved| saved.name() == preset.name()) {
            self.status = Some((tr.tr("light-preset-duplicate"), true));
            return false;
        }
        if let Err(reason) = storage::save(&preset) {
            self.report_error("light-preset-save-error", reason, tr);
            return false;
        }
        self.selected = Some(self.built_in.len() + self.saved.len());
        self.saved.push(preset);
        self.status = Some((tr.tr("light-preset-saved"), false));
        true
    }

    pub(super) fn import(&mut self, bytes: Result<Vec<u8>, String>, tr: &Localizer) {
        match bytes
            .and_then(|bytes| LightPreset::from_ron(&bytes).map_err(|error| error.to_string()))
        {
            Ok(preset) => {
                if self.keep(preset, tr) {
                    self.status = Some((tr.tr("light-preset-imported"), false));
                }
            }
            Err(reason) => self.report_error("light-preset-import-error", reason, tr),
        }
    }
}

impl Studio {
    pub(crate) fn load_light_presets(&mut self, tr: &Localizer) {
        self.light_presets.load(tr);
    }

    pub(super) fn light_presets_ui(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        let mut open_picker = false;
        egui::CollapsingHeader::new(tr.tr("light-presets"))
            .id_salt("light-presets")
            .show(ui, |ui| {
                let presets = &mut self.light_presets;
                let current = presets
                    .selected
                    .and_then(|index| {
                        presets
                            .get(index)
                            .map(|preset| presets.label(index, preset, tr))
                    })
                    .unwrap_or_else(|| tr.tr("light-preset-choose"));
                egui::ComboBox::from_id_salt("light-preset")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        let mut selected = presets.selected;
                        for (index, preset) in
                            presets.built_in.iter().chain(&presets.saved).enumerate()
                        {
                            ui.selectable_value(
                                &mut selected,
                                Some(index),
                                presets.label(index, preset, tr),
                            );
                        }
                        presets.selected = selected;
                    });
                ui.horizontal_wrapped(|ui| {
                    if let Some(preset) = presets.selected.and_then(|index| presets.get(index))
                        && ui.button(tr.tr("light-preset-apply")).clicked()
                    {
                        preset.apply(&mut self.scene);
                        self.selected_light = None;
                        self.pending_mask = None;
                        presets.status = Some((tr.tr("light-preset-applied"), false));
                    }
                    if let Some(preset) = presets.selected.and_then(|index| presets.get(index))
                        && ui.button(tr.tr("light-preset-export")).clicked()
                    {
                        let result = preset.to_ron().map_err(|error| error.to_string()).and_then(
                            |document| {
                                write_ron(
                                    document.as_bytes(),
                                    &tr.tr("light-preset-file-type"),
                                    "preset.light.ron",
                                )
                            },
                        );
                        if let Err(reason) = result {
                            presets.report_error("light-preset-export-error", reason, tr);
                        }
                    }
                    open_picker = ui.button(tr.tr("light-preset-import")).clicked();
                });

                ui.label(tr.tr("light-preset-name"));
                ui.text_edit_singleline(&mut presets.name);
                if ui
                    .add_enabled(
                        !presets.name.trim().is_empty(),
                        egui::Button::new(tr.tr("light-preset-save-current")),
                    )
                    .clicked()
                {
                    match LightPreset::new(&presets.name, self.scene.lights().to_vec()) {
                        Ok(preset) => {
                            if presets.keep(preset, tr) {
                                presets.name.clear();
                            }
                        }
                        Err(error) => {
                            presets.report_error("light-preset-save-error", error.to_string(), tr)
                        }
                    }
                }
            });
        // An import can also arrive by drag and drop with the controls collapsed.
        if let Some((message, error)) = &self.light_presets.status {
            if *error {
                ui.colored_label(ui.visuals().warn_fg_color, message);
            } else {
                ui.weak(message);
            }
        }
        if open_picker {
            self.pick_file(
                ui.ctx(),
                &tr.tr("light-preset-file-type"),
                FileKind::LightPreset,
            );
        }
    }
}
