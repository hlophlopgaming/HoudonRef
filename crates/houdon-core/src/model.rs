//! Static triangle meshes loaded from glTF.

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};

/// Default figure height in metres used when normalising human models.
pub const DEFAULT_FIGURE_HEIGHT: f32 = 1.75;

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    /// An empty box that any point will extend.
    pub const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Self {
        points.into_iter().fold(Self::EMPTY, |b, p| b.extended(p))
    }

    #[must_use]
    pub fn extended(self, p: Vec3) -> Self {
        Self {
            min: self.min.min(p),
            max: self.max.max(p),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min.cmpgt(self.max).any()
    }

    pub fn size(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            self.max - self.min
        }
    }

    pub fn center(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            (self.min + self.max) * 0.5
        }
    }

    /// Radius of the sphere around [`Self::center`] that encloses the box.
    pub fn radius(&self) -> f32 {
        self.size().length() * 0.5
    }
}

/// GPU-ready vertex: position and normal, both in model space.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// Errors that can happen while loading a model.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("invalid glTF file: {0}")]
    Gltf(#[from] gltf::Error),
    #[error("the file contains no triangle meshes")]
    NoMeshes,
}

/// A static model: all triangle meshes of a glTF scene merged into one
/// indexed mesh, with node transforms baked in.
///
/// Skinned meshes are taken in their bind pose. Materials and textures are
/// ignored for now: every model is drawn with the same neutral clay shader.
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds: Bounds,
}

impl Model {
    /// Loads a model from the bytes of a `.glb` file or a self-contained
    /// `.gltf` file (buffers embedded as data URIs).
    pub fn from_gltf_slice(name: impl Into<String>, bytes: &[u8]) -> Result<Self, LoadError> {
        let (document, buffers, _images) = gltf::import_slice(bytes)?;
        let mut model = Self {
            name: name.into(),
            vertices: Vec::new(),
            indices: Vec::new(),
            bounds: Bounds::EMPTY,
        };

        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next());
        match scene {
            Some(scene) => {
                for node in scene.nodes() {
                    model.add_node(&node, Mat4::IDENTITY, &buffers);
                }
            }
            // No scene: take every mesh without transforms.
            None => {
                for mesh in document.meshes() {
                    model.add_mesh(&mesh, Mat4::IDENTITY, &buffers);
                }
            }
        }

        if model.indices.is_empty() {
            return Err(LoadError::NoMeshes);
        }
        model.recompute_bounds();
        Ok(model)
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Parameter `t` of the nearest triangle hit by the ray
    /// `origin + t * direction` (model space), or `None`.
    pub fn ray_hit(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let mut nearest: Option<f32> = None;
        for tri in self.indices.chunks_exact(3) {
            let [a, b, c] =
                [tri[0], tri[1], tri[2]].map(|i| Vec3::from(self.vertices[i as usize].position));
            if let Some(t) = ray_triangle(origin, direction, a, b, c)
                && nearest.is_none_or(|n| t < n)
            {
                nearest = Some(t);
            }
        }
        nearest
    }

    /// Scales the model uniformly so it is `height` units tall, stands on
    /// the ground plane (`y = 0`) and is centred on the vertical axis.
    ///
    /// Models come in arbitrary units, so this is what makes camera distance
    /// and focal length mean the same thing for every model.
    pub fn normalize_to_height(&mut self, height: f32) {
        let size = self.bounds.size();
        if size.y <= f32::EPSILON {
            return;
        }
        let scale = height / size.y;
        let center = self.bounds.center();
        let offset = Vec3::new(-center.x, -self.bounds.min.y, -center.z);
        for v in &mut self.vertices {
            let p = (Vec3::from(v.position) + offset) * scale;
            v.position = p.to_array();
        }
        self.recompute_bounds();
    }

    fn recompute_bounds(&mut self) {
        self.bounds = Bounds::from_points(self.vertices.iter().map(|v| Vec3::from(v.position)));
    }

