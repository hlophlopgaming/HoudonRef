//! The 3D viewport: camera and selection by mouse, light icons, and the
//! picture drawn by the renderer.

use super::*;

impl Studio {
    /// Draws where each light is: a dot in its colour for point and spot
    /// lights, and for directional lights a dot far out in their direction
    /// with a line towards the scene centre. The selected light is ringed.
    pub(super) fn paint_light_icons(&self, painter: &egui::Painter, rect: egui::Rect, aspect: f32) {
        let view_proj = self.scene.camera.view_projection(aspect);
        let to_screen = |world: glam::Vec3| -> Option<egui::Pos2> {
            let clip = view_proj * world.extend(1.0);
            if clip.w <= 1e-4 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            Some(egui::pos2(
                rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
                rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
            ))
        };
        let centre = self.scene.light_centre();
        // On the scene's bounding sphere: near the edge of a framed view, clear of the model.
        let reach = self.scene.bounds().radius().max(0.5);
        for light in self.scene.lights() {
            let position = if light.kind.is_positional() {
                light.position(centre)
            } else {
                centre + light.direction() * reach
            };
            let Some(at) = to_screen(position) else {
                continue;
            };
            let [r, g, b] = (light.color / light.color.max_element().max(1e-4) * 255.0)
                .to_array()
                .map(|c| c as u8);
            let fill = if light.enabled {
                egui::Color32::from_rgb(r, g, b)
            } else {
                egui::Color32::from_gray(90)
            };
            if light.kind == LightKind::Directional
                && let Some(target) = to_screen(centre)
            {
                let towards = at + (target - at).normalized() * 18.0;
                painter.line_segment([at, towards], egui::Stroke::new(1.5, fill));
            }
            painter.circle(at, 6.0, fill, egui::Stroke::new(1.0, egui::Color32::BLACK));
            if self.selected_light == Some(light.id) {
                painter.circle_stroke(at, 9.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
        }
    }

    pub(super) fn viewport(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let aspect = rect.width() / rect.height().max(1.0);

        if self.scene.object_count() > 0 {
            match self.pending_frame.take() {
                Some(FrameTarget::Scene) => self.scene.frame_camera(aspect),
                Some(FrameTarget::Selected) => self.scene.frame_selected(aspect),
                None => {}
            }
        }

        let drag = response.drag_delta();
        let height = rect.height().max(1.0);
        let pan = response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged_by(egui::PointerButton::Primary)
                && ui.input(|i| i.modifiers.shift));
        if pan {
            self.scene.camera.pan(drag.x / height, drag.y / height);
        } else if response.dragged_by(egui::PointerButton::Primary) {
            // A drag across the full viewport height turns the model half way round.
            let speed = std::f32::consts::PI / height;
            self.scene.camera.orbit(-drag.x * speed, drag.y * speed);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            let pinch = ui.input(|i| i.zoom_delta());
            if scroll != 0.0 {
                self.scene.camera.dolly((-scroll * 0.002).exp());
            }
            if pinch != 1.0 {
                self.scene.camera.dolly(1.0 / pinch);
            }
        }
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            // Pointer to normalised device coordinates, y up.
            let ndc_x = (pointer.x - rect.left()) / rect.width() * 2.0 - 1.0;
            let ndc_y = 1.0 - (pointer.y - rect.top()) / rect.height() * 2.0;
            let (origin, direction) = self.scene.camera.ray(ndc_x, ndc_y, aspect);
            let hit = self.scene.pick(origin, direction);
            self.scene.select(hit);
        }
        if response.double_clicked() {
            // Frames the whole scene when nothing is selected.
            self.pending_frame = Some(FrameTarget::Selected);
        }
        if response.hovered()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
            && let Some(id) = self.scene.selected()
        {
            self.scene.remove_object(id);
        }
        if response.hovered() && ui.input(|i| i.key_pressed(egui::Key::V)) {
            self.scene.shading.value.toggle();
        }

        let painter = ui.painter_at(rect);
        self.scene.keep_camera_above_floor();
        let [r, g, b] = self.scene.room.backdrop_srgb();
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
        painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                frame: Frame::from_scene(&self.scene, aspect),
                size: physical_size(rect, ui.ctx().pixels_per_point()),
                anti_aliasing: self.anti_aliasing,
            },
        ));
        self.paint_light_icons(&painter, rect, aspect);
        if self.scene.object_count() == 0 {
            // An empty scene says how to begin.
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                tr.tr("studio-empty-viewport"),
                egui::FontId::proportional(16.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
}

/// The viewport's size in device pixels, as the GPU draws it.
pub(super) fn physical_size(rect: egui::Rect, pixels_per_point: f32) -> [u32; 2] {
    let size = rect.size() * pixels_per_point;
    [
        size.x.round().max(1.0) as u32,
        size.y.round().max(1.0) as u32,
    ]
}
