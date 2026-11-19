//! The built-in matcaps (Blender's set, CC0), used exactly as Blender ships
//! them, and their picker thumbnails.

use std::collections::HashMap;
use std::sync::Arc;

use houdon_core::MatcapImage;

/// Includes matcaps from `assets/matcaps/blender` by name.
#[allow(unused_macros)]
macro_rules! blender {
    ($($name:literal),* $(,)?) => {
        &[$((
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/matcaps/blender/",
                $name,
                ".exr"
            )) as &[u8],
        )),*]
    };
}

/// Every built-in matcap, by name.
pub const BUILT_IN: &[(&str, &[u8])] = &[];

/// Side of the picker's thumbnails, pixels.
const THUMBNAIL_SIZE: usize = 64;

/// Matcaps decoded so far and their thumbnails.
#[derive(Default)]
pub struct MatcapLibrary {
    images: HashMap<&'static str, Arc<MatcapImage>>,
    thumbnails: HashMap<&'static str, egui::TextureHandle>,
}

impl MatcapLibrary {
    /// The built-in matcap `name`.
    pub fn image(&mut self, name: &'static str) -> Option<Arc<MatcapImage>> {
        if let Some(image) = self.images.get(name) {
            return Some(image.clone());
        }
        let (_, bytes) = BUILT_IN.iter().find(|(n, _)| *n == name)?;
        match MatcapImage::decode(bytes) {
            Ok(image) => {
                let image = Arc::new(image);
                self.images.insert(name, image.clone());
                Some(image)
            }
            Err(error) => {
                log::error!("matcap {name} is unreadable: {error}");
                None
            }
        }
    }

    /// The picker thumbnail of `name`.
    pub fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        name: &'static str,
    ) -> Option<egui::TextureHandle> {
        if let Some(handle) = self.thumbnails.get(name) {
            return Some(handle.clone());
        }
        let image = self.image(name)?;
        let handle = ctx.load_texture(
            format!("matcap-{name}"),
            thumbnail(&image),
            egui::TextureOptions::LINEAR,
        );
        self.thumbnails.insert(name, handle.clone());
        Some(handle)
    }
}

/// A small sRGB picture of a matcap, for the picker only.
fn thumbnail(image: &MatcapImage) -> egui::ColorImage {
    let encode = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    let pixels = (0..THUMBNAIL_SIZE * THUMBNAIL_SIZE)
        .map(|i| {
            let (x, y) = (i % THUMBNAIL_SIZE, i / THUMBNAIL_SIZE);
            let sx = x * image.width as usize / THUMBNAIL_SIZE;
            let sy = y * image.height as usize / THUMBNAIL_SIZE;
            let at = (sy * image.width as usize + sx) * 4;
            let [r, g, b] = [0, 1, 2].map(|c| encode(image.rgba[at + c]));
            egui::Color32::from_rgb(r, g, b)
        })
        .collect();
    egui::ColorImage::new([THUMBNAIL_SIZE, THUMBNAIL_SIZE], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_matcaps_decode() {
        let mut library = MatcapLibrary::default();
        for (name, _) in BUILT_IN {
            assert!(library.image(name).is_some(), "{name}");
        }
    }

    #[test]
    fn thumbnails_are_small_srgb_pictures() {
        let image = MatcapImage::from_fn(128, 128, |_, _| [1.0, 0.2158, 0.0]);
        let picture = thumbnail(&image);
        assert_eq!(picture.size, [THUMBNAIL_SIZE, THUMBNAIL_SIZE]);
        let p = picture.pixels[0];
        assert_eq!((p.r(), p.b()), (255, 0));
        assert!(p.g().abs_diff(128) <= 1, "{}", p.g());
    }
}
