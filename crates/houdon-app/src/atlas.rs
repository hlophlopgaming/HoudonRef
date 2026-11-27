//! The Atlas tab: the human body, bone and muscle, layer by layer.

use std::sync::{Arc, Mutex};

use glam::Vec3;
use houdon_core::atlas::{ATLAS_MANIFEST, StructureIndex};
use houdon_core::atlas::{DEEP_MUSCLE_SRGB, MASSES, Mass, mass_of};
use houdon_core::{
    Atlas, AtlasColors, AtlasPack, AtlasView, CameraFlight, CameraPreset, Layer, LayerState,
    ShadingMode, Side,
};
use houdon_i18n::{Language, Localizer};
use houdon_render::{AntiAliasing, Frame, RenderQuality, ViewportCallback};

use crate::studio::{drive_camera, physical_size};

mod pose;
use pose::PoseControls;

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
    pose_controls: PoseControls,
    anti_aliasing: AntiAliasing,
    quality: RenderQuality,
    view_size: [u32; 2],
    /// Frame the body (or the selection) once the view knows its
    /// proportions.
    frame_pending: Option<Framing>,
    search: String,
    /// A model to open in the Studio: its name and binary glTF.
    to_studio: Option<(String, Vec<u8>)>,
    /// The structure under the pointer, and where and when it was found.
    hover: Option<Hover>,
    /// The camera on its way to what was asked to be framed.
    flight: Option<CameraFlight>,
    /// Where the selection's label points: the spot clicked on it.
    anchor: Option<(StructureIndex, Vec3)>,
    /// Nothing drawn yet: the first framing is a cut, not a flight.
    first_frame: bool,
}

struct Hover {
    pointer: egui::Pos2,
    time: f64,
    index: Option<usize>,
}

/// The least time between two picks under a moving pointer, seconds.
const HOVER_INTERVAL: f64 = 0.06;

/// What to point the camera at.
#[derive(Clone, Copy, PartialEq)]
enum Framing {
    Shown,
    Selected,
    /// What is shown, from the built-in camera preset at this index.
    View(usize),
}

impl Default for AtlasTab {
    fn default() -> Self {
        Self {
            state: State::Idle,
            pose_controls: PoseControls::default(),
            anti_aliasing: AntiAliasing::VIEWPORT,
            quality: RenderQuality::default(),
            view_size: [1, 1],
            frame_pending: Some(Framing::Shown),
            search: String::new(),
            to_studio: None,
            hover: None,
            flight: None,
            anchor: None,
            first_frame: true,
        }
    }
}

impl AtlasTab {
    /// The app settings that affect how the Atlas is drawn.
    pub fn apply_settings(&mut self, anti_aliasing: AntiAliasing, quality: RenderQuality) {
        self.anti_aliasing = anti_aliasing;
        self.quality = quality;
    }

