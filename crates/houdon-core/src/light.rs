//! Lights of the Studio scene.

use glam::Vec3;

use crate::mask::LightMask;

/// Identifies a light in a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LightId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightKind {
    /// Parallel rays from far away, like the sun. No falloff.
    Directional,
    /// A bulb: light from one point in every direction, fading with distance.
    Point,
    /// A point light limited to a cone aimed at the scene centre, like on a stage.
    Spot,
}

impl LightKind {
    pub const ALL: [Self; 3] = [Self::Directional, Self::Point, Self::Spot];

    /// Whether the light has a position, and so a distance and falloff.
    pub fn is_positional(self) -> bool {
        !matches!(self, Self::Directional)
    }

    /// Whether the light can shine through a mask: a spot light over its
    /// cone, a directional light repeated across the scene.
    pub fn takes_mask(self) -> bool {
        matches!(self, Self::Directional | Self::Spot)
    }
}

/// One light. Placed around the scene centre by angles and distance, so it
/// stays in place relative to the objects as they move.
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub id: LightId,
    pub kind: LightKind,
    /// Linear RGB.
    pub color: Vec3,
    /// For a directional light, the brightness of a surface facing it. For
    /// point and spot lights, the same at one metre; it falls off with the
    /// square of the distance.
    pub intensity: f32,
    pub enabled: bool,
    /// Whether the light casts shadows of the objects.
    pub casts_shadows: bool,
    /// Around the vertical axis, radians, 0 = in front of the scene.
    pub azimuth: f32,
    /// Above the horizon, radians.
    pub elevation: f32,
    /// From the scene centre, metres. Unused by directional lights.
    pub distance: f32,
    /// Full angle of a spot light's cone, radians.
    pub cone_angle: f32,
    /// How soft a spot light's edge is: 0 hard, 1 fading from the centre.
    pub softness: f32,
    /// How soft the light's shadows are, 0 to 1: the light's apparent size,
    /// from a bare bulb or the sun (0) to a large softbox (1). Shadows blur
    /// more the farther they fall from what casts them.
    pub shadow_softness: f32,
    /// What the light shines through, for spot and directional lights.
    pub mask: Option<LightMask>,
}

/// The apparent size of a light at full shadow softness, radians across.
pub const MAX_SHADOW_ANGLE: f32 = 0.35;

/// Distance new point and spot lights are placed at, metres.
pub const DEFAULT_LIGHT_DISTANCE: f32 = 3.0;

impl Light {
    /// The light's apparent size seen from the scene, radians across.
    pub fn shadow_angle(&self) -> f32 {
        self.shadow_softness.clamp(0.0, 1.0) * MAX_SHADOW_ANGLE
    }

    /// A white light of `kind` in the classic three-quarter key position,
    /// as bright at the scene centre as the default key light.
    pub fn new(kind: LightKind) -> Self {
        let distance = DEFAULT_LIGHT_DISTANCE;
        Self {
            id: LightId(0),
            kind,
            color: Vec3::ONE,
            intensity: if kind.is_positional() {
                distance * distance
            } else {
                1.0
            },
            enabled: true,
            casts_shadows: true,
            azimuth: (-45f32).to_radians(),
            elevation: 45f32.to_radians(),
            distance,
            cone_angle: 40f32.to_radians(),
            softness: 0.3,
            shadow_softness: 0.15,
            mask: None,
        }
    }

    /// Changes the kind, keeping the brightness at the scene centre when
    /// the falloff appears or disappears.
    pub fn set_kind(&mut self, kind: LightKind) {
        let squared = (self.distance * self.distance).max(1e-4);
        match (self.kind.is_positional(), kind.is_positional()) {
            (false, true) => self.intensity *= squared,
            (true, false) => self.intensity /= squared,
            _ => {}
        }
        self.kind = kind;
    }

    /// How brightly the light lights a surface at the scene centre facing it.
    pub fn intensity_at_centre(&self) -> f32 {
        if self.kind.is_positional() {
            self.intensity / (self.distance * self.distance).max(1e-4)
        } else {
            self.intensity
        }
    }

    /// Unit vector from the scene centre towards the light.
    pub fn direction(&self) -> Vec3 {
        let (sin_az, cos_az) = self.azimuth.sin_cos();
        let (sin_el, cos_el) = self.elevation.sin_cos();
        Vec3::new(cos_el * sin_az, sin_el, cos_el * cos_az)
    }

    /// Where a point or spot light is, given the scene centre.
    pub fn position(&self, centre: Vec3) -> Vec3 {
        centre + self.direction() * self.distance
    }

    /// Cosines of the half angles where a spot light's edge starts (outer,
    /// unlit beyond) and where it is fully lit (inner).
    pub fn cone_cosines(&self) -> (f32, f32) {
        let half = (self.cone_angle * 0.5).clamp(0.0, std::f32::consts::FRAC_PI_2);
        let inner_half = half * (1.0 - self.softness.clamp(0.0, 1.0));
        let outer = half.cos();
        // Keep a tiny ramp so a hard edge is still well defined.
        let inner = inner_half.cos().max(outer + 1e-4);
        (outer, inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn placement_angles_give_the_direction_to_the_light() {
        let mut light = Light::new(LightKind::Point);
        light.azimuth = 0.0;
        light.elevation = 0.0;
        light.distance = 3.0;
        assert!((light.direction() - Vec3::Z).length() < 1e-6, "in front");
        let centre = Vec3::new(1.0, 0.5, 0.0);
        assert!((light.position(centre) - (centre + Vec3::Z * 3.0)).length() < 1e-5);
        light.elevation = std::f32::consts::FRAC_PI_2;
        assert!((light.direction() - Vec3::Y).length() < 1e-6, "above");
        light.elevation = 0.0;
        light.azimuth = std::f32::consts::FRAC_PI_2;
        assert!(
            (light.direction() - Vec3::X).length() < 1e-6,
            "to the right"
        );
    }

    #[test]
    fn changing_the_kind_keeps_the_brightness_at_the_centre() {
        let mut light = Light::new(LightKind::Directional);
        light.distance = 10.0;
        let before = light.intensity_at_centre();
        for kind in [LightKind::Point, LightKind::Spot, LightKind::Directional] {
            light.set_kind(kind);
            assert_eq!(light.kind, kind);
            assert!(
                (light.intensity_at_centre() - before).abs() < 1e-4,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_spot_cone_edge_is_softer_with_more_softness() {
        let mut light = Light::new(LightKind::Spot);
        light.cone_angle = 40f32.to_radians();
        light.softness = 0.0;
        let (outer, hard_inner) = light.cone_cosines();
        assert!((outer - 20f32.to_radians().cos()).abs() < 1e-6);
        assert!(hard_inner > outer, "a hard edge still has a tiny ramp");
        light.softness = 1.0;
        let (_, soft_inner) = light.cone_cosines();
        assert!(
            soft_inner > hard_inner,
            "the lit core shrinks as the edge softens"
        );
    }
}
