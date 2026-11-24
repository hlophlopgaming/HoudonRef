//! The toolbar over the viewport: the scene file, adding models, the
//! shading mode, the value study, framing, and help with the controls.

use super::display::shading_mode_buttons;
use super::*;

impl Studio {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(tr.tr("toolbar-scene"), |ui| {
                if ui.button(tr.tr("scene-open")).clicked() {
                    self.pick_file(ui.ctx(), &tr.tr("scene-file-type"), FileKind::Scene);
                    ui.close();
                }
                if ui.button(tr.tr("scene-save")).clicked() {
                    self.save_scene(tr);
                    ui.close();
                }
                #[cfg(not(target_arch = "wasm32"))]
                ui.checkbox(&mut self.embed_models, tr.tr("scene-embed-models"));
            });
            ui.menu_button(tr.tr("toolbar-add"), |ui| self.add_menu(ui, tr));
            ui.separator();

            shading_mode_buttons(ui, tr, &mut self.scene.shading.mode);
            let study = &mut self.scene.shading.value;
            let mut on = study.mode != ValueMode::Off;
            if ui
                .toggle_value(&mut on, tr.tr("toolbar-values"))
                .on_hover_text(tr.tr("values-key-hint"))
                .changed()
            {
                study.toggle();
            }
            ui.separator();

            if ui
                .button(tr.tr("studio-frame-scene"))
                .on_hover_text(tr.tr("toolbar-frame-scene-hint"))
                .clicked()
            {
                self.pending_frame = Some(FrameTarget::Scene);
            }
            if ui
                .add_enabled(
                    self.scene.selected().is_some(),
                    egui::Button::new(tr.tr("studio-frame-selected")),
                )
                .on_hover_text(tr.tr("toolbar-frame-selected-hint"))
                .clicked()
            {
                self.pending_frame = Some(FrameTarget::Selected);
            }
            ui.separator();
            ui.menu_button("?", |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width(340.0);
                    ui.label(tr.tr("studio-camera-hint"));
                });
            })
            .response
            .on_hover_text(tr.tr("toolbar-help"));
        });
    }

    /// The Add menu: the built-in models, then a model from a file.
    fn add_menu(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        let mut group = None;
        for builtin in Builtin::ALL {
            if group != Some(builtin.group()) {
                if group.is_some() {
                    ui.separator();
                }
                group = Some(builtin.group());
                ui.weak(tr.tr(builtin.group()));
            }
            if ui.button(tr.tr(builtin.menu_message_id())).clicked() {
                self.add_builtin(builtin);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr.tr("toolbar-open-model")).clicked() {
            self.pick_file(ui.ctx(), &tr.tr("model-file-type"), FileKind::Model);
            ui.close();
        }
        ui.weak(tr.tr("studio-drop-hint"));
    }

    /// The last error or warning over the viewport, until it is dismissed.
    pub(super) fn message_banner(&mut self, ui: &mut egui::Ui) {
        let Some(message) = &self.message else {
            return;
        };
        let mut dismissed = false;
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    dismissed = ui.small_button("✕").clicked();
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                });
            });
        if dismissed {
            self.message = None;
        }
    }
}
