//! Application shell: top bar, section switching and language.

use houdon_i18n::{Language, Localizer};
use serde::{Deserialize, Serialize};

use crate::studio::Studio;

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
}

const SETTINGS_KEY: &str = "houdonref-settings";

pub struct HoudonApp {
    localizer: Localizer,
    section: Section,
    studio: Studio,
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

        Self {
            localizer: Localizer::new(language),
            section: settings.section,
            studio: Studio::new(),
        }
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
            });
        });
    }
}

impl eframe::App for HoudonApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.studio.poll_file_loads(ui.ctx(), &self.localizer);

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
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let settings = Settings {
            language: Some(self.localizer.language().code().to_owned()),
            section: self.section,
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