    fn add_node(&mut self, node: &gltf::Node, parent: Mat4, buffers: &[gltf::buffer::Data]) {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            // The glTF spec says the transform of a skinned mesh node is ignored:
            // its vertices are placed by the joints, which in the bind pose
            // reproduce the mesh's own coordinates.
            let mesh_transform = if node.skin().is_some() {
                Mat4::IDENTITY
            } else {
                world
            };
            self.add_mesh(&mesh, mesh_transform, buffers);
        }
        for child in node.children() {
            self.add_node(&child, world, buffers);
        }
    }

    fn add_mesh(&mut self, mesh: &gltf::Mesh, transform: Mat4, buffers: &[gltf::buffer::Data]) {
        let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();
        let flip_winding = transform.determinant() < 0.0;

        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|d| &d.0[..]));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let positions: Vec<Vec3> = positions
                .map(|p| transform.transform_point3(Vec3::from(p)))
                .collect();
            let indices: Vec<u32> = match reader.read_indices() {
                Some(indices) => indices.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            if indices.iter().any(|&i| i as usize >= positions.len()) {
                continue;
            }
            let normals: Vec<Vec3> = match reader.read_normals() {
                Some(normals) => normals
                    .map(|n| (normal_matrix * Vec3::from(n)).normalize_or_zero())
                    .collect(),
                None => smooth_normals(&positions, &indices),
            };

            let base = self.vertices.len() as u32;
            self.vertices
                .extend(positions.iter().zip(&normals).map(|(p, n)| Vertex {
                    position: p.to_array(),
                    normal: n.to_array(),
                }));
            for tri in indices.chunks_exact(3) {
                let (a, b, c) = (tri[0] + base, tri[1] + base, tri[2] + base);
                if flip_winding {
                    self.indices.extend([a, c, b]);
                } else {
                    self.indices.extend([a, b, c]);
                }
            }
        }
    }
}

/// Möller–Trumbore ray/triangle intersection, both sides.
fn ray_triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let edge1 = b - a;
    let edge2 = c - a;
    let p = direction.cross(edge2);
    let det = edge1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = direction.dot(q) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = edge2.dot(q) * inv_det;
    (t > 0.0).then_some(t)
}

/// Area-weighted vertex normals for meshes that come without normals.
fn smooth_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
        let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        normals[a] += face;
        normals[b] += face;
        normals[c] += face;
    }
    normals.iter().map(|n| n.normalize_or(Vec3::Y)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MALE: &[u8] = include_bytes!("../../../assets/models/test/male.glb");
    const FEMALE: &[u8] = include_bytes!("../../../assets/models/test/female.glb");

    #[test]
    fn bounds_of_points() {
        let b = Bounds::from_points([Vec3::new(-1.0, 0.0, 2.0), Vec3::new(3.0, 4.0, -2.0)]);
        assert_eq!(b.min, Vec3::new(-1.0, 0.0, -2.0));
        assert_eq!(b.max, Vec3::new(3.0, 4.0, 2.0));
        assert_eq!(b.center(), Vec3::new(1.0, 2.0, 0.0));
        assert!(Bounds::EMPTY.is_empty());
        assert_eq!(Bounds::EMPTY.size(), Vec3::ZERO);
    }

    #[test]
    fn loads_test_models() {
        let male = Model::from_gltf_slice("male", MALE).unwrap();
        assert_eq!(male.triangle_count(), 3592);
        let female = Model::from_gltf_slice("female", FEMALE).unwrap();
        assert_eq!(female.triangle_count(), 3568);
        for model in [&male, &female] {
            assert!(model.vertices.iter().all(|v| {
                let n = Vec3::from(v.normal);
                n.is_finite() && (n.length() - 1.0).abs() < 1e-3
            }));
        }
    }

    #[test]
    fn female_node_transform_is_applied() {
        // The female mesh is stored lying along -Z and is stood up by a
        // 90 degree rotation on its node; it must come out upright.
        let female = Model::from_gltf_slice("female", FEMALE).unwrap();
        let size = female.bounds.size();
        assert!(size.y > size.x && size.y > size.z, "size = {size}");
    }

    #[test]
    fn normalization_puts_model_on_ground() {
        for bytes in [MALE, FEMALE] {
            let mut model = Model::from_gltf_slice("test", bytes).unwrap();
            model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
            let b = model.bounds;
            assert!((b.size().y - DEFAULT_FIGURE_HEIGHT).abs() < 1e-4);
            assert!(b.min.y.abs() < 1e-5);
            assert!(b.center().x.abs() < 1e-4 && b.center().z.abs() < 1e-4);
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(Model::from_gltf_slice("bad", b"not a model").is_err());
    }
}
