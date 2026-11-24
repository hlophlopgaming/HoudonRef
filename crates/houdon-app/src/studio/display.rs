//! The display controls: shading mode, overlays, colour and value study.

use super::*;

impl Studio {
    pub(super) fn values_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-values"));
        let study = &mut self.scene.shading.value;
        let modes = [ValueMode::Off, ValueMode::Grey]
            .into_iter()
            .chain((ValueMode::MIN_TONES..=ValueMode::MAX_TONES).map(ValueMode::Tones));
        ui.horizontal_wrapped(|ui| {
            for mode in modes {
                let label = match mode {
                    ValueMode::Off => tr.tr("values-off"),
                    ValueMode::Grey => tr.tr("values-grey"),
                    ValueMode::Tones(n) => {
                        tr.tr_args("values-tones", &[("count", i64::from(n).into())])
                    }
                };
                if ui.selectable_label(study.mode == mode, label).clicked() {
                    study.set_mode(mode);
                }
            }
        });
        if matches!(study.mode, ValueMode::Tones(_)) {
            ui.add(egui::Slider::new(&mut study.balance, -1.0..=1.0).text(tr.tr("values-balance")));
        }
        ui.add(egui::Slider::new(&mut study.squint, 0.0..=1.0).text(tr.tr("values-squint")));
        ui.weak(tr.tr("values-hint"));
    }

    pub(super) fn shading_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-shading"));
        let shading = &mut self.scene.shading;
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut shading.mode,
                ShadingMode::Solid,
                tr.tr("shading-solid"),
            );
            ui.selectable_value(&mut shading.mode, ShadingMode::Lit, tr.tr("shading-lit"));
            ui.add_enabled(
                false,
                egui::Button::selectable(false, tr.tr("shading-render")),
            )
            .on_disabled_hover_text(tr.tr("shading-render-later"));
        });
        cavity_controls(ui, tr, &mut shading.cavity);
        line_controls(ui, tr, &mut shading.lines);
        ui.checkbox(
            &mut shading.backface_culling,
            tr.tr("studio-backface-culling"),
        )
        .on_hover_text(tr.tr("backface-culling-hint"));
        if shading.mode != ShadingMode::Solid {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for lighting in SolidLighting::ALL {
                ui.selectable_value(
                    &mut shading.solid,
                    lighting,
                    tr.tr(solid_lighting_message_id(lighting)),
                );
            }
        });
        if shading.solid == SolidLighting::Matcap {
            ui.horizontal(|ui| {
                ui.label(tr.tr("studio-matcap"));
                let name = shading
                    .matcap
                    .as_ref()
                    .map_or_else(|| tr.tr("studio-matcap-none"), |m| m.name.clone());
                if ui.button(name).clicked() {
                    self.matcap_picker_open = true;
                }
            });
        }
    }

    pub(super) fn color_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-color-source"));
        let shading = &mut self.scene.shading;
        ui.horizontal_wrapped(|ui| {
            for source in ColorSource::ALL {
                ui.selectable_value(
                    &mut shading.color,
                    source,
                    tr.tr(color_source_message_id(source)),
                );
            }
        });
        ui.horizontal(|ui| {
            let mut rgb = shading.clay_color.to_array();
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                shading.clay_color = glam::Vec3::from(rgb);
            }
            ui.label(tr.tr("studio-clay-tone"));
        });
    }
}

/// Contour lines and the wireframe.
pub(super) fn line_controls(ui: &mut egui::Ui, tr: &Localizer, lines: &mut Lines) {
    let color_button = |ui: &mut egui::Ui, color: &mut glam::Vec3| {
        let mut rgb = color.to_array();
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            *color = glam::Vec3::from(rgb);
        }
    };
    ui.horizontal(|ui| {
        ui.checkbox(&mut lines.contours, tr.tr("studio-contours"))
            .on_hover_text(tr.tr("contours-hint"));
        if lines.contours {
            color_button(ui, &mut lines.color);
        }
    });
    if lines.contours {
        ui.indent("contours", |ui| {
            ui.add(
                egui::Slider::new(&mut lines.thickness, 1.0..=Lines::MAX_THICKNESS)
                    .suffix(" px")
                    .text(tr.tr("contours-thickness")),
            );
            ui.add(
                egui::Slider::new(&mut lines.fold_sensitivity, 0.0..=1.0)
                    .text(tr.tr("contours-folds")),
            );
        });
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut lines.wireframe, tr.tr("studio-wireframe"));
        if lines.wireframe {
            color_button(ui, &mut lines.wireframe_color);
        }
    });
    if lines.wireframe {
        ui.indent("wireframe", |ui| {
            ui.add(
                egui::Slider::new(&mut lines.wireframe_opacity, 0.0..=1.0)
                    .text(tr.tr("wireframe-opacity")),
            );
        });
    }
}

/// The cavity overlay's two kinds, each with its ridge and hollow strength.
pub(super) fn cavity_controls(ui: &mut egui::Ui, tr: &Localizer, cavity: &mut Cavity) {
    ui.label(tr.tr("studio-cavity"))
        .on_hover_text(tr.tr("cavity-hint"));
    let kinds = [
        (
            "cavity-geometry",
            &mut cavity.geometry,
            &mut cavity.geometry_ridge,
            &mut cavity.geometry_hollow,
        ),
        (
            "cavity-screen",
            &mut cavity.screen,
            &mut cavity.screen_ridge,
            &mut cavity.screen_hollow,
        ),
    ];
    for (label, on, ridge, hollow) in kinds {
        ui.checkbox(on, tr.tr(label));
        if *on {
            ui.indent(label, |ui| {
                ui.add(
                    egui::Slider::new(ridge, 0.0..=Cavity::MAX_STRENGTH)
                        .text(tr.tr("cavity-ridge")),
                );
                ui.add(
                    egui::Slider::new(hollow, 0.0..=Cavity::MAX_STRENGTH)
                        .text(tr.tr("cavity-hollow")),
                );
            });
        }
    }
}
