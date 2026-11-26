//! The Atlas tab: the human body, bone and muscle, layer by layer.

use std::sync::{Arc, Mutex};

use houdon_core::atlas::ATLAS_MANIFEST;
use houdon_core::atlas::{DEEP_MUSCLE_SRGB, MASSES, Mass, mass_of};
use houdon_core::{Atlas, AtlasColors, AtlasPack, AtlasView, Layer, LayerState, ShadingMode, Side};
use houdon_i18n::{Language, Localizer};
use houdon_render::{AntiAliasing, Frame, RenderQuality, ViewportCallback};

use crate::studio::{drive_camera, physical_size};

/// Where the human pack is, next to the web page.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const PACK_URL: &str = "atlas/human/";

/// The human pack, carried in desktop builds.
#[cfg(not(target_arch = "wasm32"))]
fn carried_file(name: &str) -> Option<Vec<u8>> {
    macro_rules! pack {
        ($($file:literal),*) => {
            match name {
                $($file => Some(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/atlas/human/",
                    $file
                )).to_vec()),)*
                _ => None,
            }
        };
    }
    pack!(
        "atlas.ron",
        "skeletal.bin",
        "muscular.bin",
        "joints.bin",
        "names.ru.ron"
    )
}

type Arrival = Arc<Mutex<Option<Result<Atlas, String>>>>;

enum State {
    /// The tab has not been opened yet: nothing is loaded.
    Idle,
    Loading(Arrival),
    Ready(Box<AtlasView>),
    Failed(String),
}

pub struct AtlasTab {
    state: State,
    anti_aliasing: AntiAliasing,
    quality: RenderQuality,
    view_size: [u32; 2],
    /// Frame the body (or the selection) once the view knows its
    /// proportions.
    frame_pending: Option<Framing>,
    search: String,
}

/// What to point the camera at.
#[derive(Clone, Copy, PartialEq)]
enum Framing {
    Shown,
    Selected,
}

impl Default for AtlasTab {
    fn default() -> Self {
        Self {
            state: State::Idle,
            anti_aliasing: AntiAliasing::VIEWPORT,
            quality: RenderQuality::default(),
            view_size: [1, 1],
            frame_pending: Some(Framing::Shown),
            search: String::new(),
        }
    }
}

impl AtlasTab {
    /// The app settings that affect how the Atlas is drawn.
    pub fn apply_settings(&mut self, anti_aliasing: AntiAliasing, quality: RenderQuality) {
        self.anti_aliasing = anti_aliasing;
        self.quality = quality;
    }

    /// Whether the pack has been asked for (for tests: never at start).
    #[cfg(test)]
    pub fn started(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        self.poll(ui.ctx());
        match &mut self.state {
            State::Idle => {
                self.state = State::Loading(start_loading(ui.ctx()));
                message(ui, &tr.tr("atlas-loading"));
            }
            State::Loading(_) => message(ui, &tr.tr("atlas-loading")),
            State::Failed(reason) => message(
                ui,
                &tr.tr_args("atlas-load-failed", &[("reason", reason.clone().into())]),
            ),
            State::Ready(_) => self.body(ui, tr),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let State::Loading(arrival) = &self.state else {
            return;
        };
        let Some(result) = arrival.lock().unwrap().take() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        };
        self.state = match result {
            Ok(atlas) => {
                let mut view = AtlasView::new(atlas);
                view.scene.shading.mode = ShadingMode::Lit;
                State::Ready(Box::new(view))
            }
            Err(reason) => {
                log::error!("the Atlas could not load: {reason}");
                State::Failed(reason)
            }
        };
    }

