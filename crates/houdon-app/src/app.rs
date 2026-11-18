//! Application shell: top bar, section switching and language.

use houdon_core::UnitSystem;
use houdon_core::model::DEFAULT_FIGURE_HEIGHT;
use houdon_i18n::{Language, Localizer};
use serde::{Deserialize, Serialize};

use crate::studio::{Studio, unit_message_id};

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

/// Figure heights the settings accept, metres.
const FIGURE_HEIGHTS: std::ops::RangeInclusive<f32> = 0.3..=3.0;

const SETTINGS_KEY: &str = "houdonref-settings";

pub struct HoudonApp {
    localizer: Localizer,
    section: Section,
    studio: Studio,
    units: UnitSystem,
    figure_height: f32,
    settings_open: bool,
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

        Self {
            localizer: Localizer::new(language),
            section: settings.section,
            studio: Studio::new(),
            units,
            figure_height,
            settings_open: false,
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
                let unit = format!(" {}", tr.tr(unit_message_id(self.units.editing_unit())));
                let mut shown = self.units.to_editing(self.figure_height);
                let range = self.units.to_editing(*FIGURE_HEIGHTS.start())
                    ..=self.units.to_editing(*FIGURE_HEIGHTS.end());
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::DragValue::new(&mut shown)
                                .speed(0.01)
                                .range(range)
                                .suffix(unit.as_str())
                                .max_decimals(2),
                        )
                        .changed()
                    {
                        self.figure_height = self.units.from_editing(shown);
                    }
                    ui.label(tr.tr("settings-figure-height"));
                });
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
        self.studio.poll_file_loads(ui.ctx(), &self.localizer);
        self.studio.apply_settings(self.units, self.figure_height);

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });

        match self.section {
            Section::Studio => self.studio.ui(ui, &self.localizer),
            Section::Atlas => coming_soon(ui, &self.localizer.tr("atlas-coming")),
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