    /// What the artist asked to open in the Studio since last asked: a
    /// model's name and its binary glTF.
    pub fn take_studio_request(&mut self) -> Option<(String, Vec<u8>)> {
        self.to_studio.take()
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
            Ok(atlas) => State::Ready(Box::new(AtlasView::new(atlas))),
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
                    if self.pose_controls.panel(ui, tr, view) {
                        self.anchor = None;
                        self.hover = None;
                    }
                    if ui
                        .button(tr.tr("atlas-open-in-studio"))
                        .on_hover_text(tr.tr("atlas-open-in-studio-hint"))
                        .clicked()
                    {
                        self.to_studio = studio_model(view, tr);
                    }
                    ui.separator();
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
            if let Some(framing) = self.frame_pending.take() {
                let start = view.scene.camera;
                match framing {
                    Framing::Shown => view.frame(aspect),
                    Framing::Selected => view.frame_selected(aspect),
                    Framing::View(index) => {
                        if let Some(preset) = CameraPreset::built_ins().get(index) {
                            view.view_from(preset, aspect);
                        }
                    }
                }
                // Fly there, unless motion is turned off or nothing is on
                // screen yet to fly from.
                let animate = ui.style().animation_time > 0.0;
                if animate && !self.first_frame {
                    self.flight = Some(CameraFlight::new(start, view.scene.camera));
                    view.scene.camera = start;
                }
            }
            self.first_frame = false;
            if let Some(flight) = &mut self.flight {
                let seconds = ui.input(|i| i.stable_dt).min(0.1);
                view.scene.camera = flight.advance(seconds);
                if flight.finished() {
                    self.flight = None;
                } else {
                    ui.ctx().request_repaint();
                }
            }
            let placed = view.scene.camera;
            let (pose_input, pose_changed) =
                self.pose_controls.interact(ui, tr, view, rect, aspect);
            if pose_changed {
                self.anchor = None;
                self.hover = None;
                self.flight = None;
            }
            if !pose_input {
                drive_camera(&mut view.scene.camera, ui, &response, rect);
                if response.double_clicked() {
                    self.frame_pending = Some(Framing::Selected);
                }
            }
            if view.scene.camera != placed {
                // The artist took the camera: stop flying it.
                self.flight = None;
            }
            let ray = |view: &AtlasView, pointer: egui::Pos2| {
                let ndc_x = (pointer.x - rect.left()) / rect.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pointer.y - rect.top()) / rect.height() * 2.0;
                view.scene.camera.ray(ndc_x, ndc_y, aspect)
            };
            // The name under the pointer, looked up again once it moves.
            match response
                .hover_pos()
                .filter(|_| !response.dragged() && !pose_input)
            {
                Some(pointer) => {
                    let now = ui.input(|i| i.time);
                    let stale = self.hover.as_ref().is_none_or(|h| {
                        h.pointer.distance(pointer) > 2.0 && now - h.time > HOVER_INTERVAL
                    });
                    if stale {
                        let (origin, direction) = ray(view, pointer);
                        self.hover = Some(Hover {
                            pointer,
                            time: now,
                            index: view.pick(origin, direction),
                        });
                    }
                    view.hovered = self.hover.as_ref().and_then(|h| h.index);
                    // The selection is named by its label already.
                    if let Some(index) = view.hovered.filter(|&i| view.selected() != Some(i)) {
                        let structure = &view.atlas().structures()[index];
                        let name = display_name(structure, tr);
                        let latin = structure.latin.clone();
                        response.clone().on_hover_ui_at_pointer(|ui| {
                            ui.strong(name);
                            if let Some(latin) = latin {
                                ui.label(egui::RichText::new(latin).italics());
                            }
                        });
                    }
                }
                None => {
                    self.hover = None;
                    view.hovered = None;
                }
            }
            if !ui.ctx().egui_wants_keyboard_input() {
                let (f, h, i, escape) = ui.input(|input| {
                    (
                        input.key_pressed(egui::Key::F),
                        input.key_pressed(egui::Key::H),
                        input.key_pressed(egui::Key::I),
                        input.key_pressed(egui::Key::Escape),
                    )
                });
                if f {
                    self.frame_pending = Some(Framing::Selected);
                }
                if let Some(index) = view.selected() {
                    if h {
                        view.hide(index);
                    }
                    if i {
                        view.isolate([index]);
                        self.frame_pending = Some(Framing::Selected);
                    }
                }
                if escape {
                    if view.selected().is_some() {
                        view.select(None);
                    } else {
                        view.show_all();
                        self.frame_pending = Some(Framing::Shown);
                    }
                }
            }
            if !pose_input
                && response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let (origin, direction) = ray(view, pointer);
                let hit = view.pick_hit(origin, direction);
                view.select(hit.map(|(index, _)| index));
                self.anchor = hit.map(|(index, t)| (index, origin + direction * t));
            }
            let painter = ui.painter_at(rect);
            let preview = self
                .pose_controls
                .previewing(pose_input && ui.input(|i| i.pointer.primary_down()));
            let [r, g, b] = view.scene.room.backdrop_srgb();
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(r, g, b));
            painter.add(eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    frame: Frame {
                        quality: self.quality,
                        ..Frame::from_atlas(view, aspect)
                    },
                    size: if preview {
                        self.view_size.map(|side| side.div_ceil(2))
                    } else {
                        self.view_size
                    },
                    anti_aliasing: if preview {
                        AntiAliasing::OFF
                    } else {
                        self.anti_aliasing
                    },
                },
            ));
            self.pose_controls.paint(&painter, view, rect, aspect);
            if let Some(index) = view.selected() {
                let anchor = match self.anchor {
                    Some((clicked, point)) if clicked == index => point,
                    _ => view.structure_bounds(index).center(),
                };
                let screen = view_projection(view, aspect, rect, anchor);
                if let Some(screen) = screen
                    && let Some(action) = callout(ui, tr, view, index, screen, rect)
                {
                    match action {
                        CalloutAction::Isolate => {
                            view.isolate([index]);
                            self.frame_pending = Some(Framing::Selected);
                        }
                        CalloutAction::Hide => view.hide(index),
                        CalloutAction::Frame => self.frame_pending = Some(Framing::Selected),
                        CalloutAction::Close => view.select(None),
                    }
                    ui.ctx().request_repaint();
                }
            }
        });
    }
}

