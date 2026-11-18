//! Studio section: arrange objects and light them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use glam::{EulerRot, Quat};
use houdon_core::{LoadError, Model, ModelId, OrbitCamera, Scene, model::DEFAULT_FIGURE_HEIGHT};
use houdon_i18n::Localizer;
use houdon_render::{Frame, ViewportCallback};

const MALE_GLB: &[u8] = include_bytes!("../../../assets/models/test/male.glb");
const FEMALE_GLB: &[u8] = include_bytes!("../../../assets/models/test/female.glb");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Builtin {
    Male,
    Female,
}

impl Builtin {
    fn name_and_bytes(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Male => ("male", MALE_GLB),
            Self::Female => ("female", FEMALE_GLB),
        }
    }
}

/// What to point the camera at on the next drawn frame, once the viewport
/// size is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameTarget {
    Scene,
    Selected,
}

/// A file read from disk or the browser, waiting to be parsed.
type FileResult = (String, Result<Vec<u8>, String>);

pub struct Studio {
    scene: Scene,
    /// Models already in the scene by where they came from, so adding the
    /// same model again places another object of it instead of loading it twice.
    sources: HashMap<String, ModelId>,
    pending_frame: Option<FrameTarget>,
    message: Option<String>,
    /// Files whose bytes arrive asynchronously (always on the web).
    incoming: Arc<Mutex<Vec<FileResult>>>,
}

impl Studio {
    pub fn new() -> Self {
        let mut studio = Self {
            scene: Scene::new(),
            sources: HashMap::new(),
            pending_frame: Some(FrameTarget::Scene),
            message: None,
            incoming: Arc::default(),
        };
        studio.add_builtin(Builtin::Male);
        studio.scene.select(None);
        studio
    }

    /// Places a new object of the model from `source`, loading the model
    /// only if the scene does not have it yet, and selects the object.
    fn add_object_from(
        &mut self,
        source: String,
        load: impl FnOnce() -> Result<Model, LoadError>,
    ) -> Result<(), LoadError> {
        let known = self
            .sources
            .get(&source)
            .copied()
            .filter(|id| self.scene.model(*id).is_some());
        let model_id = match known {
            Some(id) => id,
            None => {
                let mut model = load()?;
                model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
                let id = self.scene.add_model(model);
                self.sources.insert(source, id);
                id
            }
        };
        let object = self.scene.add_object(model_id);
        self.scene.select(object);
        self.pending_frame = Some(FrameTarget::Scene);
        self.message = None;
        Ok(())
    }

    fn add_builtin(&mut self, builtin: Builtin) {
        let (name, bytes) = builtin.name_and_bytes();
        if let Err(error) = self.add_object_from(format!("builtin:{name}"), || {
            Model::from_gltf_slice(name, bytes)
        }) {
            log::error!("built-in model {name} failed to load: {error}");
        }
    }

    fn load_file(&mut self, name: &str, bytes: &[u8], tr: &Localizer) {
        let source = format!("file:{name}:{}", bytes.len());
        if let Err(error) = self.add_object_from(source, || Model::from_gltf_slice(name, bytes)) {
            self.message = Some(tr.tr_args(
                "error-load-failed",
                &[
                    ("name", name.to_owned().into()),
                    ("reason", error.to_string().into()),
                ],
            ));
        }
    }