    fn body(&mut self, ui: &mut egui::Ui, tr: &Localizer) {
        let State::Ready(view) = &mut self.state else {
            return;
        };
        egui::Panel::left("atlas_panel")
            .resizable(true)
            .default_size(260.0)
            .min_size(200.0)
            .max_size(340.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(framing) = panel(ui, tr, view, &mut self.search) {
                        self.frame_pending = Some(framing);
                    }
                });
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let aspect = rect.width() / rect.height().max(1.0);
            self.view_size = physical_size(rect, ui.ctx().pixels_per_point());
            if response.double_clicked() {
                self.frame_pending = Some(Framing::Selected);
            }
            match self.frame_pending.take() {
                Some(Framing::Shown) => view.frame(aspect),
                Some(Framing::Selected) => view.frame_selected(aspect),
                None => {}
            }
            drive_camera(&mut view.scene.camera, ui, &response, rect);
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let ndc_x = (pointer.x - rect.left()) / rect.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pointer.y - rect.top()) / rect.height() * 2.0;
                let (origin, direction) = view.scene.camera.ray(ndc_x, ndc_y, aspect);
                let hit = view.pick(origin, direction);
                view.select(hit);
            }
            let painter = ui.painter_at(rect);
            let [r, g, b] = view.scene.room.backdrop_srgb();
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
            painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    frame: Frame {
                        quality: self.quality,
                        ..Frame::from_atlas(view, aspect)
                    },
                    size: self.view_size,
                    anti_aliasing: self.anti_aliasing,
                },
            ));
        });
    }
}

/// The side panel: search, the selection, layers, shading, the list of
/// structures and credits. Returns what to frame, if anything.
fn panel(
    ui: &mut egui::Ui,
    tr: &Localizer,
    view: &mut AtlasView,
    search: &mut String,
) -> Option<Framing> {
    let mut framing = None;
    ui.add(
        egui::TextEdit::singleline(search)
            .hint_text(tr.tr("atlas-search"))
            .desired_width(f32::INFINITY),
    );
    if !search.trim().is_empty() {
        let found = view.search(search, |s| translated(s, tr));
        if found.is_empty() {
            ui.weak(tr.tr("atlas-search-none"));
        }
        for &index in found.iter().take(30) {
            let name = display_name(&view.atlas().structures()[index], tr);
            if ui
                .selectable_label(view.selected() == Some(index), name)
                .clicked()
            {
                view.select(Some(index));
                framing = Some(Framing::Selected);
            }
        }
        if found.len() > 1 && ui.button(tr.tr("atlas-isolate-found")).clicked() {
            view.isolate(found);
            framing = Some(Framing::Shown);
        }
        ui.separator();
    }

    if let Some(index) = view.selected() {
        let structure = &view.atlas().structures()[index];
        ui.strong(display_name(structure, tr));
        if let Some(latin) = &structure.latin {
            ui.label(egui::RichText::new(latin).italics());
        }
        match mass_of(structure) {
            Some(mass) => ui.weak(tr.tr_args(
                "atlas-mass-of",
                &[("mass", tr.tr(&mass_message_id(mass)).into())],
            )),
            None => ui.weak(tr.tr(layer_message_id(Layer::of(structure)))),
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr.tr("atlas-isolate")).clicked() {
                view.isolate([index]);
                framing = Some(Framing::Selected);
            }
            if ui.button(tr.tr("atlas-hide")).clicked() {
                view.hide(index);
            }
            if ui.button(tr.tr("atlas-frame-selected")).clicked() {
                framing = Some(Framing::Selected);
            }
        });
        ui.separator();
    } else {
        ui.weak(tr.tr("atlas-select-hint"));
        ui.separator();
    }

    if (view.is_isolating() || view.hides_any()) && ui.button(tr.tr("atlas-show-all")).clicked() {
        view.show_all();
        framing = Some(Framing::Shown);
    }

    ui.heading(tr.tr("atlas-layers"));
    for layer in Layer::ALL {
        let mut state = view.layer(layer);
        let before = state;
        ui.checkbox(&mut state.visible, tr.tr(layer_message_id(layer)));
        if state.visible {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                // Further right is more see-through.
                let mut see_through = 1.0 - state.opacity;
                ui.add(
                    egui::Slider::new(&mut see_through, 0.0..=1.0 - LayerState::MIN_OPACITY)
                        .show_value(false),
                );
                state.opacity = 1.0 - see_through;
                ui.label(tr.tr("atlas-opacity"));
            });
        }
        if state != before {
            view.set_layer(layer, state);
        }
    }
    ui.separator();
    ui.heading(tr.tr("atlas-look"));
    let shading = &mut view.scene.shading;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut shading.mode, ShadingMode::Lit, tr.tr("shading-lit"));
        ui.selectable_value(
            &mut shading.mode,
            ShadingMode::Solid,
            tr.tr("shading-solid"),
        );
    });
    ui.horizontal_wrapped(|ui| {
        for (colors, id) in [
            (AtlasColors::Masses, "atlas-color-masses"),
            (AtlasColors::Natural, "atlas-color-natural"),
            (AtlasColors::Clay, "color-clay"),
        ] {
            ui.selectable_value(&mut view.colors, colors, tr.tr(id));
        }
    });
    let shading = &mut view.scene.shading;
    ui.checkbox(
        &mut shading.ambient_occlusion.enabled,
        tr.tr("studio-ambient-occlusion"),
    );
    if ui.button(tr.tr("atlas-frame")).clicked() {
        framing = Some(Framing::Shown);
    }
    if view.colors == AtlasColors::Masses {
        ui.separator();
        legend(ui, tr, view, &mut framing);
    }
    ui.separator();
    structure_list(ui, tr, view, &mut framing);
    ui.separator();
    ui.weak(tr.tr("atlas-credit"));
    framing
}

