//! The classic views an artist turns a figure to: front, back, the sides,
//! the three-quarter views between them, and from above and below.

use std::f32::consts::FRAC_PI_4;

use glam::Vec3;

use crate::OrbitCamera;

/// One of the classic views of a figure facing +Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassicView {
    /// Around the figure, in eighths of a turn from the front: 0 is the
    /// front, 2 its left side (+X), 4 the back, 6 its right side.
    Around(u8),
    Above,
    Below,
}

/// How far up or down a direction must point to mean "from above" or
/// "from below" rather than a view from around.
const STEEP: f32 = 0.8;

impl ClassicView {
    /// The view that looks from `direction` (from the figure towards the
    /// eye), snapped to the nearest classic one.
    pub fn nearest(direction: Vec3) -> Self {
        let d = direction.normalize_or(Vec3::Z);
        if d.y > STEEP {
            return Self::Above;
        }
        if d.y < -STEEP {
            return Self::Below;
        }
        let yaw = d.x.atan2(d.z);
        Self::Around(((yaw / FRAC_PI_4).round() as i32).rem_euclid(8) as u8)
    }

    /// A short stable name: "front", "back-left", "above", …
    pub fn key(self) -> &'static str {
        match self {
            Self::Around(0) => "front",
            Self::Around(1) => "front-left",
            Self::Around(2) => "left",
            Self::Around(3) => "back-left",
            Self::Around(4) => "back",
            Self::Around(5) => "back-right",
            Self::Around(6) => "right",
            Self::Around(_) => "front-right",
            Self::Above => "above",
            Self::Below => "below",
        }
    }

    /// `camera` turned to look from this view, around the same point at
    /// the same distance. From above or below, the figure keeps the side
    /// it showed nearest to the bottom of the picture.
    pub fn applied_to(self, camera: &OrbitCamera) -> OrbitCamera {
        let snapped_yaw = |yaw: f32| (yaw / FRAC_PI_4).round() * FRAC_PI_4;
        let (yaw, pitch) = match self {
            Self::Around(step) => (f32::from(step % 8) * FRAC_PI_4, 0.0),
            Self::Above => (snapped_yaw(camera.yaw), OrbitCamera::MAX_PITCH),
            Self::Below => (snapped_yaw(camera.yaw), OrbitCamera::MIN_PITCH),
        };
        OrbitCamera {
            yaw: yaw.rem_euclid(std::f32::consts::TAU),
            pitch,
            ..*camera
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_snap_to_the_nearest_classic_view() {
        assert_eq!(ClassicView::nearest(Vec3::Z), ClassicView::Around(0));
        assert_eq!(ClassicView::nearest(-Vec3::Z), ClassicView::Around(4));
        assert_eq!(ClassicView::nearest(Vec3::X), ClassicView::Around(2));
        assert_eq!(ClassicView::nearest(-Vec3::X), ClassicView::Around(6));
        assert_eq!(
            ClassicView::nearest(Vec3::new(0.7, 0.1, 0.6)),
            ClassicView::Around(1)
        );
        assert_eq!(
            ClassicView::nearest(Vec3::new(-0.5, 0.0, -0.6)),
            ClassicView::Around(5)
        );
        assert_eq!(
            ClassicView::nearest(Vec3::new(0.1, 1.0, 0.2)),
            ClassicView::Above
        );
        assert_eq!(
            ClassicView::nearest(Vec3::new(0.0, -1.0, 0.3)),
            ClassicView::Below
        );
        // A little up is still a view from around.
        assert_eq!(
            ClassicView::nearest(Vec3::new(0.0, 0.5, -1.0)),
            ClassicView::Around(4)
        );
    }

    #[test]
    fn a_view_turns_the_camera_and_keeps_its_place() {
        let camera = OrbitCamera {
            yaw: 0.3,
            pitch: 0.4,
            ..OrbitCamera::default()
        };
        let back = ClassicView::Around(4).applied_to(&camera);
        assert!((back.yaw - std::f32::consts::PI).abs() < 1e-5);
        assert_eq!(back.pitch, 0.0);
        assert_eq!(
            (back.target, back.distance),
            (camera.target, camera.distance)
        );
        // The eye ends up on the side the view names.
        let eye = back.eye() - back.target;
        assert!(eye.z < 0.0 && eye.x.abs() < 1e-4);
        let left = ClassicView::Around(2).applied_to(&camera);
        assert!(
            (left.eye() - left.target).x > 0.0,
            "the figure's left is +X"
        );
        let above = ClassicView::Above.applied_to(&camera);
        assert_eq!(above.pitch, OrbitCamera::MAX_PITCH);
        assert_eq!(above.yaw, 0.0, "the nearest eighth of the yaw");
    }

    #[test]
    fn every_view_has_its_own_name() {
        let mut names: Vec<_> = (0..8)
            .map(ClassicView::Around)
            .chain([ClassicView::Above, ClassicView::Below])
            .map(ClassicView::key)
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 10);
    }
}
