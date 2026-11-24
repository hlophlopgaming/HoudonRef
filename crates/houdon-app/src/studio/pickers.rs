//! The matcap and light mask pickers, and the images they put on the scene.

use super::*;

impl Studio {
    /// The window to pick the Solid mode's matcap.
    pub(super) fn matcap_picker(&mut self, ctx: &egui::Context, tr: &Localizer) {
        let mut open = true;
        egui::Window::new(tr.tr("matcap-picker"))
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for &(name, _) in matcaps::BUILT_IN {
                                let Some(texture) = self.matcaps.thumbnail(ctx, name) else {
                                    continue;
                                };
                                let button = egui::Button::image(
                                    egui::Image::new(&texture)
                                        .fit_to_exact_size(egui::vec2(64.0, 64.0)),
                                );
                                if ui.add(button).on_hover_text(name).clicked() {
                                    chosen = Some(name);
                                }
                            }
                        });
                    });
                ui.separator();
                if ui.button(tr.tr("matcap-open-file")).clicked() {
                    self.pick_file(ctx, &tr.tr("image-file-type"), FileKind::Matcap);
                }
                ui.weak(tr.tr("matcap-picker-hint"));
                ui.weak(tr.tr("matcap-credit"));
                if let Some(name) = chosen
                    && let Some(image) = self.matcaps.image(name)
                {
                    self.set_matcap(name.to_owned(), image);
                }
            });
        self.matcap_picker_open = open;
    }

    pub(super) fn set_matcap(&mut self, name: String, image: Arc<MatcapImage>) {
        let shading = &mut self.scene.shading;
        shading.matcap = Some(Matcap { name, image });
        shading.mode = ShadingMode::Solid;
        shading.solid = SolidLighting::Matcap;
    }

    /// Uses a dropped image as the Solid mode's matcap.
    pub(super) fn load_matcap_file(&mut self, name: &str, bytes: &[u8], tr: &Localizer) {
        match MatcapImage::decode(bytes) {
            Ok(image) => {
                self.set_matcap(name.to_owned(), Arc::new(image));
                self.message = None;
            }
            Err(error) => {
                log::warn!("{name}: {error}");
                self.message = Some(tr.tr_args(
                    "error-matcap-unreadable",
                    &[("name", name.to_owned().into())],
                ));
            }
        }
    }

    /// The window to pick a light mask for the selected light.
    pub(super) fn mask_picker(&mut self, ctx: &egui::Context, tr: &Localizer) {
        let mut open = true;
        egui::Window::new(tr.tr("mask-picker"))
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for category in masks::Category::ALL {
                        ui.selectable_value(
                            &mut self.mask_category,
                            category,
                            tr.tr(category.message_id()),
                        );
                    }
                });
                ui.separator();
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let names = masks::ALL
                                .iter()
                                .filter(|name| masks::Category::of(name) == self.mask_category);
                            for &name in names {
                                let size = egui::vec2(64.0, 64.0);
                                let response = match self.masks.thumbnail(ctx, name) {
                                    Some(texture) => ui.add(egui::Button::image(
                                        egui::Image::new(&texture).fit_to_exact_size(size),
                                    )),
                                    None if self.masks.failed(name) => ui.add_sized(
                                        size,
                                        egui::Button::new(tr.tr("mask-unavailable")),
                                    ),
                                    None => ui.add_sized(size, egui::Spinner::new()),
                                };
                                if response.on_hover_text(name).clicked() {
                                    chosen = Some(name);
                                }
                            }
                        });
                    });
                ui.separator();
                if ui.button(tr.tr("mask-open-file")).clicked() {
                    self.pick_file(ctx, &tr.tr("image-file-type"), FileKind::Mask);
                }
                ui.weak(tr.tr("mask-picker-hint"));
                ui.weak(tr.tr("mask-credit"));
                if let (Some(name), Some(light)) = (chosen, self.selected_light) {
                    self.choose_mask(ctx, light, name);
                }
            });
        self.mask_picker_open = open;
    }

    /// Gives `light` the built-in mask `name`, now or once it has loaded.
    pub(super) fn choose_mask(&mut self, ctx: &egui::Context, light: LightId, name: &'static str) {
        match self.masks.image(ctx, name) {
            Some(image) => self.set_mask(light, name.to_owned(), image),
            None => self.pending_mask = Some((light, name)),
        }
    }

    /// Puts a mask image on a light, keeping the pattern size it had.
    pub(super) fn set_mask(&mut self, id: LightId, name: String, image: Arc<MaskImage>) {
        let Some(mut light) = self.scene.light(id).cloned() else {
            return;
        };
        if !light.kind.takes_mask() {
            return;
        }
        let pattern_size = light
            .mask
            .as_ref()
            .map_or(DEFAULT_PATTERN_SIZE, |m| m.pattern_size);
        light.mask = Some(LightMask {
            pattern_size,
            ..LightMask::new(name, image)
        });
        self.scene.set_light(light);
    }

    /// Uses a dropped image as the selected light's mask.
    pub(super) fn load_mask_file(&mut self, name: &str, bytes: &[u8], tr: &Localizer) {
        let image = match MaskImage::decode(bytes) {
            Ok(image) => Arc::new(image),
            Err(error) => {
                log::warn!("{name}: {error}");
                self.message =
                    Some(tr.tr_args("error-mask-unreadable", &[("name", name.to_owned().into())]));
                return;
            }
        };
        let light = self
            .selected_light
            .filter(|id| self.scene.light(*id).is_some_and(|l| l.kind.takes_mask()));
        match light {
            Some(id) => {
                self.set_mask(id, name.to_owned(), image);
                self.message = None;
            }
            None => self.message = Some(tr.tr("error-mask-needs-light")),
        }
    }
}
