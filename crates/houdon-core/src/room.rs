//! The Room: the floor, the backdrop and the ambient light they give.

use glam::Vec3;

use crate::{Bounds, Model, Vertex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinRoom {
    Grey,
    White,
    Dark,
}

impl BuiltinRoom {
    pub const ALL: [Self; 3] = [Self::Grey, Self::White, Self::Dark];
}

/// What surrounds the objects: a floor that sweeps up into a backdrop
/// without a corner, like a photo studio, and the ambient light they give:
/// the backdrop's colour from above and the floor's colour from below, so
/// a coloured floor tints the underside of forms (a reflex).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Room {
    /// Linear RGB.
    pub floor_color: Vec3,
    /// Linear RGB.
    pub backdrop_color: Vec3,
    pub ambient_intensity: f32,
    /// Whether the floor and backdrop are drawn; the ambient light stays.
    pub visible: bool,
}

impl Room {
    pub fn builtin(room: BuiltinRoom) -> Self {
        let (floor, backdrop, ambient) = match room {
            // Photographic 18% grey.
            BuiltinRoom::Grey => (0.18, 0.18, 0.8),
            BuiltinRoom::White => (0.7, 0.8, 0.5),
            BuiltinRoom::Dark => (0.02, 0.015, 1.0),
        };
        Self {
            floor_color: Vec3::splat(floor),
            backdrop_color: Vec3::splat(backdrop),
            ambient_intensity: ambient,
            visible: true,
        }
    }

    /// The backdrop colour in 8-bit sRGB, for clearing the picture behind
    /// the room and for the interface.
    pub fn backdrop_srgb(&self) -> [u8; 3] {
        self.backdrop_color.to_array().map(linear_to_srgb)
    }
}

impl Default for Room {
    fn default() -> Self {
        Self::builtin(BuiltinRoom::Grey)
    }
}

/// One linear channel to 8-bit sRGB.
fn linear_to_srgb(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

/// How far the room reaches beyond the objects at least, metres: far enough
/// that orbiting a framed view does not show its edges.
pub const MIN_ROOM_REACH: f32 = 30.0;

/// Room sizes are rounded to this step, metres, so small moves of the
/// objects keep the same room instead of rebuilding it.
const STEP: f32 = 0.5;

/// The size and place of the floor-to-backdrop sweep around some objects.
/// The backdrop stands behind the objects (towards -Z), the floor reaches
/// in front of them (towards +Z), where the default camera is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomShape {
    pub centre_x: f32,
    pub half_width: f32,
    /// Where the backdrop stands.
    pub back_z: f32,
    /// Where the floor ends in front.
    pub front_z: f32,
    pub height: f32,
    /// Radius of the curve between floor and backdrop.
    pub cove_radius: f32,
}

