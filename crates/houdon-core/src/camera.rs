//! Orbit camera described the way artists think about it: a point of
//! interest, a direction around it, a distance and a lens focal length.

use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3};

use crate::Bounds;

/// Height of a full-frame (35 mm) sensor, used to turn focal length into a field of view.
pub const SENSOR_HEIGHT_MM: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrbitCamera {
    /// Point the camera looks at.
    pub target: Vec3,
    /// Rotation around the vertical axis, radians. 0 looks at the model's front (+Z side).
    pub yaw: f32,
    /// Elevation above the horizon, radians.
    pub pitch: f32,
    /// Distance from the target.
    pub distance: f32,
    /// Lens focal length in millimetres on a full-frame sensor.
    pub focal_length_mm: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            distance: 4.0,
            focal_length_mm: 50.0,
        }
    }
}

impl OrbitCamera {
    pub const MIN_PITCH: f32 = -1.55;
    pub const MAX_PITCH: f32 = 1.55;
    pub const MIN_DISTANCE: f32 = 0.05;
    pub const MAX_DISTANCE: f32 = 1000.0;
    pub const MIN_FOCAL_LENGTH_MM: f32 = 14.0;
    pub const MAX_FOCAL_LENGTH_MM: f32 = 300.0;

    /// Vertical field of view in radians.
    pub fn vertical_fov(&self) -> f32 {
        2.0 * (SENSOR_HEIGHT_MM * 0.5 / self.focal_length_mm).atan()
    }

    pub fn eye(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cos_pitch * sin_yaw, sin_pitch, cos_pitch * cos_yaw)
    }

    pub fn view(&self) -> Mat4 {
        view::look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// Projection with a 0..1 depth range, as wgpu expects.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let near = (self.distance * 0.01).max(0.01);
        let far = self.distance * 20.0 + 100.0;
        directx::perspective(self.vertical_fov(), aspect.max(1e-3), near, far)
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// A ray from the eye through a point of the picture, given in
    /// normalised device coordinates (`-1..1`, `y` up). Returns origin and
    /// unit direction.
    pub fn ray(&self, ndc_x: f32, ndc_y: f32, aspect: f32) -> (Vec3, Vec3) {
        let inverse = self.view_projection(aspect).inverse();
        let near = inverse.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let far = inverse.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (self.eye(), (far - near).normalize())
    }

    /// Rotates around the target by the given angles in radians.
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(Self::MIN_PITCH, Self::MAX_PITCH);
    }

    /// Moves the target in the view plane. `dx`, `dy` are fractions of the
    /// visible height at the target's distance.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let view_height = 2.0 * self.distance * (self.vertical_fov() * 0.5).tan();
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        self.target += (-right * dx + up * dy) * view_height;
    }

    /// Multiplies the distance by `factor` (less than 1 moves closer).
    pub fn dolly(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    /// Sets the focal length, keeping the framing: the distance changes so
    /// the subject keeps the same size on screen, like a dolly zoom.
    pub fn set_focal_length_keep_framing(&mut self, focal_length_mm: f32) {
        let focal_length_mm =
            focal_length_mm.clamp(Self::MIN_FOCAL_LENGTH_MM, Self::MAX_FOCAL_LENGTH_MM);
        self.distance *= focal_length_mm / self.focal_length_mm;
        self.focal_length_mm = focal_length_mm;
    }

    /// Points the camera at `bounds` and moves back until it fits the view.
    pub fn frame(&mut self, bounds: &Bounds, aspect: f32) {
        if bounds.is_empty() {
            return;
        }
        let vertical = self.vertical_fov();
        let horizontal = 2.0 * ((vertical * 0.5).tan() * aspect.max(1e-3)).atan();
        let half_fov = vertical.min(horizontal) * 0.5;
        // Leave a small margin around the subject.
        let radius = bounds.radius() * 1.05;
        self.target = bounds.center();
        self.distance = radius / half_fov.sin();
    }
}

/// A smooth move of the camera from one view to another, so the eye
/// follows where it goes instead of being cut to a new picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraFlight {
    from: OrbitCamera,
    to: OrbitCamera,
    elapsed: f32,
}

impl CameraFlight {
    /// How long a flight takes, seconds.
    pub const DURATION: f32 = 0.5;

    pub fn new(from: OrbitCamera, to: OrbitCamera) -> Self {
        Self {
            from,
            to,
            elapsed: 0.0,
        }
    }

    /// Where the flight ends.
    pub fn destination(&self) -> OrbitCamera {
        self.to
    }