/// The masses with their colours; a click shows only that mass.
fn legend(ui: &mut egui::Ui, tr: &Localizer, view: &mut AtlasView, framing: &mut Option<Framing>) {
    ui.heading(tr.tr("atlas-masses"));
    ui.weak(tr.tr("atlas-masses-hint"));
    for mass in MASSES {
        ui.horizontal(|ui| {
            let [r, g, b] = mass.srgb;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 3.0, egui::Color32::from_rgb(r, g, b));
            if ui.link(tr.tr(&mass_message_id(mass))).clicked() {
                view.isolate(view.structures_of(mass));
                *framing = Some(Framing::Shown);
            }
        });
    }
    ui.horizontal(|ui| {
        let [r, g, b] = DEEP_MUSCLE_SRGB;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 3.0, egui::Color32::from_rgb(r, g, b));
        ui.label(tr.tr("mass-deep"));
    });
}

/// The structure's name in the interface's language, when it has one other
/// than English.
fn translated(structure: &houdon_core::Structure, tr: &Localizer) -> Option<String> {
    match tr.language() {
        Language::Russian => structure.russian.as_ref().map(|r| r.name.clone()),
        _ => None,
    }
}

fn mass_message_id(mass: &Mass) -> String {
    format!("mass-{}", mass.key)
}

/// Every structure by layer, alphabetically, each with a box to hide it.
fn structure_list(
    ui: &mut egui::Ui,
    tr: &Localizer,
    view: &mut AtlasView,
    framing: &mut Option<Framing>,
) {
    ui.heading(tr.tr("atlas-structures"));
    for layer in Layer::ALL {
        egui::CollapsingHeader::new(tr.tr(layer_message_id(layer)))
            .id_salt(("atlas-list", layer_message_id(layer)))
            .show(ui, |ui| {
                let mut rows: Vec<(String, usize)> = view
                    .atlas()
                    .structures()
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| Layer::of(s) == layer)
                    .map(|(i, s)| (display_name(s, tr), i))
                    .collect();
                rows.sort();
                for (name, index) in rows {
                    ui.horizontal(|ui| {
                        let mut shown = !view.is_hidden_alone(index);
                        if ui.checkbox(&mut shown, "").changed() {
                            view.set_hidden(index, !shown);
                        }
                        if ui
                            .selectable_label(view.selected() == Some(index), name)
                            .clicked()
                        {
                            view.select(Some(index));
                            *framing = Some(Framing::Selected);
                        }
                    });
                }
            });
    }
}

/// A structure's name in the interface's language, with its side. Russian
/// names show only in the Russian interface.
fn display_name(structure: &houdon_core::Structure, tr: &Localizer) -> String {
    let name = translated(structure, tr).unwrap_or_else(|| structure.name.clone());
    match structure.side {
        Side::Left => tr.tr_args("atlas-left", &[("name", name.into())]),
        Side::Right => tr.tr_args("atlas-right", &[("name", name.into())]),
        Side::Middle => name,
    }
}

