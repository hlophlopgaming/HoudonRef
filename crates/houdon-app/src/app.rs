//! Application shell: top bar, section switching and language.

use houdon_core::UnitSystem;
use houdon_core::model::DEFAULT_FIGURE_HEIGHT;
use houdon_i18n::{Language, Localizer};
use houdon_render::{QualityGovernor, QualityLevel};
use serde::{Deserialize, Serialize};

use crate::atlas::AtlasTab;
use crate::studio::{Studio, length_field};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Section {
    Atlas,
    #[default]
    Studio,
    Atelier,
}

impl Section {
    const ALL: [Self; 3] = [Self::Atlas, Self::Studio, Self::Atelier];

    fn message_id(self) -> &'static str {
        match self {
            Self::Atlas => "section-atlas",
            Self::Studio => "section-studio",
            Self::Atelier => "section-atelier",
        }
    }
}

/// Settings kept between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    language: Option<String>,
    section: Section,
    /// "metric" or "imperial".
    units: Option<String>,
    /// Height Figures are shown at, metres.
    figure_height: Option<f32>,
    /// Smooth edges in the viewport; on unless turned off.
    anti_aliasing: Option<bool>,
    /// "low", "medium" or "high"; automatic when unset.
    quality: Option<String>,
}

fn units_code(units: UnitSystem) -> &'static str {
    match units {
        UnitSystem::Metric => "metric",
        UnitSystem::Imperial => "imperial",
    }
}

fn units_message_id(units: UnitSystem) -> &'static str {
    match units {
        UnitSystem::Metric => "units-metric",
        UnitSystem::Imperial => "units-imperial",
    }
}

fn quality_code(level: QualityLevel) -> &'static str {
    match level {
        QualityLevel::Low => "low",
        QualityLevel::Medium => "medium",
        QualityLevel::High => "high",
    }
}

fn quality_message_id(level: QualityLevel) -> &'static str {
    match level {
        QualityLevel::Low => "settings-quality-low",
        QualityLevel::Medium => "settings-quality-medium",
        QualityLevel::High => "settings-quality-high",
    }
}

/// Figure heights the settings accept, metres.
const FIGURE_HEIGHTS: std::ops::RangeInclusive<f32> = 0.3..=3.0;

const SETTINGS_KEY: &str = "houdonref-settings";

pub struct HoudonApp {
    localizer: Localizer,
    section: Section,
    studio: Studio,
    atlas: AtlasTab,
    units: UnitSystem,
    figure_height: f32,
    anti_aliasing: bool,
    /// Rendering quality: the artist's choice, or one that follows the
    /// hardware and frame times.
    quality: QualityGovernor,
    settings_open: bool,
    /// Only the Studio's viewport shows, to draw from the screen.
    presentation: bool,
}

impl HoudonApp {
    pub fn new(cc: &eframe::CreationContext<'_>, depth_bits: u8) -> Self {
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        let language = settings
            .language
            .as_deref()
            .and_then(Language::from_code)
            .unwrap_or_else(system_language);

        if let Some(render_state) = cc.wgpu_render_state.as_ref() {
            let depth_format = egui_wgpu_depth_format(depth_bits);
            houdon_render::SceneRenderer::install(render_state, depth_format);
        } else {
            log::error!("no wgpu render state: the 3D viewport will be empty");
        }

        let units = settings
            .units
            .as_deref()
            .and_then(|code| UnitSystem::ALL.into_iter().find(|u| units_code(*u) == code))
            .unwrap_or_default();
        let figure_height = settings
            .figure_height
            .filter(|h| FIGURE_HEIGHTS.contains(h))
            .unwrap_or(DEFAULT_FIGURE_HEIGHT);

        let manual_quality = settings.quality.as_deref().and_then(|code| {
            QualityLevel::ALL
                .into_iter()
                .find(|level| quality_code(*level) == code)
        });
        let start_quality =
            cc.wgpu_render_state
                .as_ref()
                .map_or(QualityLevel::Medium, |render_state| {
                    let adapter = render_state.adapter.get_info();
                    let level = QualityLevel::for_adapter(&adapter);
                    log::info!("graphics adapter {adapter:?}: starting at {level:?} quality");
                    level
                });

        let localizer = Localizer::new(language);
        let mut studio = Studio::new();
        studio.load_presets(&localizer);
        if let Some(render_state) = cc.wgpu_render_state.as_ref() {
            studio.set_render_state(render_state);
        }
        Self {
            localizer,
            section: settings.section,
            studio,
            atlas: AtlasTab::default(),
            units,
            figure_height,
            anti_aliasing: settings.anti_aliasing.unwrap_or(true),
            quality: QualityGovernor::new(start_quality, manual_quality),
            settings_open: false,
            presentation: false,
        }
    }

