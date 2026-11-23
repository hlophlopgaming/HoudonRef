//! Light masks: images a light shines through, like window bars, foliage or
//! water caustics. A spot light stretches its mask over its cone; a
//! directional light repeats it across the scene, one copy per pattern size.

use std::sync::Arc;

/// How big one copy of a directional light's mask is by default, metres.
pub const DEFAULT_PATTERN_SIZE: f32 = 2.0;

/// A mask as brightness, 0 (blocks the light) to 255 (lets it through),
/// rows top to bottom.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MaskImage {
    pub width: u32,
    pub height: u32,
    pub luminance: Vec<u8>,
}

impl std::fmt::Debug for MaskImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MaskImage({}×{})", self.width, self.height)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("not a PNG or JPEG image: {0}")]
pub struct MaskError(#[from] image::ImageError);

impl MaskImage {
    /// Reads a PNG or JPEG file. Colour becomes brightness; transparent
    /// parts block the light.
    pub fn decode(bytes: &[u8]) -> Result<Self, MaskError> {
        let image = image::load_from_memory(bytes)?.into_rgba8();
        let (width, height) = image.dimensions();
        let luminance = image
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0.map(f32::from);
                let grey = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                (grey * a / 255.0).round() as u8
            })
            .collect();
        Ok(Self {
            width,
            height,
            luminance,
        })
    }

    /// A mask drawn by `brightness(u, v)`, with u and v from 0 to 1.
    pub fn from_fn(width: u32, height: u32, brightness: impl Fn(f32, f32) -> u8) -> Self {
        let luminance = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                brightness(
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                )
            })
            .collect();
        Self {
            width,
            height,
            luminance,
        }
    }

    /// Brightness at `(x, y)`.
    pub fn at(&self, x: u32, y: u32) -> u8 {
        self.luminance[(y * self.width + x) as usize]
    }

    /// The same picture at `size` × `size`, sampled bilinearly. The file
    /// itself is never changed; this is only for the GPU's fixed layer size.
    pub fn resampled(&self, size: u32) -> Vec<u8> {
        if self.width == size && self.height == size {
            return self.luminance.clone();
        }
        let sample = |x: f32, y: f32| {
            let x = (x - 0.5).clamp(0.0, (self.width - 1) as f32);
            let y = (y - 0.5).clamp(0.0, (self.height - 1) as f32);
            let (x0, y0) = (x.floor() as u32, y.floor() as u32);
            let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
            let (fx, fy) = (x.fract(), y.fract());
            let lerp = |a: u8, b: u8, t: f32| f32::from(a) * (1.0 - t) + f32::from(b) * t;
            let top = lerp(self.at(x0, y0), self.at(x1, y0), fx);
            let bottom = lerp(self.at(x0, y1), self.at(x1, y1), fx);
            (top * (1.0 - fy) + bottom * fy).round() as u8
        };
        let (sx, sy) = (
            self.width as f32 / size as f32,
            self.height as f32 / size as f32,
        );
        (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .map(|(x, y)| sample((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy))
            .collect()
    }
}

/// The mask a light shines through.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LightMask {
    /// Where it came from: a built-in mask's name or the user's file name.
    pub name: String,
    pub image: Arc<MaskImage>,
    /// For a directional light, the size of one copy of the mask, metres.
    pub pattern_size: f32,
}

impl LightMask {
    pub fn new(name: impl Into<String>, image: Arc<MaskImage>) -> Self {
        Self {
            name: name.into(),
            image,
            pattern_size: DEFAULT_PATTERN_SIZE,
        }
    }
}

impl PartialEq for LightMask {
    /// The same mask: the same image (not just equal pixels) and settings.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.pattern_size == other.pattern_size
            && Arc::ptr_eq(&self.image, &other.image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        bytes
    }
    use image::ImageEncoder;

    #[test]
    fn white_lets_light_through_and_black_or_transparent_blocks_it() {
        let bytes = png(3, 1, &[255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 0]);
        let mask = MaskImage::decode(&bytes).unwrap();
        assert_eq!((mask.width, mask.height), (3, 1));
        assert_eq!(mask.luminance, vec![255, 0, 0]);
    }

    #[test]
    fn a_file_that_is_not_an_image_is_an_error() {
        assert!(MaskImage::decode(b"not an image").is_err());
    }

    #[test]
    fn resampling_keeps_the_picture() {
        let half = MaskImage::from_fn(4, 4, |u, _| if u < 0.5 { 0 } else { 255 });
        let big = half.resampled(8);
        assert_eq!(big.len(), 64);
        assert_eq!(big[0], 0, "left stays dark");
        assert_eq!(big[7], 255, "right stays light");
        assert_eq!(half.resampled(4), half.luminance, "same size is unchanged");
    }

    #[test]
    fn masks_are_the_same_only_with_the_same_image() {
        let image = Arc::new(MaskImage::from_fn(2, 2, |_, _| 255));
        let a = LightMask::new("window", image.clone());
        assert_eq!(a, LightMask::new("window", image));
        let copy = Arc::new(MaskImage::from_fn(2, 2, |_, _| 255));
        assert_ne!(a, LightMask::new("window", copy));
    }
}
