//! Static triangle meshes loaded from glTF.

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3, Vec4};

/// Default figure height in metres used when normalising human models.
pub const DEFAULT_FIGURE_HEIGHT: f32 = 1.75;

/// How many times taller than wide a model must be to be taken for a figure.
pub const FIGURE_ASPECT: f32 = 2.5;

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

/// GPU-ready vertex: position and normal in model space, and the texture
/// coordinates of the first set (zero when the model has none).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// A surface description in the glTF metallic-roughness model. Colours are
/// linear; texture fields index [`Model::textures`].
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: String,
    /// Linear RGBA, multiplied with the base colour texture.
    pub base_color: Vec4,
    pub base_color_texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    /// Roughness in green, metalness in blue, multiplied with the factors.
    pub metallic_roughness_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    pub normal_scale: f32,
}

/// An image a material uses, decoded to 8-bit RGBA rows.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A run of [`Model::indices`] drawn with one material: one glTF primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshPart {
    pub first_index: u32,
    pub index_count: u32,
    /// Index into [`Model::materials`], or `None` for a part without one.
    pub material: Option<usize>,
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
/// indexed mesh with node transforms baked in, split into parts by material.
///
/// Skinned meshes are taken in their bind pose.
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds: Bounds,
    pub parts: Vec<MeshPart>,
    pub materials: Vec<Material>,
    pub textures: Vec<TextureImage>,
}

impl Model {
    /// Loads a model from the bytes of a `.glb` file or a self-contained
    /// `.gltf` file (buffers embedded as data URIs).
    pub fn from_gltf_slice(name: impl Into<String>, bytes: &[u8]) -> Result<Self, LoadError> {
        let (document, buffers, images) = gltf::import_slice(bytes)?;
        let mut model = Self::empty(name);
        model.textures = images.iter().map(texture_image).collect();
        // glTF material index -> ours; materials that set nothing are dropped.
        let mut material_map = Vec::new();
        for material in document.materials() {
            material_map.push(Material::from_gltf(&material).map(|m| {
                model.materials.push(m);
                model.materials.len() - 1
            }));
        }

        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next());
        match scene {
            Some(scene) => {
                for node in scene.nodes() {
                    model.add_node(&node, Mat4::IDENTITY, &buffers, &material_map);
                }
            }
            // No scene: take every mesh without transforms.
            None => {
                for mesh in document.meshes() {
                    model.add_mesh(&mesh, Mat4::IDENTITY, &buffers, &material_map);
                }
            }
        }

        if model.indices.is_empty() {
            return Err(LoadError::NoMeshes);
        }
        model.recompute_bounds();
        Ok(model)
    }

    fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            vertices: Vec::new(),
            indices: Vec::new(),
            bounds: Bounds::EMPTY,
            parts: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
        }
    }

    /// A model from ready vertices, as one part without a material.
    pub fn from_vertices(
        name: impl Into<String>,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
    ) -> Self {
        let mut model = Self::empty(name);
        model.parts = vec![MeshPart {
            first_index: 0,
            index_count: indices.len() as u32,
            material: None,
        }];
        model.vertices = vertices;
        model.indices = indices;
        model.recompute_bounds();
        model
    }

    /// Whether any part has a material of its own; models without are
    /// shown as Clay.
    pub fn has_materials(&self) -> bool {
        self.parts.iter().any(|p| p.material.is_some())
    }

    /// A model from triangle positions, with smooth normals.
    pub fn from_mesh(name: impl Into<String>, positions: Vec<Vec3>, indices: Vec<u32>) -> Self {
        let normals = smooth_normals(&positions, &indices);
        let vertices = positions
            .iter()
            .zip(&normals)
            .map(|(p, n)| Vertex {
                position: p.to_array(),
                normal: n.to_array(),
                uv: [0.0; 2],
            })
            .collect();
        Self::from_vertices(name, vertices, indices)
    }

    /// Whether the model is shaped like a standing human body: at least
    /// [`FIGURE_ASPECT`] times taller than it is wide or deep.
    pub fn looks_like_figure(&self) -> bool {
        let size = self.bounds.size();
        size.y > 0.0 && size.y >= FIGURE_ASPECT * size.x.max(size.z)
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Parameter `t` of the nearest triangle hit by the ray
    /// `origin + t * direction` (model space), or `None`.
    pub fn ray_hit(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let mut nearest: Option<f32> = None;
        for tri in self.indices.as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| Vec3::from(self.vertices[i as usize].position));
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

    fn add_node(
        &mut self,
        node: &gltf::Node,
        parent: Mat4,
        buffers: &[gltf::buffer::Data],
        materials: &[Option<usize>],
    ) {
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
            self.add_mesh(&mesh, mesh_transform, buffers, materials);
        }
        for child in node.children() {
            self.add_node(&child, world, buffers, materials);
        }
    }

    fn add_mesh(
        &mut self,
        mesh: &gltf::Mesh,
        transform: Mat4,
        buffers: &[gltf::buffer::Data],
        materials: &[Option<usize>],
    ) {
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

            let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
                Some(uvs) => uvs.into_f32().collect(),
                None => vec![[0.0; 2]; positions.len()],
            };
            if uvs.len() != positions.len() {
                continue;
            }

            let base = self.vertices.len() as u32;
            let first_index = self.indices.len() as u32;
            self.vertices.extend(
                positions
                    .iter()
                    .zip(&normals)
                    .zip(&uvs)
                    .map(|((p, n), uv)| Vertex {
                        position: p.to_array(),
                        normal: n.to_array(),
                        uv: *uv,
                    }),
            );
            for tri in indices.as_chunks::<3>().0 {
                let [a, b, c] = tri.map(|i| i + base);
                if flip_winding {
                    self.indices.extend([a, c, b]);
                } else {
                    self.indices.extend([a, b, c]);
                }
            }
            let material = primitive
                .material()
                .index()
                .and_then(|i| materials.get(i).copied().flatten());
            self.parts.push(MeshPart {
                first_index,
                index_count: self.indices.len() as u32 - first_index,
                material,
            });
        }
    }
}

