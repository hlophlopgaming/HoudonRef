//! The camera and room sections.

use super::*;

impl Studio {
    pub(super) fn camera_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        // Built-in viewpoints, one click each.
        ui.horizontal_wrapped(|ui| {
            for preset in CameraPreset::built_ins() {
                let label = tr.tr(&format!("camera-preset-{}", preset.name()));
                let hint = tr.tr_args(
                    "camera-preset-lens",
                    &[("mm", (preset.focal_length_mm().round() as i64).into())],
                );
                if ui.button(label).on_hover_text(hint).clicked() {
                    preset.apply(&mut self.scene, self.camera_target, self.view_aspect);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr.tr("camera-target"));
            ui.selectable_value(
                &mut self.camera_target,
                CameraTarget::Scene,
                tr.tr("camera-target-scene"),
            );
            ui.selectable_value(
                &mut self.camera_target,
                CameraTarget::Selected,
                tr.tr("camera-target-selected"),
            );
        });
        let mut action = None;
        egui::CollapsingHeader::new(tr.tr("camera-presets-mine"))
            .id_salt("camera-presets-mine")
            .show(ui, |ui| action = self.camera_presets.ui(ui, tr));
        match action {
            Some(PresetAction::Apply(preset)) => {
                preset.apply(&mut self.scene, self.camera_target, self.view_aspect);
            }
            Some(PresetAction::SaveCurrent(name)) => {
                let bounds = self.scene.framing_bounds(self.camera_target);
                match CameraPreset::from_camera(
                    &name,
                    &self.scene.camera,
                    &bounds,
                    self.view_aspect,
                ) {
                    Ok(preset) => {
                        self.camera_presets.keep(preset, tr);
                    }
                    Err(error) => self.camera_presets.report(Err(error.to_string()), tr),
                }
            }
            Some(PresetAction::Import) => {
                self.pick_file(ui.ctx(), &tr.tr("preset-file-type"), FileKind::CameraPreset);
            }
            None => {}
        }
        ui.separator();
        let mut focal = self.scene.camera.focal_length_mm;
        let slider = egui::Slider::new(
            &mut focal,
            OrbitCamera::MIN_FOCAL_LENGTH_MM..=OrbitCamera::MAX_FOCAL_LENGTH_MM,
        )
        .logarithmic(true)
        .suffix(format!(" {}", tr.tr("unit-millimetre-short")))
        .text(tr.tr("studio-focal-length"));
        if ui.add(slider).changed() {
            self.scene.camera.set_focal_length_keep_framing(focal);
        }
        let camera = &mut self.scene.camera;
        // Shown around the front (0°) from -180° to 180°.
        let mut direction = snap_to_zero(wrap_degrees(camera.yaw.to_degrees()));
        if ui
            .add(
                egui::Slider::new(&mut direction, -180.0..=180.0)
                    .suffix("°")
                    .text(tr.tr("studio-camera-direction")),
            )
            .changed()
        {
            camera.yaw = direction.to_radians();
        }
        let mut height = snap_to_zero(camera.pitch.to_degrees());
        let pitch_range = OrbitCamera::MIN_PITCH.to_degrees()..=OrbitCamera::MAX_PITCH.to_degrees();
        if ui
            .add(
                egui::Slider::new(&mut height, pitch_range)
                    .suffix("°")
                    .text(tr.tr("studio-camera-height")),
            )
            .changed()
        {
            camera.pitch = height.to_radians();
        }
        ui.horizontal(|ui| {
            length_field(
                ui,
                tr,
                self.units,
                &mut camera.distance,
                OrbitCamera::MIN_DISTANCE..=OrbitCamera::MAX_DISTANCE,
            );
            ui.label(tr.tr("studio-camera-distance"));
        });
    }

    pub(super) fn room_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.horizontal_wrapped(|ui| {
            for builtin in BuiltinRoom::ALL {
                if ui.button(tr.tr(builtin_room_message_id(builtin))).clicked() {
                    self.scene.use_builtin_room(builtin);
                }
            }
        });
        let room = &mut self.scene.room;
        ui.checkbox(&mut room.visible, tr.tr("studio-room-visible"));
        for (color, label) in [
            (&mut room.floor_color, "studio-room-floor"),
            (&mut room.backdrop_color, "studio-room-backdrop"),
        ] {
            ui.horizontal(|ui| {
                let mut rgb = color.to_array();
                if ui.color_edit_button_rgb(&mut rgb).changed() {
                    *color = glam::Vec3::from(rgb);
                }
                ui.label(tr.tr(label));
            });
        }
        ui.add(
            egui::Slider::new(&mut room.ambient_intensity, 0.0..=2.0).text(tr.tr("studio-ambient")),
        );
        let mut action = None;
        egui::CollapsingHeader::new(tr.tr("room-presets-mine"))
            .id_salt("room-presets-mine")
            .show(ui, |ui| action = self.room_presets.ui(ui, tr));
        match action {
            Some(PresetAction::Apply(preset)) => preset.apply(&mut self.scene),
            Some(PresetAction::SaveCurrent(name)) => {
                match RoomPreset::new(&name, self.scene.room) {
                    Ok(preset) => {
                        self.room_presets.keep(preset, tr);
                    }
                    Err(error) => self.room_presets.report(Err(error.to_string()), tr),
                }
            }
            Some(PresetAction::Import) => {
                self.pick_file(ui.ctx(), &tr.tr("preset-file-type"), FileKind::RoomPreset);
            }
            None => {}
        }
    }
}
