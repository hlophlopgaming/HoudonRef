use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use houdon_core::scene::ModelSource;
use houdon_core::{LoadError, Model, Scene};
use houdon_i18n::Localizer;

use super::{Builtin, Studio, extension, file_name};

const SCENE_FILE_NAME: &str = "scene.ron";

/// What a file is for. A file picked in a dialog has the kind of the
/// action that opened the dialog; a dropped file gets one by its extension.
#[derive(Clone, Copy)]
pub(super) enum FileKind {
    /// Dropped: a model, or an image for a matcap or a light mask.
    ModelOrImage,
    Model,
    Mask,
    Matcap,
    Scene,
    LightPreset,
}

impl FileKind {
    /// The extensions a file dialog offers for this kind.
    pub(super) fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::ModelOrImage => &["glb", "gltf", "png", "jpg", "jpeg", "exr"],
            Self::Model => &["glb", "gltf"],
            Self::Mask => &["png", "jpg", "jpeg"],
            Self::Matcap => &["exr", "png", "jpg", "jpeg"],
            Self::Scene | Self::LightPreset => &["ron"],
        }
    }
}

pub(super) struct FileResult {
    pub name: String,
    pub path: Option<PathBuf>,
    pub bytes: Result<Vec<u8>, String>,
    pub kind: FileKind,
}

#[derive(PartialEq, Eq)]
pub(super) struct ModelAsset {
    pub builtin: Option<Builtin>,
    path: Option<PathBuf>,
    bytes: Cow<'static, [u8]>,
}

impl ModelAsset {
    pub fn builtin(builtin: Builtin) -> Self {
        Self {
            builtin: Some(builtin),
            path: None,
            bytes: Cow::Borrowed(builtin.name_and_bytes().1),
        }
    }

    pub fn imported(path: Option<PathBuf>, bytes: Vec<u8>) -> Self {
        Self {
            builtin: None,
            path,
            bytes: Cow::Owned(bytes),
        }
    }

    pub fn load(&self, name: &str) -> Result<Model, LoadError> {
        Model::from_gltf_slice(name, &self.bytes)
    }

    fn source(&self, embed: bool) -> ModelSource {
        let builtin = self.builtin.map(|b| b.name_and_bytes().0.to_owned());
        if !embed {
            if let Some(path) = self.path.as_ref().filter(|path| path.to_str().is_some()) {
                return ModelSource::Path(path.clone());
            }
            if let Some(builtin) = self.builtin
                && self.bytes == builtin.name_and_bytes().1
            {
                return ModelSource::Builtin(builtin.name_and_bytes().0.to_owned());
            }
        }
        ModelSource::Embedded {
            builtin,
            bytes: Arc::from(self.bytes.as_ref()),
        }
    }

    fn resolve(source: ModelSource, scene_path: Option<&Path>) -> Result<Self, String> {
        let find_builtin = |name: &str| {
            Builtin::ALL
                .into_iter()
                .find(|b| b.name_and_bytes().0 == name)
        };
        match source {
            ModelSource::Builtin(name) => find_builtin(&name)
                .map(Self::builtin)
                .ok_or_else(|| format!("unknown built-in model: {name}")),
            ModelSource::Embedded { builtin, bytes } => Ok(Self {
                builtin: builtin.as_deref().and_then(find_builtin),
                bytes: Cow::Owned(bytes.to_vec()),
                path: None,
            }),
            ModelSource::Path(path) => {
                let directory = scene_path
                    .and_then(Path::parent)
                    .ok_or_else(|| path.display().to_string())?;
                let path = directory.join(path);
                if !matches!(extension(&file_name(&path)).as_str(), "glb" | "gltf")
                    || !path.is_file()
                {
                    return Err(path.display().to_string());
                }
                let bytes =
                    std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
                Ok(Self::imported(Some(path), bytes))
            }
        }
    }
}

