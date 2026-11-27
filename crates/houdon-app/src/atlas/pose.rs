use glam::{Quat, Vec3};
use houdon_core::{AtlasView, Side, atlas::PoseJoint};
use houdon_i18n::Localizer;

use super::view_projection;

#[derive(Default)]
pub(super) struct PoseControls {
    open: bool,
    joint: usize,
    angle_dragging: bool,
}

impl PoseControls {
    /// Returns whether the geometry changed.
    pub fn panel(&mut self, ui: &mut egui::Ui, tr: &Localizer, view: &mut AtlasView) -> bool {
        let mut changed = false;
        self.angle_dragging = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.open, tr.tr("atlas-pose"))
                .clicked()
            {
                self.open = !self.open && view.enable_pose();
            }
            if !view.pose_joints().is_empty() && ui.button(tr.tr("atlas-pose-reset")).clicked() {
                view.reset_pose();
                changed = true;
            }
        });
        if !self.open {
            return changed;
        }

        ui.weak(tr.tr("atlas-pose-hint"));
        let joints = view.pose_joints();
        egui::ComboBox::from_id_salt("atlas-pose-joint")
            .selected_text(joint_name(tr, &joints[self.joint]))
            .show_ui(ui, |ui| {
                for (i, joint) in joints.iter().enumerate() {
                    ui.selectable_value(&mut self.joint, i, joint_name(tr, joint));
                }
            });
        let mut angles = joints[self.joint].angles.map(f32::to_degrees);
        let (min, max) = joints[self.joint].degree_limits();
        let before = angles;
        ui.horizontal_wrapped(|ui| {
            let axes = [
                ("X ", &mut angles.x),
                ("Y ", &mut angles.y),
                ("Z ", &mut angles.z),
            ];
            for (i, (label, angle)) in axes.into_iter().enumerate() {
                let response = ui.add(
                    egui::DragValue::new(angle)
                        .prefix(label)
                        .suffix("°")
                        .speed(0.5)
                        .range(min[i]..=max[i]),
                );
                self.angle_dragging |= response.dragged();
            }
        });
        if angles != before {
            view.set_joint_angles(self.joint, angles.map(f32::to_radians));
            changed = true;
        }
        changed
    }

    /// Reserve pointer drags on the handles before orbiting the camera.
    pub fn interact(
        &mut self,
        ui: &egui::Ui,
        tr: &Localizer,
        view: &mut AtlasView,
        rect: egui::Rect,
        aspect: f32,
    ) -> (bool, bool) {
        if !self.open {
            return (false, false);
        }
        let mut captured = false;
        let mut motion = None;
        for (index, joint) in view.pose_joints().iter().enumerate() {
            if !self.show_handle(view, joint) {
                continue;
            }
            let Some(pos) = view_projection(view, aspect, rect, joint.position()) else {
                continue;
            };
            if !rect.contains(pos) {
                continue;
            }
            let response = ui
                .interact(
                    egui::Rect::from_center_size(
                        pos,
                        egui::Vec2::splat(if joint.digit.is_some() { 12.0 } else { 22.0 }),
                    ),
                    ui.scope_id().with(("pose-handle", index)),
                    egui::Sense::click_and_drag(),
                )
                .on_hover_text(joint_name(tr, joint));
            if response.clicked() || response.dragged() || response.drag_stopped() {
                self.joint = index;
                captured = true;
            }
            if response.dragged() && response.drag_delta() != egui::Vec2::ZERO {
                motion = Some(response.drag_delta());
            }
        }
        if let Some(delta) = motion {
            let camera = view.scene.camera.view();
            let right: Vec3 = camera.row(0).truncate();
            let up: Vec3 = camera.row(1).truncate();
            let normal: Vec3 = camera.row(2).truncate();
            let scale = std::f32::consts::PI / rect.height().max(1.0);
            let rotation = if ui.input(|i| i.modifiers.shift) {
                Quat::from_axis_angle(normal, -delta.x * scale)
            } else {
                Quat::from_axis_angle(up, delta.x * scale)
                    * Quat::from_axis_angle(right, delta.y * scale)
            };
            view.rotate_joint(self.joint, rotation);
        }
        (captured, motion.is_some())
    }

    pub fn previewing(&self, joint_dragging: bool) -> bool {
        self.open && (joint_dragging || self.angle_dragging)
    }

    pub fn paint(&self, painter: &egui::Painter, view: &AtlasView, rect: egui::Rect, aspect: f32) {
        if !self.open {
            return;
        }
        for (index, joint) in view.pose_joints().iter().enumerate() {
            if !self.show_handle(view, joint) {
                continue;
            }
            let Some(pos) = view_projection(view, aspect, rect, joint.position()) else {
                continue;
            };
            let active = index == self.joint;
            painter.circle(
                pos,
                if active {
                    8.0
                } else if joint.digit.is_some() {
                    3.0
                } else {
                    5.0
                },
                if active {
                    egui::Color32::from_rgb(247, 179, 70)
                } else {
                    egui::Color32::from_rgb(250, 249, 242)
                },
                egui::Stroke::new(1.5, egui::Color32::from_rgb(36, 40, 48)),
            );
        }
    }

    fn show_handle(&self, view: &AtlasView, joint: &PoseJoint) -> bool {
        joint.digit.is_none_or(|(root, _)| {
            view.scene.camera.distance < 1.0
                && (self.joint == root
                    || view.pose_joints()[self.joint]
                        .digit
                        .is_some_and(|(selected, _)| selected == root))
        })
    }
}