/// Where a point of the body falls in the viewport, or `None` behind the
/// eye.
fn view_projection(
    view: &AtlasView,
    aspect: f32,
    rect: egui::Rect,
    point: Vec3,
) -> Option<egui::Pos2> {
    let clip = view.scene.camera.view_projection(aspect) * point.extend(1.0);
    if clip.w <= 1e-6 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(egui::pos2(
        rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(),
        rect.top() + (0.5 - ndc.y * 0.5) * rect.height(),
    ))
}

/// What the selection's label was asked to do.
enum CalloutAction {
    Isolate,
    Hide,
    Frame,
    Close,
}

/// How far the label sits from the spot it names, points: up and to the
/// side, clear of the structure itself.
const CALLOUT_OFFSET: egui::Vec2 = egui::vec2(72.0, -56.0);

/// The selection's name on a label pinned to the body by a leader line, so
/// the name is read where the eye already is, with what to do with it.
fn callout(
    ui: &mut egui::Ui,
    tr: &Localizer,
    view: &AtlasView,
    index: StructureIndex,
    anchor: egui::Pos2,
    rect: egui::Rect,
) -> Option<CalloutAction> {
    let structure = &view.atlas().structures()[index];
    // To the right of the spot, unless that runs off the viewport.
    let right = anchor.x + CALLOUT_OFFSET.x + 220.0 < rect.right();
    let offset = if right {
        CALLOUT_OFFSET
    } else {
        egui::vec2(-CALLOUT_OFFSET.x, CALLOUT_OFFSET.y)
    };
    let pivot = if right {
        egui::Align2::LEFT_CENTER
    } else {
        egui::Align2::RIGHT_CENTER
    };
    let mut action = None;
    let shown = egui::Area::new(egui::Id::unique("atlas_callout"))
        .order(egui::Order::Foreground)
        .constrain_to(rect.shrink(8.0))
        .pivot(pivot)
        .fixed_pos(anchor + offset)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(260.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(display_name(structure, tr))
                                .strong()
                                .size(15.0),
                        )
                        .wrap(),
                    );
                    if ui
                        .small_button("×")
                        .on_hover_text(tr.tr("atlas-callout-close"))
                        .clicked()
                    {
                        action = Some(CalloutAction::Close);
                    }
                });
                if let Some(latin) = &structure.latin {
                    ui.label(egui::RichText::new(latin).italics().weak());
                }
                if let Some(mass) = mass_of(structure) {
                    ui.weak(tr.tr_args(
                        "atlas-mass-of",
                        &[("mass", tr.tr(&mass_message_id(mass)).into())],
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.small_button(tr.tr("atlas-frame-selected")).clicked() {
                        action = Some(CalloutAction::Frame);
                    }
                    if ui.small_button(tr.tr("atlas-isolate")).clicked() {
                        action = Some(CalloutAction::Isolate);
                    }
                    if ui.small_button(tr.tr("atlas-hide")).clicked() {
                        action = Some(CalloutAction::Hide);
                    }
                });
            });
        });
    // The leader runs from the spot to the near side of the label.
    let label = shown.response.rect;
    let end = if label.center().x >= anchor.x {
        label.left_center()
    } else {
        label.right_center()
    };
    let painter = ui.painter_at(rect);
    // Dark ink with a light rim, like the outlines: it reads on the page
    // and on any colour of the body.
    let ink = egui::Color32::from_rgb(40, 30, 26);
    let rim = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 170);
    painter.line_segment([anchor, end], egui::Stroke::new(3.0, rim));
    painter.line_segment([anchor, end], egui::Stroke::new(1.5, ink));
    painter.circle(anchor, 3.5, ink, egui::Stroke::new(1.5, rim));
    action
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
    view.emphasis = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr.tr("atlas-quick-muscles")).clicked() {
            view.show_all();
            view.set_layer(Layer::Bones, LayerState::SHOWN);
            view.set_layer(Layer::Muscles, LayerState::SHOWN);
            view.set_layer(Layer::DeepMuscles, LayerState::HIDDEN);
            view.colors = AtlasColors::Masses;
            framing = Some(Framing::Shown);
        }
        if ui.button(tr.tr("atlas-quick-skeleton")).clicked() {
            view.show_all();
            view.set_layer(Layer::Bones, LayerState::SHOWN);
            view.set_layer(Layer::Muscles, LayerState::HIDDEN);
            view.set_layer(Layer::DeepMuscles, LayerState::HIDDEN);
            view.colors = AtlasColors::Natural;
            framing = Some(Framing::Shown);
        }
        if ui.button(tr.tr("atlas-quick-skull")).clicked() {
            view.isolate(view.skull());
            view.colors = AtlasColors::Bones;
            framing = Some(Framing::View(0));
        }
    });
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
                select_list_entry(view, index);
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

    section(ui, tr, "atlas-views-title", true, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (index, preset) in CameraPreset::built_ins().iter().enumerate() {
                let label = tr.tr(&format!("camera-preset-{}", preset.name()));
                if ui.button(label).clicked() {
                    framing = Some(Framing::View(index));
                }
            }
            if ui.button(tr.tr("atlas-frame")).clicked() {
                framing = Some(Framing::Shown);
            }
        });
    });
    if view.colors == AtlasColors::Masses {
        section(ui, tr, "atlas-masses", true, |ui| {
            legend(ui, tr, view, &mut framing)
        });
    }
    section(ui, tr, "atlas-layers", true, |ui| {
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
    });
    section(ui, tr, "atlas-look", false, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (colors, id) in [
                (AtlasColors::Masses, "atlas-color-masses"),
                (AtlasColors::Bones, "atlas-color-bones"),
                (AtlasColors::Natural, "atlas-color-natural"),
                (AtlasColors::Clay, "color-clay"),
            ] {
                ui.selectable_value(&mut view.colors, colors, tr.tr(id));
            }
        });
        ui.checkbox(&mut view.show_fibres, tr.tr("atlas-fibres"));
        ui.checkbox(&mut view.fade_others, tr.tr("atlas-fade-others"));
        let shading = &mut view.scene.shading;
        ui.checkbox(&mut shading.lines.contours, tr.tr("atlas-outlines"));
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut shading.mode,
                ShadingMode::Solid,
                tr.tr("atlas-light-even"),
            );
            ui.selectable_value(
                &mut shading.mode,
                ShadingMode::Lit,
                tr.tr("atlas-light-shadows"),
            );
        });
        if shading.mode == ShadingMode::Lit {
            ui.checkbox(
                &mut shading.ambient_occlusion.enabled,
                tr.tr("studio-ambient-occlusion"),
            );
        }
    });
    section(ui, tr, "atlas-structures", false, |ui| {
        structure_list(ui, tr, view, &mut framing);
    });
    ui.separator();
    ui.weak(tr.tr("atlas-keys"));
    ui.weak(tr.tr("atlas-credit"));
    framing
}