impl RoomShape {
    /// A room around `objects`, generous enough to fill the view.
    pub fn around(objects: &Bounds) -> Self {
        let objects = if objects.is_empty() {
            Bounds::from_points([Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5)])
        } else {
            *objects
        };
        let up = |v: f32| (v / STEP).ceil() * STEP;
        let down = |v: f32| (v / STEP).floor() * STEP;
        let radius = objects.radius();
        let height = up((objects.max.y.max(0.0) * 1.5 + 1.0).max(MIN_ROOM_REACH * 0.5));
        // The cove is sized by the objects, not the tall backdrop, so the
        // floor stays flat around them.
        let cove_radius = up(objects.max.y.max(0.5) * 0.5);
        // The flat floor must reach behind the objects: the cove starts
        // only after a gap of at least its radius.
        let gap = (radius * 0.5 + 0.5).max(cove_radius + 0.2);
        Self {
            centre_x: (objects.center().x / STEP).round() * STEP,
            half_width: up((objects.size().x * 0.5 + radius * 2.0 + 1.0).max(MIN_ROOM_REACH)),
            back_z: down(objects.min.z - gap),
            front_z: up(objects.max.z + (radius * 3.0 + 2.0).max(MIN_ROOM_REACH)),
            height,
            cove_radius,
        }
    }

    /// The sweep as a mesh with normals facing into the room. Triangles wind
    /// counter-clockwise seen from inside, so the room can be drawn one-sided
    /// and seen through from outside.
    pub fn mesh(&self) -> Model {
        // The profile in the (z, y) plane from the front of the floor, round
        // the cove, up the backdrop; each point with its inward normal.
        let r = self
            .cove_radius
            .min(self.height)
            .min(self.front_z - self.back_z);
        /// A point of the profile: position and inward normal in (z, y).
        struct ProfilePoint {
            z: f32,
            y: f32,
            normal_z: f32,
            normal_y: f32,
        }
        let point = |z, y, normal_z, normal_y| ProfilePoint {
            z,
            y,
            normal_z,
            normal_y,
        };
        let mut profile = vec![point(self.front_z, 0.0, 0.0, 1.0)];
        const COVE_STEPS: usize = 12;
        for i in 0..=COVE_STEPS {
            let angle = std::f32::consts::FRAC_PI_2 * i as f32 / COVE_STEPS as f32;
            let (sin, cos) = angle.sin_cos();
            profile.push(point(self.back_z + r - r * sin, r - r * cos, sin, cos));
        }
        profile.push(point(self.back_z, self.height, 1.0, 0.0));

        let left = self.centre_x - self.half_width;
        let right = self.centre_x + self.half_width;
        let vertices = profile
            .iter()
            .flat_map(|p| {
                let normal = [0.0, p.normal_y, p.normal_z];
                [left, right].map(|x| Vertex {
                    position: [x, p.y, p.z],
                    normal,
                    // Floor and backdrop in metres, for textures later.
                    uv: [x, p.z - p.y],
                })
            })
            .collect::<Vec<_>>();
        let indices = (0..profile.len() as u32 - 1)
            .flat_map(|i| {
                let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
                [a, b, d, a, d, c]
            })
            .collect();
        Model::from_vertices("room", vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bounds;
    use glam::Vec3;

    #[test]
    fn built_in_rooms_differ() {
        let grey = Room::builtin(BuiltinRoom::Grey);
        let white = Room::builtin(BuiltinRoom::White);
        let dark = Room::builtin(BuiltinRoom::Dark);
        assert!(white.backdrop_color.x > grey.backdrop_color.x);
        assert!(grey.backdrop_color.x > dark.backdrop_color.x);
        assert!(white.floor_color.x > dark.floor_color.x);
    }

    #[test]
    fn the_room_encloses_the_objects() {
        let objects = Bounds::from_points([Vec3::new(-1.0, 0.0, -0.5), Vec3::new(2.0, 1.8, 0.7)]);
        let shape = RoomShape::around(&objects);
        assert!(shape.half_width >= 1.0 + 1.0 && shape.centre_x - shape.half_width <= -1.0);
        assert!(shape.centre_x + shape.half_width >= 2.0);
        assert!(
            shape.back_z < -0.5,
            "the backdrop stands behind the objects"
        );
        assert!(
            shape.front_z > 0.7,
            "the floor reaches in front of the objects"
        );
        assert!(shape.height > 1.8, "the backdrop rises above the objects");
        let mesh = shape.mesh();
        let b = mesh.bounds;
        assert!(b.min.y.abs() < 1e-5, "the floor is the ground");
        assert!(b.min.x <= objects.min.x && b.max.x >= objects.max.x);
        assert!(b.min.z <= objects.min.z && b.max.z >= objects.max.z);
        assert!(b.max.y >= objects.max.y);
    }

    #[test]
    fn the_cove_starts_behind_even_large_objects() {
        for size in [0.2, 2.0, 10.0, 40.0] {
            let objects = Bounds::from_points([
                Vec3::new(-size, 0.0, -size),
                Vec3::new(size, size * 2.0, size),
            ]);
            let shape = RoomShape::around(&objects);
            assert!(
                shape.back_z + shape.cove_radius <= objects.min.z,
                "size {size}: {shape:?}"
            );
        }
    }

    #[test]
    fn the_room_is_large_enough_to_keep_its_edges_out_of_view() {
        let figure = Bounds::from_points([Vec3::new(-0.3, 0.0, -0.15), Vec3::new(0.3, 1.75, 0.15)]);
        let shape = RoomShape::around(&figure);
        assert!(shape.half_width >= MIN_ROOM_REACH, "{shape:?}");
        assert!(shape.front_z - figure.max.z >= MIN_ROOM_REACH, "{shape:?}");
        assert!(shape.height >= MIN_ROOM_REACH * 0.5, "{shape:?}");
    }

    #[test]
    fn small_moves_keep_the_same_room() {
        let a = Bounds::from_points([Vec3::new(-0.3, 0.0, -0.2), Vec3::new(0.3, 1.75, 0.2)]);
        let b = Bounds::from_points([Vec3::new(-0.31, 0.0, -0.2), Vec3::new(0.32, 1.75, 0.2)]);
        assert_eq!(RoomShape::around(&a), RoomShape::around(&b));
    }

    #[test]
    fn the_room_faces_inwards() {
        let objects = Bounds::from_points([Vec3::new(-0.3, 0.0, -0.2), Vec3::new(0.3, 1.75, 0.2)]);
        let mesh = RoomShape::around(&objects).mesh();
        for v in &mesh.vertices {
            let n = Vec3::from(v.normal);
            // Floor normals point up, backdrop normals point forward (+Z),
            // the sweep in between both.
            assert!(n.y >= -1e-4 && n.z >= -1e-4, "{n}");
            assert!((n.length() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn colours_convert_to_srgb_for_display() {
        let room = Room {
            backdrop_color: Vec3::splat(0.2159),
            ..Room::builtin(BuiltinRoom::Grey)
        };
        // Linear 0.2159 is sRGB 128.
        assert_eq!(room.backdrop_srgb(), [128, 128, 128]);
    }
}