    /// Moves on by `seconds`; returns the camera for this moment.
    pub fn advance(&mut self, seconds: f32) -> OrbitCamera {
        self.elapsed = (self.elapsed + seconds.max(0.0)).min(Self::DURATION);
        let progress = self.elapsed / Self::DURATION;
        // Fast at first, settling gently.
        let eased = 1.0 - (1.0 - progress).powi(3);
        between(&self.from, &self.to, eased)
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= Self::DURATION
    }
}

/// The camera a fraction `t` of the way from `a` to `b`: the shorter way
/// round, and the distance changing by the same ratio at every step, so
/// a close-up does not rush in at the end.
fn between(a: &OrbitCamera, b: &OrbitCamera, t: f32) -> OrbitCamera {
    use std::f32::consts::{PI, TAU};
    let turn = (b.yaw - a.yaw + PI).rem_euclid(TAU) - PI;
    let ratio = b.distance / a.distance.max(f32::MIN_POSITIVE);
    OrbitCamera {
        target: a.target.lerp(b.target, t),
        yaw: (a.yaw + turn * t).rem_euclid(TAU),
        pitch: a.pitch + (b.pitch - a.pitch) * t,
        distance: a.distance * ratio.powf(t),
        focal_length_mm: a.focal_length_mm + (b.focal_length_mm - a.focal_length_mm) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifty_mm_is_a_normal_lens() {
        let cam = OrbitCamera::default();
        let degrees = cam.vertical_fov().to_degrees();
        assert!((degrees - 26.99).abs() < 0.05, "{degrees}");
    }

    #[test]
    fn default_camera_looks_at_front() {
        let cam = OrbitCamera::default();
        let eye = cam.eye();
        assert!(eye.z > 0.0 && eye.x.abs() < 1e-6);
    }

    #[test]
    fn the_centre_ray_points_at_the_target() {
        let mut cam = OrbitCamera::default();
        cam.orbit(0.7, 0.3);
        let (origin, direction) = cam.ray(0.0, 0.0, 1.5);
        assert!((origin - cam.eye()).length() < 1e-5);
        let to_target = (cam.target - cam.eye()).normalize();
        assert!(direction.dot(to_target) > 0.9999);
        let (_, up_ray) = cam.ray(0.0, 1.0, 1.5);
        assert!(
            up_ray.y > direction.y,
            "the top of the picture looks higher"
        );
    }

    #[test]
    fn pitch_is_clamped() {
        let mut cam = OrbitCamera::default();
        cam.orbit(0.0, 10.0);
        assert_eq!(cam.pitch, OrbitCamera::MAX_PITCH);
    }

    #[test]
    fn framing_fits_the_bounding_sphere() {
        let bounds = Bounds::from_points([Vec3::new(-0.3, 0.0, -0.2), Vec3::new(0.3, 1.8, 0.2)]);
        let mut cam = OrbitCamera::default();
        cam.frame(&bounds, 1.5);
        assert_eq!(cam.target, bounds.center());
        let angle = (bounds.radius() / cam.distance).asin();
        assert!(angle <= cam.vertical_fov() * 0.5);
    }

    #[test]
    fn focal_change_keeps_framing() {
        let mut cam = OrbitCamera::default();
        let before = cam.distance * (cam.vertical_fov() * 0.5).tan();
        cam.set_focal_length_keep_framing(100.0);
        let after = cam.distance * (cam.vertical_fov() * 0.5).tan();
        // Visible height at the target stays about the same.
        assert!((before - after).abs() / before < 0.01);
    }

    #[test]
    fn a_flight_starts_where_it_was_and_lands_where_asked() {
        let from = OrbitCamera::default();
        let to = OrbitCamera {
            target: Vec3::new(0.3, 1.5, 0.1),
            yaw: 1.0,
            pitch: 0.4,
            distance: 0.5,
            focal_length_mm: 85.0,
        };
        let mut flight = CameraFlight::new(from, to);
        assert_eq!(flight.advance(0.0), from);
        assert!(!flight.finished());
        let middle = flight.advance(CameraFlight::DURATION * 0.5);
        assert!(middle.distance < from.distance && middle.distance > to.distance);
        let end = flight.advance(1.0);
        assert!(flight.finished());
        assert!(end.target.distance(to.target) < 1e-5);
        assert!((end.yaw - to.yaw).abs() < 1e-5);
        assert!((end.distance - to.distance).abs() < 1e-5);
    }

    #[test]
    fn a_flight_turns_the_short_way_round() {
        let from = OrbitCamera {
            yaw: 0.1,
            ..OrbitCamera::default()
        };
        let to = OrbitCamera {
            yaw: std::f32::consts::TAU - 0.1,
            ..OrbitCamera::default()
        };
        let middle = between(&from, &to, 0.5);
        // Through 0, not through half a turn.
        assert!(middle.yaw < 0.01 || middle.yaw > std::f32::consts::TAU - 0.01);
    }
}
