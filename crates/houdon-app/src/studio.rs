//! Studio section: arrange objects and light them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use houdon_core::{LengthUnit, LoadError, Model, ModelId, OrbitCamera, Scene, UnitSystem};
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
    const ALL: [Self; 2] = [Self::Male, Self::Female];

    fn name_and_bytes(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Male => ("male", MALE_GLB),
            Self::Female => ("female", FEMALE_GLB),
        }
    }

    /// Where the model came from, for [`Studio::sources`].
    fn source(self) -> String {
        format!("builtin:{}", self.name_and_bytes().0)
    }

    /// Translated name shown in the objects list.
    fn message_id(self) -> &'static str {
        match self {
            Self::Male => "model-male-figure",
            Self::Female => "model-female-figure",
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
    /// How lengths are shown; an app setting, not part of the scene.
    units: UnitSystem,
}

impl Studio {
    pub fn new() -> Self {
        let mut studio = Self {
            scene: Scene::new(),
            sources: HashMap::new(),
            pending_frame: Some(FrameTarget::Scene),
            message: None,
            incoming: Arc::default(),
            units: UnitSystem::default(),
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
                let id = self.scene.add_model(load()?);
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
        if let Err(error) =
            self.add_object_from(builtin.source(), || Model::from_gltf_slice(name, bytes))
        {
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

    /// Applies the app settings that affect the Studio.
    pub fn apply_settings(&mut self, units: UnitSystem, figure_height: f32) {
        self.units = units;
        self.scene.set_figure_height(figure_height);
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
        .suffix(format!(" {}", tr.tr("unit-millimetre-short")))
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

    /// Name of a model for the interface: translated for built-in models,
    /// the file name for loaded ones.
    fn model_display_name(&self, id: ModelId, tr: &Localizer) -> String {
        let builtin = Builtin::ALL
            .into_iter()
            .find(|b| self.sources.get(&b.source()) == Some(&id));
        match builtin {
            Some(builtin) => tr.tr(builtin.message_id()),
            None => self
                .scene
                .model(id)
                .map(|m| m.name.clone())
                .unwrap_or_default(),
        }
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

    fn selected_object_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
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
        let unit = format!(" {}", tr.tr(unit_message_id(self.units.editing_unit())));
        let units = self.units;
        ui.horizontal(|ui| {
            for (axis, value) in [
                ("X", &mut transform.translation.x),
                ("Y", &mut transform.translation.y),
                ("Z", &mut transform.translation.z),
            ] {
                ui.label(axis);
                let mut shown = units.to_editing(*value);
                let edited = ui
                    .add(
                        egui::DragValue::new(&mut shown)
                            .speed(0.01)
                            .suffix(unit.as_str())
                            .max_decimals(3),
                    )
                    .changed();
                if edited {
                    *value = units.from_editing(shown);
                    changed = true;
                }
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
    fn real_size_editor(&mut self, ui: &mut egui::Ui, tr: &Localizer, model: ModelId) {
        let Some(real_size) = self.scene.real_size(model) else {
            return;
        };
        if self.scene.needs_real_size(model) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr.tr("studio-real-size-warning"),
            );
        }
        let units = self.units;
        let unit = format!(" {}", tr.tr(unit_message_id(units.editing_unit())));
        let mut shown = units.to_editing(real_size);
        ui.horizontal(|ui| {
            let edited = ui
                .add(
                    egui::DragValue::new(&mut shown)
                        .speed(0.01)
                        .range(0.001..=10_000.0)
                        .suffix(unit.as_str())
                        .max_decimals(3),
                )
                .changed();
            ui.label(tr.tr("studio-real-size"));
            if edited {
                self.scene.set_real_size(model, units.from_editing(shown));
            }
        });
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
            // Frames the whole scene when nothing is selected.
            self.pending_frame = Some(FrameTarget::Selected);
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

/// Message id of a unit's short name.
pub fn unit_message_id(unit: LengthUnit) -> &'static str {
    match unit {
        LengthUnit::Centimetre => "unit-centimetre-short",
        LengthUnit::Metre => "unit-metre-short",
        LengthUnit::Inch => "unit-inch-short",
        LengthUnit::Foot => "unit-foot-short",
    }
}

/// A length in the most readable unit of `units`, such as "1.75 m" or "20 cm".
fn format_length(tr: &Localizer, units: UnitSystem, metres: f32) -> String {
    let length = units.readable(metres);
    let decimals = match length.unit {
        LengthUnit::Metre | LengthUnit::Foot => 2,
        LengthUnit::Centimetre | LengthUnit::Inch => 1,
    };
    format!(
        "{:.*} {}",
        decimals,
        length.value,
        tr.tr(unit_message_id(length.unit))
    )
}

/// Snaps values that are zero up to rounding to exactly 0, so they do not
/// display as "-0".
fn snap_to_zero(value: f32) -> f32 {
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