    /// Takes files dropped onto the window and files whose bytes have arrived.
    pub fn poll_file_loads(&mut self, ctx: &egui::Context, tr: &Localizer) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            let name = file_name(file.path());
            match extension(&name).as_str() {
                "glb" | "gltf" => {}
                "blend" => {
                    self.message = Some(tr.tr("error-blend-file"));
                    continue;
                }
                _ => {
                    self.message =
                        Some(tr.tr_args("error-unsupported-file", &[("name", name.into())]));
                    continue;
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            self.incoming.lock().unwrap().push((name, file.bytes()));

            #[cfg(target_arch = "wasm32")]
            {
                let incoming = Arc::clone(&self.incoming);
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let bytes = file.bytes_async().await;
                    incoming.lock().unwrap().push((name, bytes));
                    ctx.request_repaint();
                });
            }
        }

        let ready: Vec<FileResult> = std::mem::take(&mut *self.incoming.lock().unwrap());
        for (name, bytes) in ready {
            match bytes {
                Ok(bytes) => self.load_file(&name, &bytes, tr),
                Err(error) => {
                    log::warn!("reading {name} failed: {error}");
                    self.message = Some(tr.tr_args("error-read-failed", &[("name", name.into())]));
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        egui::Panel::left("studio_panel")
            .resizable(true)
            .default_size(260.0)
            .min_size(200.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.side_panel(ui, tr));
            });

        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport(ui, tr));
    }

    fn side_panel(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.add_space(6.0);
        self.objects_section(ui, tr);
        ui.separator();
        self.selected_object_section(ui, tr);

        ui.separator();
        ui.strong(tr.tr("studio-camera"));
        let mut focal = self.scene.camera.focal_length_mm;
        let slider = egui::Slider::new(
            &mut focal,
            OrbitCamera::MIN_FOCAL_LENGTH_MM..=OrbitCamera::MAX_FOCAL_LENGTH_MM,
        )
        .logarithmic(true)
        .suffix(" mm")
        .text(tr.tr("studio-focal-length"));
        if ui.add(slider).changed() {
            self.scene.camera.set_focal_length_keep_framing(focal);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr.tr("studio-frame-scene")).clicked() {
                self.pending_frame = Some(FrameTarget::Scene);
            }
            let has_selection = self.scene.selected().is_some();
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new(tr.tr("studio-frame-selected")),
                )
                .clicked()
            {
                self.pending_frame = Some(FrameTarget::Selected);
            }
        });
        ui.weak(tr.tr("studio-camera-hint"));

        ui.separator();
        ui.strong(tr.tr("studio-light"));
        let lighting = &mut self.scene.lighting;
        let mut azimuth = lighting.key_azimuth.to_degrees();
        let mut elevation = lighting.key_elevation.to_degrees();
        ui.add(
            egui::Slider::new(&mut azimuth, -180.0..=180.0)
                .suffix("°")
                .text(tr.tr("studio-light-azimuth")),
        );
        ui.add(
            egui::Slider::new(&mut elevation, -60.0..=90.0)
                .suffix("°")
                .text(tr.tr("studio-light-elevation")),
        );
        lighting.key_azimuth = azimuth.to_radians();
        lighting.key_elevation = elevation.to_radians();
        ui.add(
            egui::Slider::new(&mut lighting.key_intensity, 0.0..=3.0)
                .text(tr.tr("studio-light-intensity")),
        );
        ui.add(
            egui::Slider::new(&mut lighting.ambient_intensity, 0.0..=1.0)
                .text(tr.tr("studio-ambient")),
        );
        ui.horizontal(|ui| {
            ui.color_edit_button_srgb(&mut self.scene.background);
            ui.label(tr.tr("studio-background"));
        });
    }

    fn objects_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-objects"));
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr.tr("studio-add-male")).clicked() {
                self.add_builtin(Builtin::Male);
            }
            if ui.button(tr.tr("studio-add-female")).clicked() {
                self.add_builtin(Builtin::Female);
            }
        });
        ui.label(tr.tr("studio-drop-hint"));
        if let Some(message) = &self.message {
            ui.colored_label(ui.visuals().warn_fg_color, message);
        }

        // Number objects of the same model: "male 1", "male 2", …
        let mut seen: HashMap<ModelId, usize> = HashMap::new();
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("objects_list")
            .max_height(160.0)
            .show(ui, |ui| {
                for object in self.scene.objects() {
                    let n = seen.entry(object.model).or_default();
                    *n += 1;
                    let name = self
                        .scene
                        .model(object.model)
                        .map_or("?", |m| m.name.as_str());
                    let selected = self.scene.selected() == Some(object.id);
                    if ui
                        .selectable_label(selected, format!("{name} {n}"))
                        .clicked()
                    {
                        clicked = Some(object.id);
                    }
                }
            });
        if let Some(id) = clicked {
            self.scene.select(Some(id));
        }

        ui.horizontal_wrapped(|ui| {
            let selected = self.scene.selected();
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(tr.tr("studio-object-duplicate")),
                )
                .clicked()
                && let Some(id) = selected
            {
                self.scene.duplicate_object(id);
            }
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(tr.tr("studio-object-delete")),
                )
                .clicked()
                && let Some(id) = selected
            {
                self.scene.remove_object(id);
            }
        });
    }

    fn selected_object_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-selected-object"));
        let Some(id) = self.scene.selected() else {
            ui.weak(tr.tr("studio-no-selection"));
            return;
        };
        let Some(object) = self.scene.object(id) else {
            return;
        };
        if let Some(model) = self.scene.model(object.model) {
            ui.weak(tr.tr_args(
                "model-stats",
                &[("triangles", model.triangle_count().into())],
            ));
        }
        let mut transform = object.transform;
        // Show tiny rounding leftovers as 0 instead of "-0.00".
        transform.translation = transform.translation.map(tidy);
        let mut changed = false;

        ui.label(tr.tr("studio-position"));
        ui.horizontal(|ui| {
            for (axis, value) in [
                ("X", &mut transform.translation.x),
                ("Y", &mut transform.translation.y),
                ("Z", &mut transform.translation.z),
            ] {
                ui.label(axis);
                changed |= ui
                    .add(
                        egui::DragValue::new(value)
                            .speed(0.01)
                            .suffix(" m")
                            .max_decimals(3),
                    )
                    .changed();
            }
        });

        ui.label(tr.tr("studio-rotation"));
        let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);
        let mut angles = [yaw, pitch, roll].map(|a| tidy(a.to_degrees()));
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
            transform.rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
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

    fn viewport(&mut self, ui: &mut egui::Ui, _tr: &Localizer) {
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
            self.pending_frame = Some(if self.scene.selected().is_some() {
                FrameTarget::Selected
            } else {
                FrameTarget::Scene
            });
        }
        if response.hovered()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
            && let Some(id) = self.scene.selected()
        {
            self.scene.remove_object(id);
        }

        let painter = ui.painter_at(rect);
        let [r, g, b] = self.scene.background;
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
        painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                frame: Frame::from_scene(&self.scene, aspect),
            },
        ));
    }
}

/// Snaps values that are zero up to rounding to exactly 0, so they do not
/// display as "-0".
fn tidy(value: f32) -> f32 {
    if value.abs() < 1e-4 { 0.0 } else { value }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}
