use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use houdon_core::scene::{EmbeddedFile, ModelSource};
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
    CameraPreset,
    RoomPreset,
}

impl FileKind {
    /// The extensions a file dialog offers for this kind.
    pub(super) fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::ModelOrImage => &["glb", "gltf", "obj", "png", "jpg", "jpeg", "exr"],
            Self::Model => MODEL_EXTENSIONS,
            Self::Mask => &["png", "jpg", "jpeg"],
            Self::Matcap => &["exr", "png", "jpg", "jpeg"],
            Self::Scene | Self::LightPreset | Self::CameraPreset | Self::RoomPreset => &["ron"],
        }
    }
}

pub(super) struct FileResult {
    pub name: String,
    pub path: Option<PathBuf>,
    pub bytes: Result<Vec<u8>, String>,
    pub kind: FileKind,
    /// Files that came with a model: an OBJ's MTL and textures, picked or
    /// dropped together with it in the browser.
    pub companions: Vec<Companion>,
}

/// A model's file, with the files it reads besides itself (an OBJ's MTL
/// and textures) by the name the model gives them.
#[derive(PartialEq, Eq)]
pub(super) struct ModelAsset {
    pub builtin: Option<Builtin>,
    path: Option<PathBuf>,
    bytes: Cow<'static, [u8]>,
    files: Vec<(String, Arc<[u8]>)>,
}

/// A file that came with a model file: picked or dropped together with it.
pub(super) type Companion = (String, Vec<u8>);

impl ModelAsset {
    pub fn builtin(builtin: Builtin) -> Self {
        Self {
            builtin: Some(builtin),
            path: None,
            bytes: Cow::Borrowed(builtin.name_and_bytes().1),
            files: Vec::new(),
        }
    }

    /// An artist's model file. An OBJ's MTL and texture files are taken
    /// from `companions`, else from beside `path`; returns the names of
    /// those that were not found.
    pub fn imported(
        path: Option<PathBuf>,
        bytes: Vec<u8>,
        companions: &[Companion],
    ) -> (Self, Vec<String>) {
        let mut asset = Self {
            builtin: None,
            path,
            bytes: Cow::Owned(bytes),
            files: Vec::new(),
        };
        if is_gltf(&asset.bytes) {
            return (asset, Vec::new());
        }
        let found = Model::from_obj("", &asset.bytes, |reference| {
            find_companion(reference, companions)
                .or_else(|| read_beside(asset.path.as_deref()?, reference))
        });
        match found {
            Ok(import) => {
                asset.files = import
                    .files
                    .into_iter()
                    .map(|(name, bytes)| (name, Arc::from(bytes)))
                    .collect();
                (asset, import.missing)
            }
            // Loading reports the error.
            Err(_) => (asset, Vec::new()),
        }
    }

    pub fn load(&self, name: &str) -> Result<Model, LoadError> {
        if is_gltf(&self.bytes) {
            return Model::from_gltf_slice(name, &self.bytes);
        }
        Model::from_obj(name, &self.bytes, |reference| {
            self.files
                .iter()
                .find(|(n, _)| n == reference)
                .map(|(_, bytes)| bytes.to_vec())
        })
        .map(|import| import.model)
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
            files: self
                .files
                .iter()
                .map(|(name, bytes)| EmbeddedFile {
                    name: name.clone(),
                    bytes: Arc::clone(bytes),
                })
                .collect(),
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
            ModelSource::Embedded {
                builtin,
                bytes,
                files,
            } => Ok(Self {
                builtin: builtin.as_deref().and_then(find_builtin),
                bytes: Cow::Owned(bytes.to_vec()),
                path: None,
                files: files.into_iter().map(|f| (f.name, f.bytes)).collect(),
            }),
            ModelSource::Path(path) => {
                let directory = scene_path
                    .and_then(Path::parent)
                    .ok_or_else(|| path.display().to_string())?;
                let path = directory.join(path);
                if !MODEL_EXTENSIONS.contains(&extension(&file_name(&path)).as_str())
                    || !path.is_file()
                {
                    return Err(path.display().to_string());
                }
                let bytes =
                    std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
                let (asset, missing) = Self::imported(Some(path), bytes, &[]);
                if !missing.is_empty() {
                    log::warn!("model files not found: {}", missing.join(", "));
                }
                Ok(asset)
            }
        }
    }
}

/// The extensions of model files.
pub(super) const MODEL_EXTENSIONS: &[&str] = &["glb", "gltf", "obj"];

/// The extensions of files that come with an OBJ: materials and textures.
pub(super) const COMPANION_EXTENSIONS: &[&str] = &["mtl", "png", "jpg", "jpeg", "tga", "bmp"];

/// Whether `bytes` are a glTF file (binary, or JSON) rather than an OBJ.
fn is_gltf(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    bytes.starts_with(b"glTF") || text.trim_ascii_start().starts_with(b"{")
}

/// The last part of a file reference, whichever slashes it uses.
fn reference_file_name(reference: &str) -> &str {
    reference.rsplit(['/', '\\']).next().unwrap_or(reference)
}