/// A collapsible part of the panel, open or closed at first.
fn section(
    ui: &mut egui::Ui,
    tr: &Localizer,
    title: &str,
    open: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::CollapsingHeader::new(egui::RichText::new(tr.tr(title)).strong())
        .id_salt(("atlas-section", title.to_owned()))
        .default_open(open)
        .show(ui, add_contents);
}

/// The masses with their colours; a click shows only that mass.
fn legend(ui: &mut egui::Ui, tr: &Localizer, view: &mut AtlasView, framing: &mut Option<Framing>) {
    ui.weak(tr.tr("atlas-masses-hint"));
    for mass in MASSES {
        ui.horizontal(|ui| {
            let [r, g, b] = mass.srgb;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 3.0, egui::Color32::from_rgb(r, g, b));
            let link = ui.link(tr.tr(&mass_message_id(mass)));
            if link.hovered() {
                view.emphasis = Some(mass);
            }
            if link.clicked() {
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
                            select_list_entry(view, index);
                            *framing = Some(Framing::Selected);
                        }
                    });
                }
            });
    }
}

fn select_list_entry(view: &mut AtlasView, index: StructureIndex) {
    if Layer::of(&view.atlas().structures()[index]) == Layer::DeepMuscles
        && !view.layer(Layer::DeepMuscles).visible
    {
        view.isolate([index]);
    }
    view.select(Some(index));
}

/// The selection, or what is shown, as a model for the Studio, named for
/// what it is.
fn studio_model(view: &AtlasView, tr: &Localizer) -> Option<(String, Vec<u8>)> {
    let what = match view.selected() {
        Some(index) => display_name(&view.atlas().structures()[index], tr),
        None if view.is_isolating() && view.shown().count() == view.skull().len() => {
            tr.tr("atlas-model-skull")
        }
        None => tr.tr("atlas-model-shown"),
    };
    let name = tr.tr_args("atlas-model-name", &[("what", what.into())]);
    let model = view.to_model(&name)?;
    Some((
        name,
        houdon_core::atlas::to_glb(&model, houdon_core::atlas::ATTRIBUTION),
    ))
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
        Layer::DeepMuscles => "atlas-layer-deep-muscles",
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