impl Studio {
    pub(super) fn open_scene(
        &mut self,
        name: &str,
        path: Option<&Path>,
        bytes: &[u8],
        tr: &Localizer,
    ) -> bool {
        match Scene::from_ron(bytes, |name, source| {
            let asset = ModelAsset::resolve(source, path)?;
            let model = asset.load(name).map_err(|error| error.to_string())?;
            Ok((model, asset))
        }) {
            Ok(loaded) => {
                self.scene = loaded.scene;
                self.sources = loaded.models;
                self.pending_frame = None;
                self.selected_light = None;
                self.pending_mask = None;
                self.message = (!loaded.warnings.is_empty()).then(|| {
                    loaded
                        .warnings
                        .into_iter()
                        .map(|(name, reason)| {
                            log::warn!("skipped scene model {name}: {reason}");
                            tr.tr_args("scene-model-skipped", &[("name", name.into())])
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                });
                true
            }
            Err(error) => {
                self.message = Some(tr.tr_args(
                    "scene-open-error",
                    &[
                        ("name", name.to_owned().into()),
                        ("reason", error.to_string().into()),
                    ],
                ));
                false
            }
        }
    }

    pub(super) fn save_scene(&mut self, tr: &Localizer) {
        let result = self
            .scene
            .to_ron(|id| {
                self.sources
                    .get(&id)
                    .map(|asset| asset.source(self.embed_models))
            })
            .map_err(|error| error.to_string())
            .and_then(|document| {
                write_ron(
                    document.as_bytes(),
                    &tr.tr("scene-file-type"),
                    SCENE_FILE_NAME,
                )
            });
        if let Err(reason) = result {
            self.message = Some(tr.tr_args("scene-save-error", &[("reason", reason.into())]));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn pick_file(&mut self, ctx: &egui::Context, label: &str, kind: FileKind) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(label, kind.extensions())
            .pick_file()
        {
            self.incoming.lock().unwrap().push(FileResult {
                name: file_name(&path),
                bytes: std::fs::read(&path).map_err(|error| error.to_string()),
                path: Some(path),
                kind,
            });
            ctx.request_repaint();
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn pick_file(&mut self, ctx: &egui::Context, _label: &str, kind: FileKind) {
        let incoming = Arc::clone(&self.incoming);
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match web::pick(kind).await {
                Ok(Some(result)) => incoming.lock().unwrap().push(result),
                Ok(None) => {}
                Err(error) => incoming.lock().unwrap().push(FileResult {
                    name: default_file_name(kind).to_owned(),
                    bytes: Err(format!("{error:?}")),
                    path: None,
                    kind,
                }),
            }
            ctx.request_repaint();
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn default_file_name(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Scene => SCENE_FILE_NAME,
        FileKind::LightPreset => "preset.light.ron",
        _ => "file",
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn write_ron(bytes: &[u8], label: &str, file_name: &str) -> Result<(), String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter(label, &["ron"])
        .set_file_name(file_name)
        .save_file()
    else {
        return Ok(());
    };
    write_scene_to_path(bytes, path).map_err(|error| error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_scene_to_path(bytes: &[u8], mut path: PathBuf) -> std::io::Result<()> {
    use std::io::Write;

    let add_extension = extension(&file_name(&path)) != "ron";
    if add_extension {
        path.as_mut_os_string().push(".ron");
    }
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if add_extension {
        temporary.persist_noclobber(path)?;
    } else {
        temporary.persist(path)?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(super) fn write_ron(bytes: &[u8], _label: &str, file_name: &str) -> Result<(), String> {
    web::download(bytes, file_name).map_err(|error| format!("{error:?}"))
}

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{Blob, Document, HtmlAnchorElement, HtmlInputElement, Url};

    use super::{FileKind, FileResult};

    fn document() -> Result<Document, JsValue> {
        web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("browser document is unavailable"))
    }

    pub async fn pick(kind: FileKind) -> Result<Option<FileResult>, JsValue> {
        let document = document()?;
        let input: HtmlInputElement = document.create_element("input")?.dyn_into()?;
        input.set_type("file");
        let accept: Vec<String> = kind.extensions().iter().map(|e| format!(".{e}")).collect();
        input.set_accept(&accept.join(","));
        input.set_hidden(true);
        let body = document
            .body()
            .ok_or_else(|| JsValue::from_str("browser document has no body"))?;
        body.append_child(&input)?;
        let picked = js_sys::Promise::new(&mut |resolve, reject| {
            let listeners = input
                .add_event_listener_with_callback("change", &resolve)
                .and_then(|()| input.add_event_listener_with_callback("cancel", &resolve));
            match listeners {
                Ok(()) => input.click(),
                Err(error) => {
                    let _ = reject.call1(&JsValue::UNDEFINED, &error);
                }
            }
        })
        .await;
        input.remove();
        picked?;
        let Some(file) = input.files().and_then(|files| files.item(0)) else {
            return Ok(None);
        };
        let bytes = if file.size() > f64::from(u32::MAX) {
            Err("file is too large for the browser".to_owned())
        } else {
            file.array_buffer()
                .await
                .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
                .map_err(|error| format!("{error:?}"))
        };
        Ok(Some(FileResult {
            name: file.name(),
            path: None,
            bytes,
            kind,
        }))
    }

    pub fn download(bytes: &[u8], file_name: &str) -> Result<(), JsValue> {
        let document = document()?;
        let anchor: HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
        let body = document
            .body()
            .ok_or_else(|| JsValue::from_str("browser document has no body"))?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let blob = Blob::new_with_u8_array_sequence(&parts)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        anchor.set_href(&url);
        anchor.set_download(file_name);
        anchor.set_hidden(true);
        let clicked = body.append_child(&anchor).map(|_| {
            anchor.click();
            anchor.remove();
        });
        let revoked = Url::revoke_object_url(&url);
        clicked.and(revoked)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use houdon_i18n::Language;

    use super::*;

    #[test]
    fn desktop_scene_file_handles_path_and_embedded_models() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("imported.glb");
        let glb = Builtin::Female.name_and_bytes().1;
        std::fs::write(&path, glb).unwrap();
        let tr = Localizer::new(Language::English);
        let mut studio = Studio::new();
        studio.load_file("imported.glb", Some(path.clone()), glb.to_vec(), &tr);
        studio.scene.set_figure_height(2.1);
        let selected = studio.scene.selected().unwrap();
        assert_eq!(studio.scene.object_count(), 2);

        let save = |embed| {
            studio
                .scene
                .to_ron(|id| studio.sources.get(&id).map(|asset| asset.source(embed)))
                .unwrap()
        };
        let referenced = save(false);
        let embedded = save(true);
        assert!(referenced.contains("Path("));
        assert_eq!(embedded.matches("Embedded(").count(), 2);
        let scene_path = directory.path().join("study.ron");
        write_scene_to_path(referenced.as_bytes(), scene_path.clone()).unwrap();

        let mut restored = Studio::new();
        let document = std::fs::read(&scene_path).unwrap();
        assert!(restored.open_scene("study.ron", Some(&scene_path), &document, &tr));
        assert_eq!(restored.scene.object_count(), 2);
        assert_eq!(restored.scene.selected(), Some(selected));
        assert_eq!(restored.scene.figure_height(), 2.1);
        assert!(restored.message.is_none());
        assert!(!restored.open_scene("bad.ron", None, b"invalid", &tr));
        assert_eq!(restored.scene.object_count(), 2);

        std::fs::remove_file(path).unwrap();
        assert!(restored.open_scene("study.ron", Some(&scene_path), &document, &tr));
        assert_eq!(restored.scene.object_count(), 1);
        assert_eq!(restored.scene.selected(), None);
        assert!(
            restored
                .message
                .as_deref()
                .unwrap()
                .contains("imported.glb")
        );

        // The portable copy resolves from its contents, even without a file path.
        assert!(restored.open_scene("study.ron", None, embedded.as_bytes(), &tr));
        assert_eq!(restored.scene.object_count(), 2);
        assert_eq!(restored.scene.selected(), Some(selected));
        assert!(restored.message.is_none());

        let without_extension = directory.path().join("study");
        assert_eq!(
            write_scene_to_path(b"would overwrite", without_extension)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(scene_path).unwrap(), document);
    }
}