/// A companion with the file name `reference` ends in, ignoring case:
/// browsers give dropped files no folders.
fn find_companion(reference: &str, companions: &[Companion]) -> Option<Vec<u8>> {
    let wanted = reference_file_name(reference);
    companions
        .iter()
        .find(|(name, _)| reference_file_name(name).eq_ignore_ascii_case(wanted))
        .map(|(_, bytes)| bytes.clone())
}

/// The file `reference` names relative to the model at `model_path`, or
/// with that name right beside it.
fn read_beside(model_path: &Path, reference: &str) -> Option<Vec<u8>> {
    let directory = model_path.parent()?;
    let relative = reference.replace('\\', "/");
    std::fs::read(directory.join(&relative))
        .or_else(|_| std::fs::read(directory.join(reference_file_name(&relative))))
        .ok()
}

#[cfg(any(test, target_arch = "wasm32"))]
/// Files picked or dropped together, as results: when there is an OBJ
/// among them, its MTL and image files go with each OBJ rather than being
/// opened on their own.
pub(super) fn with_companions(
    files: Vec<(String, Result<Vec<u8>, String>)>,
    kind: FileKind,
) -> Vec<FileResult> {
    let is = |name: &str, extensions: &[&str]| extensions.contains(&extension(name).as_str());
    let has_obj = files.iter().any(|(name, _)| is(name, &["obj"]));
    let (companions, files): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|(name, _)| has_obj && is(name, COMPANION_EXTENSIONS));
    let companions: Vec<Companion> = companions
        .into_iter()
        .filter_map(|(name, bytes)| Some((name, bytes.ok()?)))
        .collect();
    files
        .into_iter()
        .map(|(name, bytes)| FileResult {
            companions: if is(&name, &["obj"]) {
                companions.clone()
            } else {
                Vec::new()
            },
            name,
            path: None,
            bytes,
            kind,
        })
        .collect()
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
                companions: Vec::new(),
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
                Ok(results) => incoming.lock().unwrap().extend(results),
                Err(error) => incoming.lock().unwrap().push(FileResult {
                    name: default_file_name(kind).to_owned(),
                    bytes: Err(format!("{error:?}")),
                    path: None,
                    kind,
                    companions: Vec::new(),
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
        FileKind::CameraPreset => "preset.camera.ron",
        FileKind::RoomPreset => "preset.room.ron",
        _ => "file",
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn write_ron(bytes: &[u8], label: &str, file_name: &str) -> Result<(), String> {
    write_file(bytes, label, "ron", file_name)
}

/// Saves `bytes` where the artist chooses in a save dialog, as a file
/// ending in `.extension`. Cancelling the dialog is not an error.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn write_file(
    bytes: &[u8],
    label: &str,
    extension: &str,
    file_name: &str,
) -> Result<(), String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter(label, &[extension])
        .set_file_name(file_name)
        .save_file()
    else {
        return Ok(());
    };
    write_to_path(bytes, path, extension).map_err(|error| error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_to_path(bytes: &[u8], mut path: PathBuf, wanted: &str) -> std::io::Result<()> {
    use std::io::Write;

    let add_extension = extension(&file_name(&path)) != wanted;
    if add_extension {
        path.as_mut_os_string().push(format!(".{wanted}"));
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
pub(super) fn write_ron(bytes: &[u8], label: &str, file_name: &str) -> Result<(), String> {
    write_file(bytes, label, "ron", file_name)
}

/// Downloads `bytes` as `file_name`.
#[cfg(target_arch = "wasm32")]
pub(super) fn write_file(
    bytes: &[u8],
    _label: &str,
    _extension: &str,
    file_name: &str,
) -> Result<(), String> {
    web::download(bytes, file_name).map_err(|error| format!("{error:?}"))
}

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{Blob, Document, HtmlAnchorElement, HtmlInputElement, Url};

    use super::{COMPANION_EXTENSIONS, FileKind, FileResult, with_companions};

    fn document() -> Result<Document, JsValue> {
        web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("browser document is unavailable"))
    }

    /// The files the artist picks. Picking a model allows several files,
    /// so an OBJ can come with its MTL and textures.
    pub async fn pick(kind: FileKind) -> Result<Vec<FileResult>, JsValue> {
        let document = document()?;
        let input: HtmlInputElement = document.create_element("input")?.dyn_into()?;
        input.set_type("file");
        let model = matches!(kind, FileKind::Model);
        let companions: &[&str] = if model { COMPANION_EXTENSIONS } else { &[] };
        let accept: Vec<String> = kind
            .extensions()
            .iter()
            .chain(companions)
            .map(|e| format!(".{e}"))
            .collect();
        input.set_accept(&accept.join(","));
        input.set_multiple(model);
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
        let Some(files) = input.files() else {
            return Ok(Vec::new());
        };
        let mut read = Vec::new();
        for index in 0..files.length() {
            if let Some(file) = files.item(index) {
                read.push((file.name(), read_file(&file).await));
            }
        }
        Ok(with_companions(read, kind))
    }

    async fn read_file(file: &web_sys::File) -> Result<Vec<u8>, String> {
        if file.size() > f64::from(u32::MAX) {
            return Err("file is too large for the browser".to_owned());
        }
        file.array_buffer()
            .await
            .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
            .map_err(|error| format!("{error:?}"))
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
        studio.load_file("imported.glb", Some(path.clone()), glb.to_vec(), &[], &tr);
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
        write_to_path(referenced.as_bytes(), scene_path.clone(), "ron").unwrap();

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
            write_to_path(b"would overwrite", without_extension, "ron")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(scene_path).unwrap(), document);
    }

    const CUBE_OBJ: &str = "mtllib cube.mtl\n\
        v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
        usemtl Red\nf 1 4 3 2\nf 5 6 7 8\nf 1 5 8 4\n\
        f 2 3 7 6\nf 4 8 7 3\nf 1 2 6 5\n";
    const CUBE_MTL: &str = "newmtl Red\nKd 1 0 0\nmap_Kd maps\\red.png\n";

    fn red_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn only_model(studio: &Studio) -> std::sync::Arc<Model> {
        // The last object: a new Studio starts with a figure.
        let object = studio.scene.objects().last().unwrap();
        std::sync::Arc::clone(studio.scene.model(object.model).unwrap())
    }

    #[test]
    fn a_desktop_obj_finds_its_mtl_and_textures_and_embeds_them() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cube.obj");
        std::fs::write(&path, CUBE_OBJ).unwrap();
        std::fs::write(directory.path().join("cube.mtl"), CUBE_MTL).unwrap();
        std::fs::create_dir(directory.path().join("maps")).unwrap();
        std::fs::write(directory.path().join("maps/red.png"), red_png()).unwrap();
        let tr = Localizer::new(Language::English);
        let mut studio = Studio::new();
        studio.load_file("cube.obj", Some(path.clone()), CUBE_OBJ.into(), &[], &tr);
        assert!(studio.message.is_none(), "{:?}", studio.message);
        let model = only_model(&studio);
        assert_eq!(model.materials.len(), 1);
        assert_eq!(
            model.textures.len(),
            1,
            "the texture in a folder, Windows slashes"
        );

        // Embedded: the MTL and texture travel inside the scene file.
        let embedded = studio
            .scene
            .to_ron(|id| studio.sources.get(&id).map(|asset| asset.source(true)))
            .unwrap();
        let referenced = studio
            .scene
            .to_ron(|id| studio.sources.get(&id).map(|asset| asset.source(false)))
            .unwrap();
        let scene_path = directory.path().join("study.ron");
        std::fs::write(&scene_path, &referenced).unwrap();
        let mut restored = Studio::new();
        assert!(restored.open_scene("study.ron", Some(&scene_path), referenced.as_bytes(), &tr));
        assert_eq!(
            only_model(&restored).textures.len(),
            1,
            "read beside the OBJ"
        );

        drop(directory);
        let mut restored = Studio::new();
        assert!(restored.open_scene("study.ron", None, embedded.as_bytes(), &tr));
        let model = only_model(&restored);
        assert_eq!(model.materials.len(), 1);
        assert_eq!(model.textures.len(), 1);
    }

    #[test]
    fn an_obj_alone_loads_as_clay_with_a_note() {
        let tr = Localizer::new(Language::English);
        let mut studio = Studio::new();
        studio.load_file("cube.obj", None, CUBE_OBJ.into(), &[], &tr);
        assert_eq!(studio.scene.object_count(), 2);
        assert!(!only_model(&studio).has_materials());
        let message = studio.message.as_deref().unwrap();
        assert!(message.contains("cube.mtl"), "{message}");
    }

    #[test]
    fn files_given_together_go_with_the_obj() {
        let read = |names: &[&str]| {
            names
                .iter()
                .map(|n| (n.to_string(), Ok(n.as_bytes().to_vec())))
                .collect::<Vec<_>>()
        };
        let results = with_companions(read(&["cube.obj", "cube.mtl", "red.PNG"]), FileKind::Model);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "cube.obj");
        assert_eq!(results[0].companions.len(), 2);
        // Without an OBJ, an image is a file of its own.
        let results = with_companions(read(&["mask.png"]), FileKind::ModelOrImage);
        assert_eq!(results.len(), 1);
        assert!(results[0].companions.is_empty());

        // Browsers give no folders: a texture is matched by its file name.
        let tr = Localizer::new(Language::English);
        let mut studio = Studio::new();
        let companions = vec![
            ("cube.mtl".to_owned(), CUBE_MTL.as_bytes().to_vec()),
            ("RED.png".to_owned(), red_png()),
        ];
        studio.load_file("cube.obj", None, CUBE_OBJ.into(), &companions, &tr);
        assert!(studio.message.is_none(), "{:?}", studio.message);
        assert_eq!(only_model(&studio).textures.len(), 1);
    }
}
