//! OBJ models with their MTL materials, mapped onto the glTF material model.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::BufReader;

use glam::{Vec3, Vec4};

use super::{LoadError, Material, MeshPart, Model, TextureImage, Vertex, smooth_normals};

/// An OBJ model and the files it read besides itself.
#[derive(Debug)]
pub struct ObjImport {
    pub model: Model,
    /// The MTL and texture files that were found, by the name the OBJ or
    /// MTL file gives them, in the order they were read.
    pub files: Vec<(String, Vec<u8>)>,
    /// Files the OBJ or MTL file names that were not found.
    pub missing: Vec<String>,
}

/// Reads companion files by the name an OBJ or MTL file gives them.
struct Files<F> {
    lookup: F,
    found: Vec<(String, Vec<u8>)>,
    missing: Vec<String>,
}

impl<F: FnMut(&str) -> Option<Vec<u8>>> Files<F> {
    fn get(&mut self, name: &str) -> Option<Vec<u8>> {
        if let Some((_, bytes)) = self.found.iter().find(|(n, _)| n == name) {
            return Some(bytes.clone());
        }
        match (self.lookup)(name) {
            Some(bytes) => {
                self.found.push((name.to_owned(), bytes.clone()));
                Some(bytes)
            }
            None => {
                if !self.missing.iter().any(|n| n == name) {
                    self.missing.push(name.to_owned());
                }
                None
            }
        }
    }
}

impl Model {
    /// Loads an OBJ model. `file` returns the bytes of a file the OBJ or
    /// its MTL files name (relative to the OBJ, as written there), or
    /// `None` when it is not available: parts whose material is missing
    /// are shown as Clay, and textures that are missing are left out.
    pub fn from_obj(
        name: impl Into<String>,
        bytes: &[u8],
        file: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Result<ObjImport, LoadError> {
        let files = RefCell::new(Files {
            lookup: file,
            found: Vec::new(),
            missing: Vec::new(),
        });
        let options = tobj::LoadOptions {
            single_index: true,
            triangulate: true,
            ignore_points: true,
            ignore_lines: true,
        };
        let (meshes, materials) =
            tobj::load_obj_buf(&mut BufReader::new(bytes), &options, |path| {
                let name = path.to_string_lossy();
                match files.borrow_mut().get(&name) {
                    Some(mtl) => tobj::load_mtl_buf(&mut BufReader::new(mtl.as_slice())),
                    None => Err(tobj::LoadError::OpenFileFailed),
                }
            })
            .map_err(|error| LoadError::Obj(error.to_string()))?;
        // A broken or missing MTL file leaves every part as Clay.
        let obj_materials = materials.unwrap_or_default();

        let mut model = Model::empty(name);
        let mut textures: HashMap<String, Option<usize>> = HashMap::new();
        let mut texture = |reference: &Option<String>, model: &mut Model| {
            let name = texture_file_name(reference.as_deref()?)?;
            *textures.entry(name.clone()).or_insert_with(|| {
                let bytes = files.borrow_mut().get(&name)?;
                match decode_texture(&bytes) {
                    Ok(image) => {
                        model.textures.push(image);
                        Some(model.textures.len() - 1)
                    }
                    Err(error) => {
                        log::warn!("OBJ texture {name} could not be read: {error}");
                        None
                    }
                }
            })
        };
        for material in &obj_materials {
            let normal = material.unknown_param.get("norm").cloned();
            let converted = Material {
                name: material.name.clone(),
                base_color: base_color(material),
                base_color_texture: texture(&material.diffuse_texture, &mut model),
                metallic: parameter(material, "Pm").unwrap_or(0.0).clamp(0.0, 1.0),
                roughness: roughness(material),
                metallic_roughness_texture: None,
                normal_texture: texture(&normal, &mut model),
                normal_scale: 1.0,
            };
            model.materials.push(converted);
        }

        for mesh in meshes.iter().map(|m| &m.mesh) {
            if mesh.indices.is_empty() {
                continue;
            }
            let positions: Vec<Vec3> = mesh
                .positions
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&p| Vec3::from(p))
                .collect();
            let normals: Vec<Vec3> = if mesh.normals.len() == mesh.positions.len() {
                mesh.normals
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|&n| Vec3::from(n).normalize_or(Vec3::Y))
                    .collect()
            } else {
                smooth_normals(&positions, &mesh.indices)
            };
            let has_uvs = mesh.texcoords.len() / 2 == positions.len();
            let first_vertex = model.vertices.len() as u32;
            model.vertices.extend(
                positions
                    .iter()
                    .zip(&normals)
                    .enumerate()
                    .map(|(i, (p, n))| {
                        // OBJ texture rows run upwards, glTF ones downwards.
                        let uv = if has_uvs {
                            [mesh.texcoords[2 * i], 1.0 - mesh.texcoords[2 * i + 1]]
                        } else {
                            [0.0; 2]
                        };
                        Vertex {
                            position: p.to_array(),
                            normal: n.to_array(),
                            uv,
                        }
                    }),
            );
            let first_index = model.indices.len() as u32;
            model
                .indices
                .extend(mesh.indices.iter().map(|i| first_vertex + i));
            model.parts.push(MeshPart {
                first_index,
                index_count: mesh.indices.len() as u32,
                material: mesh.material_id.filter(|&id| id < model.materials.len()),
            });
        }

