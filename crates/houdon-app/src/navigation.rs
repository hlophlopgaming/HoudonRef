//! The navigation head: a small planar head in the corner of a viewport
//! that turns with the camera, as in sculpting programs. A click on a side
//! of it turns the view to look from that side; a drag on it orbits.

use std::sync::OnceLock;

use glam::{Mat3, Vec3};
use houdon_core::{ClassicView, Model, OrbitCamera};
use houdon_i18n::Localizer;

/// The head's size on screen, points, and its distance from the corner.
const SIZE: f32 = 96.0;
const MARGIN: f32 = 10.0;

/// What the head was asked to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadAction {
    /// Turn the view to look from this side.
    View(ClassicView),
    /// Orbit by these angles, radians.
    Orbit { yaw: f32, pitch: f32 },
}

/// The head's triangles, centred, one unit in radius, facing +Z.
struct Head {
    corners: Vec<[Vec3; 3]>,
    normals: Vec<Vec3>,
}

fn head() -> &'static Head {
    static HEAD: OnceLock<Head> = OnceLock::new();
    HEAD.get_or_init(|| {
        let model = Model::from_gltf_slice("navigation head", crate::studio::ASARO_HEAD_GLB)
            .expect("the built-in head loads");
        let centre = model.bounds.center();
        // The farthest point is one unit out, so the head fills its disc
        // from any side.
        let reach = model
            .vertices
            .iter()
            .map(|v| (Vec3::from(v.position) - centre).length())
            .fold(0.0, f32::max);
        let scale = 1.0 / reach.max(1e-6);
        // The file looks along -X; turn it to face the front (+Z).
        let turn = Mat3::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let point =
            |i: u32| turn * ((Vec3::from(model.vertices[i as usize].position) - centre) * scale);
        let corners: Vec<[Vec3; 3]> = model
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| [point(t[0]), point(t[1]), point(t[2])])
            .collect();
        let normals = corners
            .iter()
            .map(|[a, b, c]| (*b - *a).cross(*c - *a).normalize_or_zero())
            .collect();
        Head { corners, normals }
    })
}

/// The rotation from the world into the camera's view: x right, y up,
/// z towards the eye.
fn view_rotation(camera: &OrbitCamera) -> Mat3 {
    Mat3::from_mat4(camera.view())
}

/// Where the head is drawn in a viewport `rect`: its top right corner.
pub fn head_rect(rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(rect.right() - MARGIN - SIZE, rect.top() + MARGIN),
        egui::vec2(SIZE, SIZE),
    )
}

/// The triangles that face the eye, in view space, far ones first.
fn visible(rotation: Mat3) -> Vec<([Vec3; 3], Vec3)> {
    let head = head();
    let mut faces: Vec<([Vec3; 3], Vec3)> = head
        .corners
        .iter()
        .zip(&head.normals)
        .filter_map(|(corners, &normal)| {
            let normal = rotation * normal;
            (normal.z > 0.0).then(|| (corners.map(|c| rotation * c), normal))
        })
        .collect();
    faces.sort_by(|a, b| {
        let depth = |f: &([Vec3; 3], Vec3)| f.0[0].z + f.0[1].z + f.0[2].z;
        depth(a).total_cmp(&depth(b))
    });
    faces
}

/// From a point of the head's rect to view space (x right, y up), in
/// head units.
fn to_view(rect: egui::Rect, point: egui::Pos2) -> (f32, f32) {
    let half = rect.width() * 0.5 * 0.92;
    (
        (point.x - rect.center().x) / half,
        (rect.center().y - point.y) / half,
    )
}

/// The view a point of the head's picture asks for: the nearest classic
/// view to the direction from the head's centre to the spot under the
/// point, or `None` beside the head.
fn view_at(rotation: Mat3, rect: egui::Rect, point: egui::Pos2) -> Option<ClassicView> {
    let (x, y) = to_view(rect, point);
    let mut nearest: Option<Vec3> = None;
    for ([a, b, c], _) in visible(rotation) {
        let (v0, v1, p) = (b - a, c - a, Vec3::new(x, y, 0.0) - a);
        let det = v0.x * v1.y - v1.x * v0.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let u = (p.x * v1.y - v1.x * p.y) / det;
        let v = (v0.x * p.y - p.x * v0.y) / det;
        if u < 0.0 || v < 0.0 || u + v > 1.0 {
            continue;
        }
        let spot = a + v0 * u + v1 * v;
        // Faces are sorted far to near: the last one found is in front.
        nearest = Some(spot);
    }
    nearest.map(|spot| ClassicView::nearest(rotation.transpose() * spot))
}

