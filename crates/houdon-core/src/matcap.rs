//! Matcaps: pictures of a lit sphere. In the Solid shading mode a surface
//! takes its colour from the point of the sphere that faces the same way,
//! so form reads the same whatever the Scene's lights do.

use std::sync::Arc;

/// A matcap as linear RGBA, rows top to bottom.
#[derive(Clone, PartialEq)]
pub struct MatcapImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<f32>,
}

impl std::fmt::Debug for MatcapImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MatcapImage({}×{})", self.width, self.height)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("not an EXR, PNG or JPEG image: {0}")]
pub struct MatcapError(#[from] image::ImageError);

impl MatcapImage {
    /// Reads an EXR (linear, as Blender ships its matcaps) or a PNG or JPEG
    /// (sRGB) file.
    pub fn decode(bytes: &[u8]) -> Result<Self, MatcapError> {
        let image = image::load_from_memory(bytes)?;
        let linear = matches!(
            image,
            image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_)
        );
        let image = image.into_rgba32f();
        let (width, height) = image.dimensions();
        let mut rgba = image.into_raw();
        if !linear {
            for pixel in rgba.as_chunks_mut::<4>().0 {
                for channel in &mut pixel[..3] {
                    *channel = srgb_to_linear(*channel);
                }
            }
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// A matcap drawn by `color(u, v)`, linear RGB, with u and v from 0 to 1.
    pub fn from_fn(width: u32, height: u32, color: impl Fn(f32, f32) -> [f32; 3]) -> Self {
        let rgba = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let [r, g, b] = color(
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                );
                [r, g, b, 1.0]
            })
            .collect();
        Self {
            width,
            height,
            rgba,
        }
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The matcap of the Solid mode.
#[derive(Debug, Clone)]
pub struct Matcap {
    /// A built-in matcap's name or the user's file name.
    pub name: String,
    pub image: Arc<MatcapImage>,
}

impl PartialEq for Matcap {
    /// The same matcap: the same image, not just equal pixels.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && Arc::ptr_eq(&self.image, &other.image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn a_png_matcap_is_turned_into_linear_colour() {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&[255, 128, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        let matcap = MatcapImage::decode(&bytes).unwrap();
        let [r, g, b, a] = matcap.rgba[..4] else {
            panic!()
        };
        assert!((r - 1.0).abs() < 1e-3 && b.abs() < 1e-3 && (a - 1.0).abs() < 1e-3);
        assert!((g - 0.2158).abs() < 2e-3, "sRGB 128 is linear 0.216: {g}");
    }

    #[test]
    fn an_exr_matcap_stays_linear() {
        let pixels = image::Rgba32FImage::from_raw(1, 1, vec![2.0, 0.5, 0.25, 1.0]).unwrap();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba32F(pixels)
            .write_to(&mut bytes, image::ImageFormat::OpenExr)
            .unwrap();
        let matcap = MatcapImage::decode(bytes.get_ref()).unwrap();
        assert_eq!(matcap.rgba, vec![2.0, 0.5, 0.25, 1.0]);
    }

    #[test]
    fn a_file_that_is_not_an_image_is_an_error() {
        assert!(MatcapImage::decode(b"no").is_err());
    }
}
