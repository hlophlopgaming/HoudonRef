//! The objects section: adding models, the list of objects and the editor
//! of the selected one.

use super::*;

impl Studio {
    /// Name of a model for the interface: translated for built-in models,
    /// the file name for loaded ones.
    pub(super) fn model_display_name(&self, id: ModelId, tr: &Localizer) -> String {
        let builtin = self.sources.get(&id).and_then(|source| source.builtin);
        match builtin {
            Some(builtin) => tr.tr(builtin.message_id()),
            None => self
                .scene
                .model(id)
                .map(|m| m.name.clone())
                .unwrap_or_default(),
        }
    }

    pub(super) fn objects_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-objects"));
        ui.horizontal_wrapped(|ui| {
            for builtin in Builtin::ALL {
                if ui.button(tr.tr(builtin.add_message_id())).clicked() {
                    self.add_builtin(builtin);
                }
            }
        });
        ui.label(tr.tr("studio-drop-hint"));
        if let Some(message) = &self.message {
            ui.colored_label(ui.visuals().warn_fg_color, message);
        }

        // Number objects of the same model: "male figure 1", "male figure 2", …
        let mut seen: HashMap<ModelId, usize> = HashMap::new();
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("objects_list")
            .max_height(160.0)
            .show(ui, |ui| {
                for object in self.scene.objects() {
                    let n = seen.entry(object.model).or_default();
                    *n += 1;
                    let label = tr.tr_args(
                        "studio-object-label",
                        &[
                            ("model", self.model_display_name(object.model, tr).into()),
                            ("number", (*n).into()),
                        ],
                    );
                    let selected = self.scene.selected() == Some(object.id);
                    if ui.selectable_label(selected, label).clicked() {
                        clicked = Some(object.id);
                    }
                }
            });
        if let Some(id) = clicked {
            self.scene.select(Some(id));
        }

        ui.horizontal_wrapped(|ui| {
            let selected = self.scene.selected();
            let button = |ui: &mut egui::Ui, id: &str| {
                ui.add_enabled(selected.is_some(), egui::Button::new(tr.tr(id)))
                    .clicked()
            };
            if button(ui, "studio-object-duplicate")
                && let Some(id) = selected
            {
                self.scene.duplicate_object(id);
            }
            if button(ui, "studio-object-delete")
                && let Some(id) = selected
            {
                self.scene.remove_object(id);
            }
        });
    }

    pub(super) fn selected_object_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-selected-object"));
        let Some(id) = self.scene.selected() else {
            ui.weak(tr.tr("studio-no-selection"));
            return;
        };
        let Some(object) = self.scene.object(id) else {
            return;
        };
        let model_id = object.model;
        let mut figure = object.figure;
        if let Some(model) = self.scene.model(model_id) {
            ui.weak(tr.tr_args(
                "model-stats",
                &[("triangles", model.triangle_count().into())],
            ));
        }
        let size = self.scene.object_bounds(object).size();
        ui.weak(tr.tr_args(
            "studio-dimensions",
            &[
                ("width", format_length(tr, self.units, size.x).into()),
                ("height", format_length(tr, self.units, size.y).into()),
                ("depth", format_length(tr, self.units, size.z).into()),
            ],
        ));
        if ui.checkbox(&mut figure, tr.tr("studio-figure")).changed() {
            self.scene.set_figure(id, figure);
        }
        if !figure {
            self.real_size_editor(ui, tr, model_id);
        }

        let Some(object) = self.scene.object(id) else {
            return;
        };
        let mut transform = object.transform;
        // Show tiny rounding leftovers as 0 instead of "-0.00".
        transform.translation = transform.translation.map(snap_to_zero);
        let mut changed = false;

        ui.label(tr.tr("studio-position"));
        let units = self.units;
        ui.horizontal(|ui| {
            for (axis, value) in [
                ("X", &mut transform.translation.x),
                ("Y", &mut transform.translation.y),
                ("Z", &mut transform.translation.z),
            ] {
                ui.label(axis);
                changed |= length_field(ui, tr, units, value, -10_000.0..=10_000.0);
            }
        });

        ui.label(tr.tr("studio-rotation"));
        let r = transform.rotation;
        let mut angles = [r.y, r.x, r.z].map(|a| snap_to_zero(a.to_degrees()));
        let mut turned = false;
        for (angle, axis) in angles.iter_mut().zip(["Y", "X", "Z"]) {
            turned |= ui
                .add(
                    egui::Slider::new(angle, -180.0..=180.0)
                        .suffix("°")
                        .text(axis),
                )
                .changed();
        }
        if turned {
            let [y, x, z] = angles.map(f32::to_radians);
            transform.rotation = glam::Vec3::new(x, y, z);
            changed = true;
        }

        changed |= ui
            .add(
                egui::Slider::new(&mut transform.scale, 0.01..=100.0)
                    .logarithmic(true)
                    .text(tr.tr("studio-size")),
            )
            .changed();

        if changed {
            self.scene.set_transform(id, transform);
        }
    }

    /// Shows and edits the real size of a model not shown as a Figure, and
    /// warns when its file units look wrong.
    pub(super) fn real_size_editor(&mut self, ui: &mut egui::Ui, tr: &Localizer, model: ModelId) {
        let Some(real_size) = self.scene.real_size(model) else {
            return;
        };
        if self.scene.needs_real_size(model) {
            let warning = tr.tr_args(
                "studio-real-size-warning",
                &[
                    (
                        "smallest",
                        format_length(tr, self.units, *PLAUSIBLE_SIZE.start()).into(),
                    ),
                    (
                        "largest",
                        format_length(tr, self.units, *PLAUSIBLE_SIZE.end()).into(),
                    ),
                ],
            );
            ui.colored_label(ui.visuals().warn_fg_color, warning);
        }
        let mut metres = real_size;
        ui.horizontal(|ui| {
            let edited = length_field(ui, tr, self.units, &mut metres, 0.001..=10_000.0);
            ui.label(tr.tr("studio-real-size"));
            if edited {
                self.scene.set_real_size(model, metres);
            }
        });
    }
}