impl Material {
    /// Reads a glTF material, or `None` when it sets nothing beyond the
    /// defaults (as Blender exports an untouched material): such parts are
    /// shown as Clay rather than as the spec's default white metal.
    fn from_gltf(material: &gltf::Material) -> Option<Self> {
        let pbr = material.pbr_metallic_roughness();
        let base_color_texture = pbr
            .base_color_texture()
            .map(|t| t.texture().source().index());
        let metallic_roughness_texture = pbr
            .metallic_roughness_texture()
            .map(|t| t.texture().source().index());
        let normal = material.normal_texture();
        let sets_something = base_color_texture.is_some()
            || metallic_roughness_texture.is_some()
            || normal.is_some()
            || pbr.base_color_factor() != [1.0; 4]
            || pbr.metallic_factor() != 1.0
            || pbr.roughness_factor() != 1.0;
        sets_something.then(|| Self {
            name: material.name().unwrap_or_default().to_owned(),
            base_color: Vec4::from(pbr.base_color_factor()),
            base_color_texture,
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            metallic_roughness_texture,
            normal_scale: normal.as_ref().map_or(1.0, |n| n.scale()),
            normal_texture: normal.map(|n| n.texture().source().index()),
        })
    }
}

/// A decoded glTF image as 8-bit RGBA, whatever its format.
fn texture_image(image: &gltf::image::Data) -> TextureImage {
    use gltf::image::Format;
    let pixels = &image.pixels;
    let count = (image.width * image.height) as usize;
    let rgba = match image.format {
        Format::R8G8B8A8 => pixels.clone(),
        Format::R8G8B8 => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        Format::R8G8 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[r, g]| [r, g, 0, 255])
            .collect(),
        Format::R8 => pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        // 16-bit images: keep the high byte of each little-endian channel.
        Format::R16G16B16A16 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[_, high]| high)
            .collect(),
        Format::R16G16B16 => pixels
            .as_chunks::<6>()
            .0
            .iter()
            .flat_map(|p| [p[1], p[3], p[5], 255])
            .collect(),
        Format::R16G16 => pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[1], p[3], 0, 255])
            .collect(),
        Format::R16 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[_, high]| [high, high, high, 255])
            .collect(),
        // Float images: clamp to 0..1 and scale to 8 bits.
        Format::R32G32B32FLOAT | Format::R32G32B32A32FLOAT => {
            let channels = if image.format == Format::R32G32B32FLOAT {
                3
            } else {
                4
            };
            let values: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&bytes| (f32::from_le_bytes(bytes).clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect();
            values
                .chunks(channels)
                .flat_map(|c| [c[0], c[1], c[2], if channels == 4 { c[3] } else { 255 }])
                .collect()
        }
    };
    debug_assert_eq!(rgba.len(), count * 4);
    TextureImage {
        width: image.width,
        height: image.height,
        rgba,
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
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| i as usize);
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

    const TWO_MATERIALS: &[u8] = include_bytes!("../../../assets/models/test/two_materials.glb");

    #[test]
    fn loads_materials_and_textures_per_part() {
        let model = Model::from_gltf_slice("two", TWO_MATERIALS).unwrap();
        assert!(model.has_materials());
        assert_eq!(model.parts.len(), 2, "one part per glTF primitive");
        assert_eq!(
            model.parts.iter().map(|p| p.index_count).sum::<u32>() as usize,
            model.indices.len()
        );

        let red = &model.materials[model.parts[0].material.unwrap()];
        assert_eq!(red.name, "red");
        assert!(red.base_color.x > 0.7 && red.base_color.y < 0.1);
        assert!(red.base_color_texture.is_none());
        assert!((red.roughness - 0.7).abs() < 1e-6 && red.metallic == 0.0);

        let checker = &model.materials[model.parts[1].material.unwrap()];
        let texture = &model.textures[checker.base_color_texture.expect("a base colour texture")];
        assert_eq!((texture.width, texture.height), (4, 4));
        assert_eq!(texture.rgba.len(), 4 * 4 * 4);
        assert!(checker.normal_texture.is_some());
        assert!((checker.metallic - 0.5).abs() < 1e-6);

        let uv_spread = model
            .vertices
            .iter()
            .map(|v| v.uv[0] + v.uv[1])
            .fold(0.0f32, f32::max);
        assert!(uv_spread > 1.0, "texture coordinates are loaded");
    }

    #[test]
    fn a_material_that_sets_nothing_counts_as_none() {
        // The test figures carry an empty "sculpture" material from Blender.
        for bytes in [MALE, FEMALE] {
            let model = Model::from_gltf_slice("figure", bytes).unwrap();
            assert!(!model.has_materials());
            assert_eq!(model.parts.len(), 1);
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(Model::from_gltf_slice("bad", b"not a model").is_err());
    }
}
