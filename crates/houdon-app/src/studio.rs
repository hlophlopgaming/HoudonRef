//! Studio section: arrange objects and light them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use houdon_core::Transform;
use houdon_core::{
    BuiltinRoom, Cavity, ColorSource, DEFAULT_PATTERN_SIZE, LengthUnit, Light, LightId, LightKind,
    LightMask, Lines, LoadError, MAX_LIGHTS, MaskImage, Matcap, MatcapImage, ModelId, OrbitCamera,
    PLAUSIBLE_SIZE, Scene, ShadingMode, SolidLighting, UnitSystem, ValueMode,
};
use houdon_i18n::Localizer;
use houdon_render::{AntiAliasing, Frame, ViewportCallback};

use crate::masks::{self, MaskLibrary};
use crate::matcaps::{self, MatcapLibrary};

mod builtins;
mod camera;
mod display;
mod files;
mod light_presets;
mod lights;
mod objects;
mod pickers;
mod scene_files;
mod toolbar;
mod viewport;
mod widgets;
use builtins::Builtin;
use files::{extension, file_name};
use light_presets::LightPresets;
use scene_files::{FileKind, FileResult, ModelAsset};
pub use widgets::length_field;
use widgets::*;

/// What to point the camera at on the next drawn frame, once the viewport
/// size is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameTarget {
    Scene,
    Selected,
}

pub struct Studio {
    scene: Scene,
    /// Original model files, kept for portable scene saves.
    sources: HashMap<ModelId, ModelAsset>,
    pending_frame: Option<FrameTarget>,
    /// The light being edited in the side panel; editor state, not scene state.
    selected_light: Option<LightId>,
    message: Option<String>,
    /// Files whose bytes arrive asynchronously (always on the web).
    incoming: Arc<Mutex<Vec<FileResult>>>,
    /// How lengths are shown; an app setting, not part of the scene.
    units: UnitSystem,
    /// How the viewport smooths edges; an app setting.
    anti_aliasing: AntiAliasing,
    /// Light mask images and the picker's state.
    masks: MaskLibrary,
    mask_picker_open: bool,
    mask_category: masks::Category,
    /// A mask chosen for a light while its image was still loading.
    pending_mask: Option<(LightId, &'static str)>,
    matcaps: MatcapLibrary,
    matcap_picker_open: bool,
    embed_models: bool,
    light_presets: LightPresets,
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
            anti_aliasing: AntiAliasing::VIEWPORT,
            masks: MaskLibrary::default(),
            mask_picker_open: false,
            mask_category: masks::Category::Windows,
            pending_mask: None,
            matcaps: MatcapLibrary::default(),
            matcap_picker_open: false,
            embed_models: cfg!(target_arch = "wasm32"),
            light_presets: LightPresets::default(),
        };
        studio.add_builtin(Builtin::Male);
        studio.scene.select(None);
        studio
    }

    /// Places a new object of the model from `source`, loading the model
    /// only if the scene does not have it yet, and selects the object.
    /// `real_size` (largest side, metres) is given to a newly loaded model
    /// before the object is placed, and `transform` sets the new object's
    /// rotation and size, so placement uses the object's real extent.
    fn add_object_from(
        &mut self,
        name: &str,
        asset: ModelAsset,
        real_size: Option<f32>,
        transform: Transform,
    ) -> Result<ModelId, LoadError> {
        let known = self.sources.iter().find_map(|(&id, source)| {
            (self.scene.model(id).is_some_and(|model| model.name == name) && *source == asset)
                .then_some(id)
        });
        let model_id = match known {
            Some(id) => id,
            None => {
                let id = self.scene.add_model(asset.load(name)?);
                if let Some(size) = real_size {
                    self.scene.set_real_size(id, size);
                }
                self.sources.insert(id, asset);
                id
            }
        };
        let object = self.scene.add_object_with(model_id, transform);
        self.scene.select(object);
        self.pending_frame = Some(FrameTarget::Scene);
        self.message = None;
        Ok(model_id)
    }

    fn add_builtin(&mut self, builtin: Builtin) {
        let (name, _) = builtin.name_and_bytes();
        let facing = Transform {
            rotation: glam::Vec3::new(0.0, builtin.facing(), 0.0),
            ..Transform::default()
        };
        if let Err(error) = self.add_object_from(
            name,
            ModelAsset::builtin(builtin),
            builtin.real_size(),
            facing,
        ) {
            log::error!("built-in model {name} failed to load: {error}");
        }
    }

    /// Applies the app settings that affect the Studio.
    pub fn apply_settings(&mut self, units: UnitSystem, figure_height: f32, smooth_edges: bool) {
        self.units = units;
        self.anti_aliasing = if smooth_edges {
            AntiAliasing::VIEWPORT
        } else {
            AntiAliasing::OFF
        };
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

        egui::CentralPanel::no_frame().show(ui, |ui| {
            // Toolbar, message and picture without gaps between them.
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .fill(ui.visuals().panel_fill)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    self.toolbar(ui, tr);
                });
            self.message_banner(ui);
            self.viewport(ui, tr);
        });

        for name in self.masks.poll() {
            if let Some((light, pending)) = self.pending_mask
                && pending == name
            {
                self.pending_mask = None;
                self.choose_mask(ui.ctx(), light, name);
            }
        }
        if self.mask_picker_open {
            self.mask_picker(&ui.ctx().clone(), tr);
        }
        if self.matcap_picker_open {
            self.matcap_picker(&ui.ctx().clone(), tr);
        }
    }

    /// The side panel: one collapsible section per part of the scene. The
    /// ones used most start open.
    fn side_panel(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        ui.add_space(4.0);
        section(ui, tr, "studio-objects", true, |ui| {
            self.objects_section(ui, tr);
            ui.add_space(4.0);
            self.selected_object_section(ui, tr);
        });
        section(ui, tr, "studio-lights", true, |ui| {
            self.lights_section(ui, tr)
        });
        section(ui, tr, "studio-camera", false, |ui| {
            self.camera_section(ui, tr)
        });
        section(ui, tr, "studio-display", false, |ui| {
            self.shading_section(ui, tr);
            ui.separator();
            self.color_section(ui, tr);
        });
        section(ui, tr, "studio-values", false, |ui| {
            self.values_section(ui, tr)
        });
        section(ui, tr, "studio-room", false, |ui| self.room_section(ui, tr));
    }
}

