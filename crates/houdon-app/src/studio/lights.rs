//! The lights section: the list of lights and the editor of the selected one.

use super::*;

impl Studio {
    pub(super) fn lights_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        self.light_presets_ui(ui, tr);
        let can_add = self.scene.lights().len() < MAX_LIGHTS;
        ui.add_enabled_ui(can_add, |ui| {
            ui.menu_button(tr.tr("studio-add-light-menu"), |ui| {
                for kind in LightKind::ALL {
                    if ui.button(tr.tr(light_kind_message_id(kind))).clicked() {
                        self.selected_light = self.scene.add_light(kind);
                        ui.close();
                    }
                }
            });
        });
        if !can_add {
            ui.weak(tr.tr_args("studio-lights-full", &[("count", MAX_LIGHTS.into())]));
        }

        // The list: an on/off box and a selectable name per light.
        let mut toggled = None;
        for (index, light) in self.scene.lights().iter().enumerate() {
            ui.horizontal(|ui| {
                let mut enabled = light.enabled;
                if ui.checkbox(&mut enabled, "").changed() {
                    toggled = Some((light.id, enabled));
                }
                let name = tr.tr_args(
                    "studio-light-label",
                    &[
                        ("number", (index + 1).into()),
                        ("kind", tr.tr(light_kind_message_id(light.kind)).into()),
                    ],
                );
                if ui
                    .selectable_label(self.selected_light == Some(light.id), name)
                    .clicked()
                {
                    self.selected_light = Some(light.id);
                }
            });
        }
        if let Some((id, enabled)) = toggled
            && let Some(light) = self.scene.light(id)
        {
            let light = Light {
                enabled,
                ..light.clone()
            };
            self.scene.set_light(light);
        }

        let Some(mut light) = self
            .selected_light
            .and_then(|id| self.scene.light(id))
            .cloned()
        else {
            self.selected_light = None;
            return;
        };
        let before = light.clone();
        let mut remove = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("light_kind")
                .selected_text(tr.tr(light_kind_message_id(light.kind)))
                .show_ui(ui, |ui| {
                    for kind in LightKind::ALL {
                        ui.selectable_value(
                            &mut light.kind,
                            kind,
                            tr.tr(light_kind_message_id(kind)),
                        );
                    }
                });
            let mut color = light.color.to_array();
            ui.color_edit_button_rgb(&mut color);
            light.color = glam::Vec3::from(color);
            remove = ui.button(tr.tr("studio-light-remove")).clicked();
        });
        ui.checkbox(
            &mut light.casts_shadows,
            tr.tr("studio-light-casts-shadows"),
        );
        if light.casts_shadows {
            ui.add(
                egui::Slider::new(&mut light.shadow_softness, 0.0..=1.0)
                    .text(tr.tr("studio-light-shadow-softness")),
            )
            .on_hover_text(tr.tr("studio-light-shadow-softness-hint"));
        }
        if light.kind != before.kind {
            let kind = light.kind;
            light.kind = before.kind;
            light.set_kind(kind);
        }
        let max_intensity = if light.kind.is_positional() {
            200.0
        } else {
            5.0
        };
        ui.add(
            egui::Slider::new(&mut light.intensity, 0.0..=max_intensity)
                .clamping(egui::SliderClamping::Edits)
                .logarithmic(light.kind.is_positional())
                .text(tr.tr("studio-light-intensity")),
        );
        let mut azimuth = snap_to_zero(light.azimuth.to_degrees());
        let mut elevation = snap_to_zero(light.elevation.to_degrees());
        if ui
            .add(
                egui::Slider::new(&mut azimuth, -180.0..=180.0)
                    .suffix("°")
                    .text(tr.tr("studio-light-azimuth")),
            )
            .changed()
        {
            light.azimuth = azimuth.to_radians();
        }
        if ui
            .add(
                egui::Slider::new(&mut elevation, -89.0..=89.0)
                    .suffix("°")
                    .text(tr.tr("studio-light-elevation")),
            )
            .changed()
        {
            light.elevation = elevation.to_radians();
        }
        if light.kind.is_positional() {
            ui.horizontal(|ui| {
                length_field(ui, tr, self.units, &mut light.distance, 0.1..=100.0);
                ui.label(tr.tr("studio-light-distance"));
            });
        }
        if light.kind == LightKind::Spot {
            let mut cone = light.cone_angle.to_degrees();
            if ui
                .add(
                    egui::Slider::new(&mut cone, 1.0..=170.0)
                        .suffix("°")
                        .text(tr.tr("studio-light-cone")),
                )
                .changed()
            {
                light.cone_angle = cone.to_radians();
            }
            ui.add(
                egui::Slider::new(&mut light.softness, 0.0..=1.0)
                    .text(tr.tr("studio-light-softness")),
            );
        }

        if light.kind.takes_mask() {
            ui.horizontal(|ui| {
                ui.label(tr.tr("studio-light-mask"));
                let name = light
                    .mask
                    .as_ref()
                    .map_or_else(|| tr.tr("studio-light-mask-none"), |m| m.name.clone());
                if ui.button(name).clicked() {
                    self.mask_picker_open = true;
                }
                if light.mask.is_some() && ui.button(tr.tr("studio-light-mask-remove")).clicked() {
                    light.mask = None;
                }
            });
            if light.kind == LightKind::Directional
                && let Some(mask) = &mut light.mask
            {
                ui.horizontal(|ui| {
                    length_field(ui, tr, self.units, &mut mask.pattern_size, 0.05..=50.0);
                    ui.label(tr.tr("studio-light-mask-pattern"));
                });
            }
        }

        if remove {
            self.scene.remove_light(light.id);
            self.selected_light = None;
        } else if light != before {
            self.scene.set_light(light);
        }
    }
}