    /// Presentation mode: P (or the toolbar's button) hides every panel in
    /// the Studio; P or Escape brings them back. Not while typing.
    fn toggle_presentation(&mut self, ctx: &egui::Context) {
        if self.section != Section::Studio {
            self.presentation = false;
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let (p, escape) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::P),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if self.studio.take_presentation_request() || (p && !typing) {
            self.presentation = !self.presentation;
        } else if escape && self.presentation {
            self.presentation = false;
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let tr = &self.localizer;
        let mut open = self.settings_open;
        egui::Window::new(tr.tr("settings"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.strong(tr.tr("settings-units"));
                for units in UnitSystem::ALL {
                    ui.radio_value(&mut self.units, units, tr.tr(units_message_id(units)));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    length_field(ui, tr, self.units, &mut self.figure_height, FIGURE_HEIGHTS);
                    ui.label(tr.tr("settings-figure-height"));
                });
                ui.separator();
                ui.checkbox(&mut self.anti_aliasing, tr.tr("settings-anti-aliasing"));
                ui.weak(tr.tr("settings-anti-aliasing-hint"));
                ui.separator();
                ui.strong(tr.tr("settings-quality"));
                let mut manual = self.quality.manual();
                let automatic = tr.tr_args(
                    "settings-quality-auto",
                    &[(
                        "level",
                        tr.tr(quality_message_id(self.quality.automatic())).into(),
                    )],
                );
                ui.radio_value(&mut manual, None, automatic);
                for level in QualityLevel::ALL {
                    ui.radio_value(&mut manual, Some(level), tr.tr(quality_message_id(level)));
                }
                ui.weak(tr.tr("settings-quality-hint"));
                if manual != self.quality.manual() {
                    self.quality.set_manual(manual);
                }
            });
        self.settings_open = open;
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(self.localizer.tr("app-title"));
            ui.separator();
            for section in Section::ALL {
                let label = self.localizer.tr(section.message_id());
                ui.selectable_value(&mut self.section, section, label);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = self.localizer.language();
                let mut selected = current;
                egui::ComboBox::from_id_salt("language")
                    .selected_text(current.native_name())
                    .show_ui(ui, |ui| {
                        for language in Language::ALL {
                            ui.selectable_value(&mut selected, language, language.native_name());
                        }
                    });
                ui.label(self.localizer.tr("language"));
                if selected != current {
                    self.localizer = Localizer::new(selected);
                }
                ui.separator();
                ui.toggle_value(&mut self.settings_open, self.localizer.tr("settings"));
            });
        });
    }
}

impl eframe::App for HoudonApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(height) = self.studio.poll_file_loads(ui.ctx(), &self.localizer) {
            self.figure_height = height;
        }
        if self.section == Section::Studio && self.quality.frame(ui.ctx().input(|i| i.unstable_dt))
        {
            log::info!(
                "frames stay slow: quality lowered to {:?}",
                self.quality.automatic()
            );
        }
        self.studio.apply_settings(
            self.units,
            self.figure_height,
            self.anti_aliasing,
            self.quality.level().budget(),
        );
        self.atlas.apply_settings(
            if self.anti_aliasing {
                houdon_render::AntiAliasing::VIEWPORT
            } else {
                houdon_render::AntiAliasing::OFF
            },
            self.quality.level().budget(),
        );

        self.toggle_presentation(ui.ctx());
        if !self.presentation {
            egui::Panel::top("top_bar").show(ui, |ui| {
                ui.add_space(4.0);
                self.top_bar(ui);
                ui.add_space(4.0);
            });
        }

        match self.section {
            Section::Studio => self.studio.ui(ui, &self.localizer, self.presentation),
            Section::Atlas => self.atlas.ui(ui, &self.localizer),
            Section::Atelier => coming_soon(ui, &self.localizer.tr("atelier-coming")),
        }
        if self.settings_open {
            self.settings_window(&ui.ctx().clone());
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let settings = Settings {
            language: Some(self.localizer.language().code().to_owned()),
            section: self.section,
            units: Some(units_code(self.units).to_owned()),
            figure_height: Some(self.figure_height),
            anti_aliasing: Some(self.anti_aliasing),
            quality: self
                .quality
                .manual()
                .map(|level| quality_code(level).to_owned()),
        };
        eframe::set_value(storage, SETTINGS_KEY, &settings);
    }
}

fn coming_soon(ui: &mut egui::Ui, text: &str) {
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(text).size(18.0));
        });
    });
}

fn egui_wgpu_depth_format(depth_bits: u8) -> Option<eframe::wgpu::TextureFormat> {
    eframe::egui_wgpu::depth_format_from_bits(depth_bits, 0)
}

/// Picks the interface language from the operating system or browser.
fn system_language() -> Language {
    #[cfg(target_arch = "wasm32")]
    let code = web_sys::window().and_then(|w| w.navigator().language());
    #[cfg(not(target_arch = "wasm32"))]
    let code = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|v| !v.is_empty()));
    code.as_deref()
        .and_then(Language::from_code)
        .unwrap_or_default()
}