/// A collapsible section of the side panel, titled by message `title`.
fn section(
    ui: &mut egui::Ui,
    tr: &Localizer,
    title: &str,
    open: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::CollapsingHeader::new(egui::RichText::new(tr.tr(title)).strong())
        .id_salt(title)
        .default_open(open)
        .show(ui, add_contents);
}

#[cfg(test)]
mod tests {
    use super::*;
    use houdon_i18n::Language;

    #[test]
    fn built_in_models_load_at_their_real_size() {
        for builtin in Builtin::ALL {
            let mut studio = Studio::new();
            studio.add_builtin(builtin);
            let id = studio.scene.selected().expect("the new object is selected");
            let object = studio.scene.object(id).unwrap();
            let size = studio.scene.object_bounds(object).size();
            match builtin.real_size() {
                Some(real) => {
                    assert!(!object.figure, "{builtin:?} is not a figure");
                    assert!(
                        (size.max_element() - real).abs() < 0.01,
                        "{builtin:?}: {size}"
                    );
                }
                None => assert!(object.figure, "{builtin:?} is a figure"),
            }
        }
    }

    #[test]
    fn built_in_models_stand_right_next_to_the_others() {
        let mut studio = Studio::new();
        for builtin in [
            Builtin::AsaroHead,
            Builtin::PlanarHead,
            Builtin::BaseHead,
            Builtin::WolfsaroHead,
            Builtin::Female,
        ] {
            let before = studio.scene.bounds();
            studio.add_builtin(builtin);
            let id = studio.scene.selected().unwrap();
            let added = studio.scene.object_bounds(studio.scene.object(id).unwrap());
            let gap = added.min.x - before.max.x;
            assert!((0.0..0.5).contains(&gap), "{builtin:?} stands {gap} m away");
        }
    }

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