        if model.indices.is_empty() {
            return Err(LoadError::NoMeshes);
        }
        model.recompute_bounds();
        let Files { found, missing, .. } = files.into_inner();
        Ok(ObjImport {
            model,
            files: found,
            missing,
        })
    }
}

/// The diffuse colour, read as sRGB like most viewers do, with its opacity.
fn base_color(material: &tobj::Material) -> Vec4 {
    let [r, g, b] = material.diffuse.unwrap_or([0.8; 3]);
    let opacity = material
        .dissolve
        .or_else(|| parameter(material, "Tr").map(|t| 1.0 - t))
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    Vec4::new(
        srgb_to_linear(r),
        srgb_to_linear(g),
        srgb_to_linear(b),
        opacity,
    )
}

/// The PBR extension's roughness, or one from the Phong exponent the way
/// Blender reads it.
fn roughness(material: &tobj::Material) -> f32 {
    parameter(material, "Pr")
        .or_else(|| {
            material
                .shininess
                .map(|ns| 1.0 - (ns.clamp(0.0, 1000.0) / 1000.0).sqrt())
        })
        .unwrap_or(0.8)
        .clamp(0.0, 1.0)
}

fn parameter(material: &tobj::Material, key: &str) -> Option<f32> {
    material.unknown_param.get(key)?.trim().parse().ok()
}

fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The file name in a texture statement, past its options
/// (`-s 1 1 1 -clamp on skin.png` gives `skin.png`).
fn texture_file_name(statement: &str) -> Option<String> {
    let tokens: Vec<&str> = statement.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() && tokens[i].starts_with('-') {
        let option = tokens[i];
        i += 1;
        match option {
            // Options taking one word.
            "-imfchan" | "-type" | "-texres" => i += 1,
            _ => {
                while i + 1 < tokens.len()
                    && (tokens[i].parse::<f32>().is_ok() || matches!(tokens[i], "on" | "off"))
                {
                    i += 1;
                }
            }
        }
    }
    let name = tokens.get(i..)?.join(" ");
    (!name.is_empty()).then_some(name)
}