/// Draws the head in the corner of the viewport `rect` and handles the
/// pointer over it. Register it after the viewport's own interaction so
/// it is on top.
pub fn navigation_head(
    ui: &mut egui::Ui,
    tr: &Localizer,
    rect: egui::Rect,
    camera: &OrbitCamera,
) -> Option<HeadAction> {
    let area = head_rect(rect);
    if !rect.contains_rect(area) {
        return None;
    }
    let response = ui.interact(
        area,
        ui.scope_id().with("navigation_head"),
        egui::Sense::click_and_drag(),
    );
    let rotation = view_rotation(camera);
    let hovered = response.hovered() || response.dragged();
    let painter = ui.painter_at(area);
    // A faint disc says the corner can be touched.
    painter.circle_filled(
        area.center(),
        SIZE * 0.5,
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, if hovered { 60 } else { 28 }),
    );
    let half = area.width() * 0.5 * 0.92;
    let to_screen =
        |v: Vec3| egui::pos2(area.center().x + v.x * half, area.center().y - v.y * half);
    // Light from the upper left, in view space, so the planes read.
    let light = Vec3::new(-0.45, 0.6, 0.66).normalize();
    let base = if hovered { 236.0 } else { 214.0 };
    let mut mesh = egui::Mesh::default();
    for (corners, normal) in visible(rotation) {
        let shade = 0.32 + 0.68 * normal.dot(light).max(0.0);
        let grey = (base * shade) as u8;
        let color = egui::Color32::from_rgb(grey, grey, (f32::from(grey) * 0.97) as u8);
        let first = mesh.vertices.len() as u32;
        for corner in corners {
            mesh.colored_vertex(to_screen(corner), color);
        }
        mesh.add_triangle(first, first + 1, first + 2);
    }
    painter.add(egui::Shape::mesh(mesh));

    let hover_view = response
        .hover_pos()
        .and_then(|pointer| view_at(rotation, area, pointer));
    if let Some(view) = hover_view
        && !response.dragged()
    {
        response
            .clone()
            .on_hover_text(tr.tr(&format!("view-{}", view.key())));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.dragged() {
        let drag = response.drag_delta();
        // A drag across the head turns the view half way round.
        let speed = std::f32::consts::PI / SIZE;
        return Some(HeadAction::Orbit {
            yaw: -drag.x * speed,
            pitch: drag.y * speed,
        });
    }
    if response.clicked()
        && let Some(view) = response
            .interact_pointer_pos()
            .and_then(|pointer| view_at(rotation, area, pointer))
    {
        return Some(HeadAction::View(view));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SIZE, SIZE))
    }

    #[test]
    fn the_head_faces_the_eye_from_the_front() {
        let camera = OrbitCamera::default();
        let rotation = view_rotation(&camera);
        // The middle of the picture is the face.
        assert_eq!(
            view_at(rotation, rect(), rect().center()),
            Some(ClassicView::Around(0))
        );
        // Beside the head is nothing.
        assert_eq!(view_at(rotation, rect(), egui::pos2(1.0, 1.0)), None);
    }

    #[test]
    fn clicking_the_back_of_the_head_asks_for_the_back() {
        let camera = OrbitCamera {
            yaw: std::f32::consts::PI,
            ..OrbitCamera::default()
        };
        let rotation = view_rotation(&camera);
        assert_eq!(
            view_at(rotation, rect(), rect().center()),
            Some(ClassicView::Around(4))
        );
    }

    #[test]
    fn the_side_of_the_head_seen_from_the_front_asks_for_that_side() {
        let camera = OrbitCamera::default();
        let rotation = view_rotation(&camera);
        // On screen right, seen from the front, is the figure's left (+X).
        let right_edge = egui::pos2(rect().center().x + SIZE * 0.14, rect().center().y);
        let view = view_at(rotation, rect(), right_edge).unwrap();
        assert!(
            matches!(view, ClassicView::Around(1) | ClassicView::Around(2)),
            "{view:?}"
        );
    }

    #[test]
    fn the_top_of_the_head_asks_for_the_view_from_above() {
        let camera = OrbitCamera {
            pitch: 0.6,
            ..OrbitCamera::default()
        };
        let rotation = view_rotation(&camera);
        let crown = egui::pos2(rect().center().x, rect().top() + SIZE * 0.2);
        assert_eq!(view_at(rotation, rect(), crown), Some(ClassicView::Above));
    }
}
