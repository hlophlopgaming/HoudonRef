//! The Atlas tab: the human body, bone and muscle, layer by layer.

use std::sync::{Arc, Mutex};

use houdon_core::atlas::ATLAS_MANIFEST;
use houdon_core::{Atlas, AtlasPack, AtlasView, ColorSource, Layer, LayerState, ShadingMode};
use houdon_i18n::Localizer;
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
    pack!("atlas.ron", "skeletal.bin", "muscular.bin", "joints.bin")
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
    /// Frame the body once the view knows its proportions.
    frame_pending: bool,
}

impl Default for AtlasTab {
    fn default() -> Self {
        Self {
            state: State::Idle,
            anti_aliasing: AntiAliasing::VIEWPORT,
            quality: RenderQuality::default(),
            view_size: [1, 1],
            frame_pending: true,
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
            .default_size(240.0)
            .min_size(200.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if panel(ui, tr, view) {
                        self.frame_pending = true;
                    }
                });
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let aspect = rect.width() / rect.height().max(1.0);
            self.view_size = physical_size(rect, ui.ctx().pixels_per_point());
            if std::mem::take(&mut self.frame_pending) || response.double_clicked() {
                view.frame(aspect);
            }
            drive_camera(&mut view.scene.camera, ui, &response, rect);
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

/// The side panel: layers, shading and credits. Returns whether the view
/// should be framed again.
fn panel(ui: &mut egui::Ui, tr: &Localizer, view: &mut AtlasView) -> bool {
    let mut frame = false;
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
    let mut natural = shading.color != ColorSource::Clay;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut natural, true, tr.tr("atlas-color-natural"));
        ui.selectable_value(&mut natural, false, tr.tr("color-clay"));
    });
    shading.color = if natural {
        ColorSource::Material
    } else {
        ColorSource::Clay
    };
    ui.checkbox(
        &mut shading.ambient_occlusion.enabled,
        tr.tr("studio-ambient-occlusion"),
    );
    if ui.button(tr.tr("atlas-frame")).clicked() {
        frame = true;
    }
    ui.separator();
    ui.weak(tr.tr("atlas-credit"));
    frame
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
    fn nothing_loads_before_the_tab_is_opened() {
        let tab = AtlasTab::default();
        assert!(!tab.started());
    }
}
