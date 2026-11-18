//! Studio section: view a model under adjustable light.

use std::path::Path;
use std::sync::{Arc, Mutex};

use houdon_core::{Model, OrbitCamera, Scene, model::DEFAULT_FIGURE_HEIGHT};
use houdon_i18n::Localizer;
use houdon_render::{Frame, ViewportCallback};

const MALE_GLB: &[u8] = include_bytes!("../../../assets/models/test/male.glb");
const FEMALE_GLB: &[u8] = include_bytes!("../../../assets/models/test/female.glb");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelSource {
    BuiltinMale,
    BuiltinFemale,
    File,
}

/// A file read from disk or the browser, waiting to be parsed.
type FileResult = (String, Result<Vec<u8>, String>);

pub struct Studio {
    scene: Scene,
    source: ModelSource,
    /// Frame the model on the next drawn frame, once the viewport size is known.
    frame_pending: bool,
    message: Option<String>,
    /// Files whose bytes arrive asynchronously (always on the web).
    incoming: Arc<Mutex<Vec<FileResult>>>,
}

impl Studio {
    pub fn new() -> Self {
        let mut studio = Self {
            scene: Scene::new(),
            source: ModelSource::BuiltinMale,
            frame_pending: true,
            message: None,
            incoming: Arc::default(),
        };
        studio.load_builtin(ModelSource::BuiltinMale);
        studio
    }

    /// Replaces the scene's content with one object of `model`.
    fn set_model(&mut self, mut model: Model) {
        model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
        self.scene.clear();
        let id = self.scene.add_model(model);
        self.scene.add_object(id);
        self.frame_pending = true;
    }

    /// The model shown, while the Studio holds a single object.
    fn current_model(&self) -> Option<&Arc<Model>> {
        let object = self.scene.objects().first()?;
        self.scene.model(object.model)
    }

    fn load_builtin(&mut self, source: ModelSource) {
        let (name, bytes) = match source {
            ModelSource::BuiltinMale => ("male", MALE_GLB),
            ModelSource::BuiltinFemale => ("female", FEMALE_GLB),
            ModelSource::File => return,
        };
        match Model::from_gltf_slice(name, bytes) {
            Ok(model) => {
                self.source = source;
                self.message = None;
                self.set_model(model);
            }
            Err(error) => log::error!("built-in model {name} failed to load: {error}"),
        }
    }

    fn load_file(&mut self, name: &str, bytes: &[u8], tr: &Localizer) {
        match Model::from_gltf_slice(name, bytes) {
            Ok(model) => {
                self.source = ModelSource::File;
                self.message = None;
                self.set_model(model);
            }
            Err(error) => {
                self.message = Some(tr.tr_args(
                    "error-load-failed",
                    &[
                        ("name", name.to_owned().into()),
                        ("reason", error.to_string().into()),
                    ],
                ));
            }
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
        ui.strong(tr.tr("studio-model"));
        let mut source = self.source;
        ui.radio_value(
            &mut source,
            ModelSource::BuiltinMale,
            tr.tr("studio-builtin-male"),
        );
        ui.radio_value(
            &mut source,
            ModelSource::BuiltinFemale,
            tr.tr("studio-builtin-female"),
        );
        if self.source == ModelSource::File
            && let Some(model) = self.current_model()
        {
            ui.radio_value(&mut source, ModelSource::File, model.name.as_str());
        }
        if source != self.source {
            self.load_builtin(source);
        }
        if let Some(model) = self.current_model() {
            ui.weak(tr.tr_args(
                "model-stats",
                &[("triangles", model.triangle_count().into())],
            ));
        }
        ui.label(tr.tr("studio-drop-hint"));
        if let Some(message) = &self.message {
            ui.colored_label(ui.visuals().warn_fg_color, message);
        }

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
        if ui.button(tr.tr("studio-frame")).clicked() {
            self.frame_pending = true;
        }
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

    fn viewport(&mut self, ui: &mut egui::Ui, _tr: &Localizer) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let aspect = rect.width() / rect.height().max(1.0);

        if self.frame_pending && self.scene.object_count() > 0 {
            self.scene.frame_camera(aspect);
            self.frame_pending = false;
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
        if response.double_clicked() {
            self.frame_pending = true;
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