fn layer_message_id(layer: Layer) -> &'static str {
    match layer {
        Layer::Bones => "atlas-layer-bones",
        Layer::Muscles => "atlas-layer-muscles",
        Layer::Fasciae => "atlas-layer-fasciae",
        Layer::Joints => "atlas-layer-joints",
    }
}

fn message(ui: &mut egui::Ui, text: &str) {
    egui::CentralPanel::default_margins().show(ui, |ui| {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(text).size(18.0));
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn build(file: impl FnMut(&str) -> Option<Vec<u8>>, manifest: &[u8]) -> Result<Atlas, String> {
    AtlasPack::parse(manifest)
        .and_then(|pack| pack.build(file))
        .map_err(|error| error.to_string())
}

/// Reads the pack on a thread of its own, so the window keeps drawing.
#[cfg(not(target_arch = "wasm32"))]
fn start_loading(ctx: &egui::Context) -> Arrival {
    let arrival = Arrival::default();
    let sink = Arc::clone(&arrival);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = match carried_file(ATLAS_MANIFEST) {
            Some(manifest) => build(carried_file, &manifest),
            None => Err("the Atlas pack is missing".to_owned()),
        };
        *sink.lock().unwrap() = Some(result);
        ctx.request_repaint();
    });
    arrival
}

/// Fetches the pack from the site, then reads it.
#[cfg(target_arch = "wasm32")]
fn start_loading(ctx: &egui::Context) -> Arrival {
    let arrival = Arrival::default();
    let sink = Arc::clone(&arrival);
    let ctx = ctx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let result = async {
            async fn fetch(name: &str) -> Result<Vec<u8>, String> {
                crate::masks::fetch_bytes(&format!("{PACK_URL}{name}")).await
            }
            let manifest = fetch(ATLAS_MANIFEST).await?;
            let pack = AtlasPack::parse(&manifest).map_err(|e| e.to_string())?;
            let mut files = std::collections::HashMap::new();
            for name in pack.files() {
                files.insert(name.to_owned(), fetch(name).await?);
            }
            // The Russian names are welcome but not needed.
            use houdon_core::atlas::RUSSIAN_NAMES;
            if let Ok(names) = fetch(RUSSIAN_NAMES).await {
                files.insert(RUSSIAN_NAMES.to_owned(), names);
            }
            pack.build(|name| files.remove(name))
                .map_err(|e| e.to_string())
        }
        .await;
        *sink.lock().unwrap() = Some(result);
        ctx.request_repaint();
    });
    arrival
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn the_carried_pack_reads() {
        let manifest = carried_file(ATLAS_MANIFEST).unwrap();
        let atlas = build(carried_file, &manifest).unwrap();
        assert_eq!(atlas.structures().len(), 1295);
    }

    #[test]
    fn every_mass_has_a_name() {
        for language in houdon_i18n::Language::ALL {
            let tr = Localizer::new(language);
            for mass in MASSES {
                let id = mass_message_id(mass);
                assert_ne!(tr.tr(&id), id, "{language:?} names {id}");
            }
        }
    }

    #[test]
    fn russian_names_show_only_in_the_russian_interface() {
        let manifest = carried_file(ATLAS_MANIFEST).unwrap();
        let atlas = build(carried_file, &manifest).unwrap();
        let femur = atlas.structure("Femur.l").unwrap();
        let english = Localizer::new(Language::English);
        let russian = Localizer::new(Language::Russian);
        assert_eq!(display_name(femur, &english), "Femur (left)");
        assert_eq!(display_name(femur, &russian), "Бедренная кость (слева)");
        let cyrillic = |text: &str| {
            text.chars()
                .any(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap()))
        };
        for structure in atlas.structures() {
            assert!(
                !cyrillic(&display_name(structure, &english)),
                "{}",
                structure.id
            );
        }
    }

    #[test]
    fn nothing_loads_before_the_tab_is_opened() {
        let tab = AtlasTab::default();
        assert!(!tab.started());
    }
}
