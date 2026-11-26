//! Files reaching the Studio: dropped on the window, picked in a dialog or
//! arriving asynchronously on the web, and where each kind goes.

use super::*;

impl Studio {
    pub(super) fn load_file(
        &mut self,
        name: &str,
        path: Option<PathBuf>,
        bytes: Vec<u8>,
        companions: &[Companion],
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
        self.load_model_file(name, path, bytes, companions, tr);
    }

    /// Adds an object of the model in a glTF, GLB or OBJ file. An OBJ's
    /// MTL and textures come from `companions` or from beside `path`.
    pub(super) fn load_model_file(
        &mut self,
        name: &str,
        path: Option<PathBuf>,
        bytes: Vec<u8>,
        companions: &[Companion],
        tr: &Localizer,
    ) {
        let (asset, missing) = ModelAsset::imported(path, bytes, companions);
        match self.add_object_from(name, asset, None, Transform::default()) {
            Ok(_) if !missing.is_empty() => {
                self.message = Some(tr.tr_args(
                    "obj-files-missing",
                    &[
                        ("name", name.to_owned().into()),
                        ("files", missing.join(", ").into()),
                    ],
                ));
            }
            Ok(_) => {}
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
    pub fn poll_file_loads(&mut self, ctx: &egui::Context, tr: &Localizer) -> Option<f32> {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        // An OBJ dropped with its MTL and textures takes them along.
        let with_obj = dropped
            .iter()
            .any(|file| extension(&file_name(file.path())) == "obj");
        let mut companions = Vec::new();
        let mut drops = Vec::new();
        for file in dropped {
            let name = file_name(file.path());
            let ext = extension(&name);
            if with_obj && COMPANION_EXTENSIONS.contains(&ext.as_str()) {
                companions.push(file);
                continue;
            }
            let kind = match ext.as_str() {
                "ron" if name.to_ascii_lowercase().ends_with(".light.ron") => FileKind::LightPreset,
                "ron" if name.to_ascii_lowercase().ends_with(".camera.ron") => {
                    FileKind::CameraPreset
                }
                "ron" if name.to_ascii_lowercase().ends_with(".room.ron") => FileKind::RoomPreset,
                "ron" => FileKind::Scene,
                "glb" | "gltf" | "obj" | "png" | "jpg" | "jpeg" | "exr" => FileKind::ModelOrImage,
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
            drops.push((name, file, kind));
        }
        self.read_drops(ctx, drops, companions);

        let ready: Vec<FileResult> = std::mem::take(&mut *self.incoming.lock().unwrap());
        let mut figure_height = None;
        for FileResult {
            name,
            path,
            bytes,
            kind,
            companions,
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
                    FileKind::Model => self.load_model_file(&name, path, bytes, &companions, tr),
                    FileKind::Mask => self.load_mask_file(&name, &bytes, tr),
                    FileKind::Matcap => self.load_matcap_file(&name, &bytes, tr),
                    _ => self.load_file(&name, path, bytes, &companions, tr),
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

impl Studio {
    /// Reads the files of one drop. On the desktop they are on disk, and an
    /// OBJ finds its MTL and textures there; in the browser their bytes are
    /// read together, so an OBJ gets the ones dropped with it.
    fn read_drops(
        &mut self,
        ctx: &egui::Context,
        drops: Vec<(String, egui::DroppedFileHandle, FileKind)>,
        companions: Vec<egui::DroppedFileHandle>,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (ctx, companions);
            let mut incoming = self.incoming.lock().unwrap();
            for (name, file, kind) in drops {
                incoming.push(FileResult {
                    name,
                    path: std::path::absolute(file.path()).ok(),
                    bytes: file.bytes(),
                    kind,
                    companions: Vec::new(),
                });
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if drops.is_empty() {
                return;
            }
            let incoming = Arc::clone(&self.incoming);
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut read = Vec::new();
                for file in companions {
                    if let Ok(bytes) = file.bytes_async().await {
                        read.push((file_name(file.path()), bytes));
                    }
                }
                for (name, file, kind) in drops {
                    let bytes = file.bytes_async().await;
                    let companions = if extension(&name) == "obj" {
                        read.clone()
                    } else {
                        Vec::new()
                    };
                    incoming.lock().unwrap().push(FileResult {
                        name,
                        path: None,
                        bytes,
                        kind,
                        companions,
                    });
                }
                ctx.request_repaint();
            });
        }
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
