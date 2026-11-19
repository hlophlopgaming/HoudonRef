//! Orbit camera described the way artists think about it: a point of
//! interest, a direction around it, a distance and a lens focal length.

use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3};

use crate::Bounds;

/// Height of a full-frame (35 mm) sensor, used to turn focal length into a field of view.
pub const SENSOR_HEIGHT_MM: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
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
        self.distance = (self.distance * factor).clamp(0.05, 1000.0);
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
}
