//! Saving the view as a PNG: rendered offscreen without the interface, at
//! the viewport's size, double it, or 4096 pixels on the long side.

use houdon_render::{ExportSize, Image, OffscreenRenderer, PendingImage};

use super::*;

/// The exporter's GPU side, made on first use on the app's own device.
#[derive(Default)]
pub(super) struct Exporter {
    device: Option<(
        eframe::egui_wgpu::wgpu::Device,
        eframe::egui_wgpu::wgpu::Queue,
    )>,
    renderer: Option<OffscreenRenderer>,
    pending: Option<PendingImage>,
}

impl Exporter {
    pub(super) fn set_device(&mut self, render_state: &eframe::egui_wgpu::RenderState) {
        self.device = Some((render_state.device.clone(), render_state.queue.clone()));
    }

    pub(super) fn is_busy(&self) -> bool {
        self.pending.is_some()
    }
}

impl Studio {
    /// Starts rendering the view at `size`; [`Studio::poll_export`] saves it.
    pub(super) fn export_picture(&mut self, size: ExportSize) {
        let exporter = &mut self.exporter;
        if exporter.pending.is_some() {
            return;
        }
        let Some((device, queue)) = exporter.device.clone() else {
            return;
        };
        let renderer = exporter
            .renderer
            .get_or_insert_with(|| OffscreenRenderer::from_device(device, queue));
        let [width, height] = size.pixels(self.view_size, renderer.max_side());
        let frame = Frame::from_scene(&self.scene, width as f32 / height as f32);
        exporter.pending = Some(renderer.start(&frame, width, height, AntiAliasing::EXPORT));
    }

    /// Saves the exported picture once the GPU has it.
    pub(super) fn poll_export(&mut self, ctx: &egui::Context, tr: &Localizer) {
        let exporter = &mut self.exporter;
        let (Some(pending), Some(renderer)) = (&exporter.pending, &exporter.renderer) else {
            return;
        };
        let Some(result) = renderer.poll(pending) else {
            ctx.request_repaint();
            return;
        };
        exporter.pending = None;
        let saved = result.and_then(|image| encode_png(&image)).and_then(|png| {
            scene_files::write_file(&png, &tr.tr("png-file-type"), "png", "houdonref.png")
        });
        if let Err(reason) = saved {
            self.message = Some(tr.tr_args("export-error", &[("reason", reason.into())]));
        }
    }
}

fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &image.rgba,
        image.width,
        image.height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| error.to_string())?;
    Ok(png)
}

/// The Scene menu's items for saving the view as a picture.
pub(super) fn export_size_message_id(size: ExportSize) -> &'static str {
    match size {
        ExportSize::Window => "export-window",
        ExportSize::Double => "export-double",
        ExportSize::Large => "export-large",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exported_picture_is_a_png_of_its_size() {
        let image = Image {
            width: 3,
            height: 2,
            rgba: vec![200; 3 * 2 * 4],
        };
        let png = encode_png(&image).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
    }
}