fn joint_name(tr: &Localizer, joint: &PoseJoint) -> String {
    let mut name = tr.tr(&format!("atlas-joint-{}", joint.name));
    if let Some((_, segment)) = joint.digit {
        name = tr.tr_args(
            "atlas-joint-digit",
            &[
                ("name", name.into()),
                ("segment", u32::from(segment).into()),
            ],
        );
    }
    match joint.side {
        Side::Left => tr.tr_args("atlas-left", &[("name", name.into())]),
        Side::Right => tr.tr_args("atlas-right", &[("name", name.into())]),
        Side::Middle => name,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::atlas::{ATLAS_MANIFEST, build, carried_file};

    #[test]
    fn dragging_a_viewport_joint_rotates_it_without_orbiting() {
        let manifest = carried_file(ATLAS_MANIFEST).unwrap();
        let mut view = AtlasView::new(build(carried_file, &manifest).unwrap());
        assert!(view.enable_pose());
        let elbow = view
            .pose_joints()
            .iter()
            .position(|j| j.name == "elbow" && j.side == Side::Left)
            .unwrap();
        let mut controls = PoseControls {
            open: true,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let tr = Localizer::new(houdon_i18n::Language::English);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let camera = view.scene.camera;
        let mut frame = |events| {
            let mut results = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::no_frame().show(ui, |ui| {
                        let (rect, response) = ui.allocate_exact_size(
                            ui.available_size(),
                            egui::Sense::click_and_drag(),
                        );
                        let aspect = rect.aspect_ratio();
                        let marker = view_projection(
                            &view,
                            aspect,
                            rect,
                            view.pose_joints()[elbow].position(),
                        )
                        .unwrap();
                        let (captured, moved) = controls.interact(ui, &tr, &mut view, rect, aspect);
                        if !captured {
                            crate::studio::drive_camera(
                                &mut view.scene.camera,
                                ui,
                                &response,
                                rect,
                            );
                        }
                        results = Some((marker, captured, moved));
                    });
                },
            );
            output.textures_delta.clear();
            results.unwrap()
        };

        let (start, _, _) = frame(Vec::new());
        frame(vec![egui::Event::PointerMoved(start)]);
        frame(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let (_, captured, moved) = frame(vec![egui::Event::PointerMoved(
            start + egui::vec2(18.0, 46.0),
        )]);
        assert!(captured && moved);
        assert_eq!(controls.joint, elbow);
        assert!(view.pose_joints()[elbow].angles.length() > 0.1);
        assert_eq!(view.scene.camera, camera);
    }
}
