//! How the Studio colours objects: their own materials, Clay or a random
//! colour per object.

use glam::Vec3;

use crate::ObjectId;
use crate::matcap::Matcap;

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

/// How surfaces are lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShadingMode {
    /// A fixed light that ignores the Scene's Lights, to read form alone.
    Solid,
    /// The Scene's Lights, shadows and Room.
    #[default]
    Lit,
}

/// The light of the Solid mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SolidLighting {
    /// A soft studio light that turns with the view.
    #[default]
    Studio,
    /// Colour from a matcap, a picture of a lit sphere.
    Matcap,
    /// No light at all: one flat colour per object.
    Flat,
}

impl SolidLighting {
    pub const ALL: [Self; 3] = [Self::Studio, Self::Matcap, Self::Flat];
}

/// The cavity overlay: brightens ridges and darkens hollows so small
/// changes of form read, in Solid and Lit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cavity {
    /// From the mesh's curvature: stable while the camera moves.
    pub geometry: bool,
    pub geometry_ridge: f32,
    pub geometry_hollow: f32,
    /// From the picture's normals: fine detail at any distance.
    pub screen: bool,
    pub screen_ridge: f32,
    pub screen_hollow: f32,
}

impl Cavity {
    /// The largest ridge or hollow strength.
    pub const MAX_STRENGTH: f32 = 2.0;

    pub fn is_on(&self) -> bool {
        self.geometry || self.screen
    }
}

impl Default for Cavity {
    fn default() -> Self {
        Self {
            geometry: false,
            geometry_ridge: 1.0,
            geometry_hollow: 1.0,
            screen: false,
            screen_ridge: 1.0,
            screen_hollow: 1.0,
        }
    }
}

/// Line overlays for studying the drawing of a form, in Solid and Lit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lines {
    /// Contour lines: silhouettes and the inner folds of a form.
    pub contours: bool,
    /// Line width, pixels.
    pub thickness: f32,
    /// Linear RGB.
    pub color: Vec3,
    /// 0 draws only outlines and the sharpest folds, 1 every soft fold.
    pub fold_sensitivity: f32,
    /// The mesh's edges, to see the planes of a planar model.
    pub wireframe: bool,
    pub wireframe_color: Vec3,
    pub wireframe_opacity: f32,
}

impl Lines {
    pub const MAX_THICKNESS: f32 = 4.0;
}

impl Default for Lines {
    fn default() -> Self {
        Self {
            contours: false,
            thickness: 1.0,
            color: Vec3::ZERO,
            fold_sensitivity: 0.3,
            wireframe: false,
            wireframe_color: Vec3::ZERO,
            wireframe_opacity: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shading {
    pub mode: ShadingMode,
    pub solid: SolidLighting,
    /// The matcap of [`SolidLighting::Matcap`]; without one, Studio light.
    pub matcap: Option<Matcap>,
    /// Off by default.
    pub cavity: Cavity,
    /// Off by default.
    pub lines: Lines,
    /// Hide the back of surfaces, as if every object were closed and seen
    /// from outside. Off by default: imported meshes are often open.
    pub backface_culling: bool,
    pub color: ColorSource,
    /// The Clay tone, linear RGB.
    pub clay_color: Vec3,
    /// A value study laid over the finished picture.
    pub value: ValueStudy,
}

impl Default for Shading {
    fn default() -> Self {
        Self {
            mode: ShadingMode::default(),
            solid: SolidLighting::default(),
            matcap: None,
            cavity: Cavity::default(),
            lines: Lines::default(),
            backface_culling: false,
            color: ColorSource::default(),
            clay_color: DEFAULT_CLAY_COLOR,
            value: ValueStudy::default(),
        }
    }
}

/// How the finished picture is turned into a value study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ValueMode {
    /// The picture as it is.
    #[default]
    Off,
    /// Colour removed, lightness kept.
    Grey,
    /// Flat tones by lightness: 2 = light and shadow, 3 adds a half-tone,
    /// 4 and 5 add highlights and accents.
    Tones(u8),
}

impl ValueMode {
    pub const MIN_TONES: u8 = 2;
    pub const MAX_TONES: u8 = 5;
}

/// A value study: the way artists squint at a model to see the big masses
/// of light and shadow before the details.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueStudy {
    pub mode: ValueMode,
    /// Moves where one tone turns into the next, -1 to 1: positive puts
    /// more of the picture into the lighter tones.
    pub balance: f32,
    /// Blur like squinting, 0 (none) to 1 (strong).
    pub squint: f32,
    /// The mode `toggle` turns back on: the last one that was not `Off`.
    pub last_mode: ValueMode,
}

impl ValueMode {
    /// The study mode the toggle starts with: light, half-tone and shadow.
    pub const DEFAULT_ON: Self = Self::Tones(3);
}

impl Default for ValueStudy {
    fn default() -> Self {
        Self {
            mode: ValueMode::Off,
            balance: 0.0,
            squint: 0.0,
            last_mode: ValueMode::DEFAULT_ON,
        }
    }
}

impl ValueStudy {
    /// Whether the picture needs the value pass at all.
    pub fn is_active(&self) -> bool {
        self.mode != ValueMode::Off || self.squint > 0.0
    }

    /// Chooses a mode and remembers it for `toggle` unless it is `Off`.
    pub fn set_mode(&mut self, mode: ValueMode) {
        self.mode = mode;
        if mode != ValueMode::Off {
            self.last_mode = mode;
        }
    }

    /// Turns the study off, or back on in the mode last used.
    pub fn toggle(&mut self) {
        if self.mode == ValueMode::Off {
            self.mode = self.last_mode;
        } else {
            self.set_mode(ValueMode::Off);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toggle_brings_back_the_mode_last_used() {
        let mut study = ValueStudy::default();
        study.toggle();
        assert_eq!(study.mode, ValueMode::DEFAULT_ON);

        study.set_mode(ValueMode::Grey);
        study.toggle();
        assert_eq!(study.mode, ValueMode::Off);
        study.toggle();
        assert_eq!(study.mode, ValueMode::Grey);
    }

    #[test]
    fn choosing_off_does_not_forget_the_last_mode() {
        let mut study = ValueStudy::default();
        study.set_mode(ValueMode::Tones(2));
        study.set_mode(ValueMode::Off);
        study.toggle();
        assert_eq!(study.mode, ValueMode::Tones(2));
    }
}
