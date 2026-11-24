//! Files reaching the Studio: dropped on the window, picked in a dialog or
//! arriving asynchronously on the web, and where each kind goes.

use super::*;

impl Studio {
    pub(super) fn load_file(
        &mut self,
        name: &str,
        path: Option<PathBuf>,
        bytes: Vec<u8>,
        tr: &Localizer,
    ) {
        // An image is a matcap while the matcap window is open (or an EXR,
        // which only matcaps use), otherwise the selected light's mask.
        match extension(name).as_str() {
            "exr" => return self.load_matcap_file(name, &bytes, tr),
            "png" | "jpg" | "jpeg" if self.matcap_picker_open => {
                return self.load_matcap_file(name, &bytes, tr);
            }
            "png" | "jpg" | "jpeg" => return self.load_mask_file(name, &bytes, tr),
            _ => {}
        }
        self.load_model_file(name, path, bytes, tr);
    }

    /// Adds an object of the model in a glTF or GLB file.
    pub(super) fn load_model_file(
        &mut self,
        name: &str,
        path: Option<PathBuf>,
        bytes: Vec<u8>,
        tr: &Localizer,
    ) {
        let asset = ModelAsset::imported(path, bytes);
        if let Err(error) = self.add_object_from(name, asset, None, Transform::default()) {
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
    pub fn poll_file_loads(&mut self, ctx: &egui::Context, tr: &Localizer) -> Option<f32> {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            let name = file_name(file.path());
            let kind = match extension(&name).as_str() {
                "ron" if name.to_ascii_lowercase().ends_with(".light.ron") => FileKind::LightPreset,
                "ron" if name.to_ascii_lowercase().ends_with(".camera.ron") => {
                    FileKind::CameraPreset
                }
                "ron" if name.to_ascii_lowercase().ends_with(".room.ron") => FileKind::RoomPreset,
                "ron" => FileKind::Scene,
                "glb" | "gltf" | "png" | "jpg" | "jpeg" | "exr" => FileKind::ModelOrImage,
                "blend" => {
                    self.message = Some(tr.tr("error-blend-file"));
                    continue;
                }
                _ => {
                    self.message =
                        Some(tr.tr_args("error-unsupported-file", &[("name", name.into())]));
                    continue;
                }
            };

            #[cfg(not(target_arch = "wasm32"))]
            self.incoming.lock().unwrap().push(FileResult {
                name,
                path: std::path::absolute(file.path()).ok(),
                bytes: file.bytes(),
                kind,
            });

            #[cfg(target_arch = "wasm32")]
            {
                let incoming = Arc::clone(&self.incoming);
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let bytes = file.bytes_async().await;
                    incoming.lock().unwrap().push(FileResult {
                        name,
                        path: None,
                        bytes,
                        kind,
                    });
                    ctx.request_repaint();
                });
            }
        }

        let ready: Vec<FileResult> = std::mem::take(&mut *self.incoming.lock().unwrap());
        let mut figure_height = None;
        for FileResult {
            name,
            path,
            bytes,
            kind,
        } in ready
        {
            match kind {
                FileKind::LightPreset => {
                    self.light_presets.import(bytes, tr);
                    continue;
                }
                FileKind::CameraPreset => {
                    self.camera_presets.import(bytes, tr);
                    continue;
                }
                FileKind::RoomPreset => {
                    self.room_presets.import(bytes, tr);
                    continue;
                }
                _ => {}
            }
            match bytes {
                Ok(bytes) if matches!(kind, FileKind::Scene) => {
                    if self.open_scene(&name, path.as_deref(), &bytes, tr) {
                        figure_height = Some(self.scene.figure_height());
                    }
                }
                Ok(bytes) => match kind {
                    FileKind::Model => self.load_model_file(&name, path, bytes, tr),
                    FileKind::Mask => self.load_mask_file(&name, &bytes, tr),
                    FileKind::Matcap => self.load_matcap_file(&name, &bytes, tr),
                    _ => self.load_file(&name, path, bytes, tr),
                },
                Err(error) => {
                    log::warn!("reading {name} failed: {error}");
                    self.message = Some(tr.tr_args("error-read-failed", &[("name", name.into())]));
                }
            }
        }
        self.sources.retain(|id, _| self.scene.model(*id).is_some());
        figure_height
    }
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

pub(super) fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}
