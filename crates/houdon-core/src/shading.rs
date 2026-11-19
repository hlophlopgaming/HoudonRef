//! How the Studio colours objects: their own materials, Clay or a random
//! colour per object.

use glam::Vec3;

use crate::ObjectId;

/// Where an object's colour comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorSource {
    /// The model's own materials; models without materials show as Clay.
    #[default]
    Material,
    /// One plain colour for every object, to study form and light alone.
    Clay,
    /// A different plain colour per object, to tell objects apart.
    Random,
}

impl ColorSource {
    pub const ALL: [Self; 3] = [Self::Material, Self::Clay, Self::Random];
}

/// Neutral light-grey clay, linear.
pub const DEFAULT_CLAY_COLOR: Vec3 = Vec3::new(0.62, 0.6, 0.57);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shading {
    pub color: ColorSource,
    /// The Clay tone, linear RGB.
    pub clay_color: Vec3,
}

impl Default for Shading {
    fn default() -> Self {
        Self {
            color: ColorSource::default(),
            clay_color: DEFAULT_CLAY_COLOR,
        }
    }
}

/// What covers an object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Surface {
    /// The model's materials, part by part.
    Materials,
    /// One plain colour, linear RGB.
    Plain(Vec3),
}

/// A pleasant colour of its own for an object: hues spread by the golden
/// ratio, so neighbours in the list differ clearly. Linear RGB.
pub fn random_color(id: ObjectId) -> Vec3 {
    let hue = (id.0 as f32 * 0.618_034).fract();
    let (saturation, value) = (0.45, 0.8);
    let h = hue * 6.0;
    let c = value * saturation;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    // sRGB-ish to linear, near enough for a label colour.
    Vec3::new(r + m, g + m, b + m).powf(2.2)
}