fn decode_texture(bytes: &[u8]) -> Result<TextureImage, image::ImageError> {
    let image = image::load_from_memory(bytes)?.to_rgba8();
    Ok(TextureImage {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE_OBJ: &str = "\
mtllib cube.mtl
o Cube
v -1 -1 -1
v  1 -1 -1
v  1  1 -1
v -1  1 -1
v -1 -1  1
v  1 -1  1
v  1  1  1
v -1  1  1
vt 0 0
vt 1 0
vt 1 1
vt 0 1
usemtl Red
f 1/1 4/4 3/3 2/2
f 5/1 6/2 7/3 8/4
f 1/1 5/2 8/3 4/4
usemtl Blue
f 2/1 3/2 7/3 6/4
f 4/1 8/2 7/3 3/4
f 1/1 2/2 6/3 5/4
";

    const CUBE_MTL: &str = "\
newmtl Red
Kd 1.0 0.0 0.0
Ns 250
newmtl Blue
Kd 0 0 1
d 0.5
map_Kd -s 1 1 1 -clamp on textures/blue tile.png
";

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 200, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn an_obj_with_its_mtl_has_its_shape_and_materials() {
        let texture = png();
        let mut asked = Vec::new();
        let import = Model::from_obj("cube", CUBE_OBJ.as_bytes(), |name| {
            asked.push(name.to_owned());
            match name {
                "cube.mtl" => Some(CUBE_MTL.as_bytes().to_vec()),
                "textures/blue tile.png" => Some(texture.clone()),
                _ => None,
            }
        })
        .unwrap();
        let model = &import.model;
        assert_eq!(model.triangle_count(), 12, "six quads in triangles");
        assert_eq!(model.bounds.size(), Vec3::splat(2.0));
        assert_eq!(model.materials.len(), 2);
        assert_eq!(model.parts.len(), 2, "one part per material");
        let red = &model.materials[model.parts[0].material.unwrap()];
        assert_eq!(red.name, "Red");
        assert_eq!(red.base_color, Vec4::new(1.0, 0.0, 0.0, 1.0));
        assert!(
            (red.roughness - 0.5).abs() < 1e-6,
            "Ns 250 is fairly glossy"
        );
        let blue = &model.materials[model.parts[1].material.unwrap()];
        assert_eq!(blue.base_color.w, 0.5);
        let texture = &model.textures[blue.base_color_texture.unwrap()];
        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(texture.rgba[..4], [10, 20, 200, 255]);
        // Rows flipped: OBJ's v = 0 is the bottom of the image.
        assert!(model.vertices.iter().any(|v| v.uv == [0.0, 1.0]));
        assert!(import.missing.is_empty());
        let found: Vec<_> = import.files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(found, ["cube.mtl", "textures/blue tile.png"]);
        assert_eq!(asked, found, "each file read once");
    }

    #[test]
    fn an_obj_without_its_mtl_loads_as_clay_and_says_what_is_missing() {
        let import = Model::from_obj("cube", CUBE_OBJ.as_bytes(), |_| None).unwrap();
        assert_eq!(import.model.triangle_count(), 12);
        assert!(!import.model.has_materials());
        assert_eq!(import.missing, ["cube.mtl"]);
        assert!(import.files.is_empty());
    }

    #[test]
    fn an_obj_naming_no_mtl_has_no_materials_and_nothing_missing() {
        let plain = CUBE_OBJ.replace("mtllib cube.mtl\n", "");
        let import = Model::from_obj("cube", plain.as_bytes(), |_| unreachable!()).unwrap();
        assert!(!import.model.has_materials());
        assert!(import.missing.is_empty());
        assert!(
            import
                .model
                .vertices
                .iter()
                .all(|v| (Vec3::from(v.normal).length() - 1.0).abs() < 1e-4),
            "normals computed when the file has none"
        );
    }

    #[test]
    fn a_missing_texture_keeps_the_material_colour() {
        let import = Model::from_obj("cube", CUBE_OBJ.as_bytes(), |name| {
            (name == "cube.mtl").then(|| CUBE_MTL.as_bytes().to_vec())
        })
        .unwrap();
        assert_eq!(import.model.materials.len(), 2);
        assert!(import.model.materials[1].base_color_texture.is_none());
        assert_eq!(import.missing, ["textures/blue tile.png"]);
    }

    #[test]
    fn texture_statements_lose_their_options() {
        assert_eq!(texture_file_name("skin.png").as_deref(), Some("skin.png"));
        assert_eq!(
            texture_file_name("-o 0.5 0.5 -bm 0.3 my skin.tga").as_deref(),
            Some("my skin.tga")
        );
        assert_eq!(
            texture_file_name("-imfchan r spec.png").as_deref(),
            Some("spec.png")
        );
        assert_eq!(texture_file_name("-clamp on").as_deref(), Some("on"));
    }

    #[test]
    fn text_that_is_no_model_is_an_error() {
        assert!(Model::from_obj("bad", b"not a model", |_| None).is_err());
    }
}
