//! Studio section: arrange objects and light them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use houdon_core::{
    BuiltinRoom, LengthUnit, Light, LightId, LightKind, LoadError, MAX_LIGHTS, Model, ModelId,
    OrbitCamera, PLAUSIBLE_SIZE, Scene, UnitSystem,
};
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
    /// The light being edited in the side panel; editor state, not scene state.
    selected_light: Option<LightId>,
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
            selected_light: None,
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
        self.lights_section(ui, tr);
        ui.separator();
        self.room_section(ui, tr);
    }

    fn room_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-room"));
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
    }

    fn lights_section(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.strong(tr.tr("studio-lights"));
        let can_add = self.scene.lights().len() < MAX_LIGHTS;
        ui.horizontal_wrapped(|ui| {
            for kind in LightKind::ALL {
                let label = tr.tr_args(
                    "studio-add-light",
                    &[("kind", tr.tr(light_kind_message_id(kind)).into())],
                );
                if ui.add_enabled(can_add, egui::Button::new(label)).clicked() {
                    self.selected_light = self.scene.add_light(kind);
                }
            }
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

        if remove {
            self.scene.remove_light(light.id);
            self.selected_light = None;
        } else if light != before {
            self.scene.set_light(light);
        }
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

    /// Draws where each light is: a dot in its colour for point and spot
    /// lights, and for directional lights a dot far out in their direction
    /// with a line towards the scene centre. The selected light is ringed.
    fn paint_light_icons(&self, painter: &egui::Painter, rect: egui::Rect, aspect: f32) {
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

    /// Shows and edits the real size of a model not shown as a Figure, and
    /// warns when its file units look wrong.
    fn real_size_editor(&mut self, ui: &mut egui::Ui, tr: &Localizer, model: ModelId) {
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
        self.scene.keep_camera_above_floor();
        let [r, g, b] = self.scene.room.backdrop_srgb();
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
        painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                frame: Frame::from_scene(&self.scene, aspect),
            },
        ));
        self.paint_light_icons(&painter, rect, aspect);
    }
}

/// Message id of a built-in room's name.
fn builtin_room_message_id(room: BuiltinRoom) -> &'static str {
    match room {
        BuiltinRoom::Grey => "room-grey",
        BuiltinRoom::White => "room-white",
        BuiltinRoom::Dark => "room-dark",
    }
}

/// Message id of a light kind's name.
fn light_kind_message_id(kind: LightKind) -> &'static str {
    match kind {
        LightKind::Directional => "light-directional",
        LightKind::Point => "light-point",
        LightKind::Spot => "light-spot",
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
    tr.tr_args(
        "length",
        &[
            ("value", format!("{:.*}", decimals, length.value).into()),
            ("unit", tr.tr(unit_message_id(length.unit)).into()),
        ],
    )
}

/// A number field for a length kept in metres, shown and typed in the
/// editing unit of `units` (metres or feet). `range` is in metres.
/// Returns whether the artist changed it.
pub fn length_field(
    ui: &mut egui::Ui,
    tr: &Localizer,
    units: UnitSystem,
    metres: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let suffix = format!(" {}", tr.tr(unit_message_id(units.editing_unit())));
    let mut shown = units.to_editing(*metres);
    let changed = ui
        .add(
            egui::DragValue::new(&mut shown)
                .speed(0.01)
                .range(units.to_editing(*range.start())..=units.to_editing(*range.end()))
                .suffix(suffix)
                .max_decimals(3),
        )
        .changed();
    if changed {
        *metres = units.from_editing(shown);
    }
    changed
}

/// An angle in degrees brought into -180..180.
fn wrap_degrees(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
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

#[cfg(test)]
mod tests {
    use super::*;
    use houdon_i18n::Language;

    #[test]
    fn lengths_read_in_the_chosen_units_and_language() {
        let en = Localizer::new(Language::English);
        assert_eq!(format_length(&en, UnitSystem::Metric, 1.75), "1.75 m");
        assert_eq!(format_length(&en, UnitSystem::Metric, 0.2), "20.0 cm");
        assert_eq!(format_length(&en, UnitSystem::Imperial, 1.8288), "6.00 ft");
        assert_eq!(format_length(&en, UnitSystem::Imperial, 0.0254), "1.0 in");
        let ru = Localizer::new(Language::Russian);
        assert_eq!(format_length(&ru, UnitSystem::Metric, 0.2), "20.0 см");
    }
}
