//! What the Atlas shows, as one model for the Studio, written as binary
//! glTF so it opens, saves and embeds like any model.

use glam::Vec3;

use super::view::AtlasView;
use crate::{Material, MeshPart, Model, Vertex};

/// The attribution every model taken from the Atlas carries, in its glTF
/// `asset.copyright`.
pub const ATTRIBUTION: &str = "Z-Anatomy - The open source atlas of anatomy - CC-BY-SA 4.0; \
    BodyParts3D, (c) The Database Center for Life Science licensed under CC Attribution 4.0 International";

impl AtlasView {
    /// The selected structure, or everything shown when nothing is, as one
    /// model in place (metres), each colour a material of its own.
    pub fn to_model(&self, name: &str) -> Option<Model> {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut parts: Vec<MeshPart> = Vec::new();
        let mut colors: Vec<Vec3> = Vec::new();
        let chosen: Vec<_> = match self.selected() {
            Some(index) => self.shown().filter(|s| s.index == index).collect(),
            None => self.shown().collect(),
        };
        for shown in chosen {
            let structure = shown.structure;
            let model = shown.model;
            let first_vertex = vertices.len() as u32;
            let first_index = indices.len() as u32;
            vertices.extend(model.vertices.iter().map(|v| {
                Vertex {
                    position: shown
                        .transform
                        .transform_point3(Vec3::from(v.position))
                        .to_array(),
                    normal: shown
                        .transform
                        .transform_vector3(Vec3::from(v.normal))
                        .to_array(),
                    uv: v.uv,
                }
            }));
            indices.extend(model.indices.iter().map(|i| i + first_vertex));
            // A part per tissue: the belly in its colour, the tendon pale.
            for part in &model.parts {
                let tissue = part
                    .material
                    .and_then(|m| structure.tissues.get(m))
                    .copied()
                    .unwrap_or(structure.tissue);
                let color = self.tissue_color(structure, tissue);
                let material = colors.iter().position(|&c| c == color).unwrap_or_else(|| {
                    colors.push(color);
                    colors.len() - 1
                });
                parts.push(MeshPart {
                    first_index: first_index + part.first_index,
                    index_count: part.index_count,
                    material: Some(material),
                });
            }
        }
        if indices.is_empty() {
            return None;
        }
        let mut model = Model::from_vertices(name, vertices, indices);
        model.parts = parts;
        model.materials = colors
            .iter()
            .enumerate()
            .map(|(i, &c)| Material {
                name: format!("atlas-{i}"),
                base_color: c.extend(1.0),
                base_color_texture: None,
                metallic: 0.0,
                roughness: 0.7,
                metallic_roughness_texture: None,
                normal_texture: None,
                normal_scale: 1.0,
            })
            .collect();
        Some(model)
    }
}

/// `model` as binary glTF: positions, normals, one primitive per part
/// with its material's colour, and `copyright` in the asset.
pub fn to_glb(model: &Model, copyright: &str) -> Vec<u8> {
    let mut binary: Vec<u8> = Vec::new();
    for v in &model.vertices {
        for c in v.position {
            binary.extend_from_slice(&c.to_le_bytes());
        }
    }
    let normals_offset = binary.len();
    for v in &model.vertices {
        for c in v.normal {
            binary.extend_from_slice(&c.to_le_bytes());
        }
    }
    let indices_offset = binary.len();
    for i in &model.indices {
        binary.extend_from_slice(&i.to_le_bytes());
    }
    let count = model.vertices.len();
    let bounds = model.bounds;
    let mut accessors = vec![
        format!(
            r#"{{"bufferView":0,"componentType":5126,"count":{count},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}"#,
            bounds.min.x, bounds.min.y, bounds.min.z, bounds.max.x, bounds.max.y, bounds.max.z
        ),
        format!(r#"{{"bufferView":1,"componentType":5126,"count":{count},"type":"VEC3"}}"#),
    ];
    let mut primitives = Vec::new();
    for part in &model.parts {
        accessors.push(format!(
            r#"{{"bufferView":2,"byteOffset":{},"componentType":5125,"count":{},"type":"SCALAR"}}"#,
            part.first_index * 4,
            part.index_count
        ));
        let material = part
            .material
            .map(|m| format!(r#","material":{m}"#))
            .unwrap_or_default();
        primitives.push(format!(
            r#"{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":{}{material}}}"#,
            accessors.len() - 1
        ));
    }
    let materials: Vec<String> = model
        .materials
        .iter()
        .map(|m| {
            let c = m.base_color;
            format!(
                r#"{{"name":{},"pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},{}],"metallicFactor":{},"roughnessFactor":{}}}}}"#,
                json_string(&m.name),
                c.x,
                c.y,
                c.z,
                c.w,
                m.metallic,
                m.roughness
            )
        })
        .collect();
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"HoudonRef Atlas","copyright":{}}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":{}}}],"meshes":[{{"primitives":[{}]}}],"materials":[{}],"accessors":[{}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{}}},{{"buffer":0,"byteOffset":{},"byteLength":{}}},{{"buffer":0,"byteOffset":{},"byteLength":{},"target":34963}}],"buffers":[{{"byteLength":{}}}]}}"#,
        json_string(copyright),
        json_string(&model.name),
        primitives.join(","),
        materials.join(","),
        accessors.join(","),
        normals_offset,
        normals_offset,
        indices_offset - normals_offset,
        indices_offset,
        binary.len() - indices_offset,
        binary.len()
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let total = 12 + 8 + json.len() + 8 + binary.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json);
    glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&binary);
    glb
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::AtlasColors;
    use crate::atlas::tests::human;

    #[test]
    fn the_isolated_skull_opens_in_the_studio_at_its_size_with_its_colours() {
        let mut view = AtlasView::new(human());
        view.colors = AtlasColors::Bones;
        view.isolate(view.skull());
        let model = view.to_model("Atlas: skull").unwrap();
        let skull_size = view.shown_bounds().size();
        assert!((model.bounds.size() - skull_size).length() < 1e-4);
        let glb = to_glb(&model, ATTRIBUTION);
        let back = Model::from_gltf_slice("Atlas: skull", &glb).unwrap();
        assert_eq!(back.triangle_count(), model.triangle_count());
        assert!(
            (back.bounds.size() - skull_size).length() < 1e-4,
            "real size"
        );
        assert!(back.materials.len() > 8, "a colour per kind of bone");
        let frontal = view.color(view.atlas().structure("Frontal bone").unwrap());
        assert!(
            back.materials
                .iter()
                .any(|m| (m.base_color.truncate() - frontal).length() < 1e-5),
            "the frontal bone's colour"
        );
        let text = String::from_utf8_lossy(&glb);
        assert!(
            text.contains("Z-Anatomy") && text.contains("BodyParts3D"),
            "attribution"
        );
    }

    #[test]
    fn a_selection_goes_alone() {
        let mut view = AtlasView::new(human());
        let femur = view.index_of("Femur.l").unwrap();
        view.select(Some(femur));
        let model = view.to_model("Femur").unwrap();
        assert_eq!(
            model.triangle_count(),
            view.atlas().structures()[femur].model.triangle_count()
        );
        view.isolate([femur]);
        view.hide(femur);
        assert!(view.to_model("nothing").is_none() || view.shown().count() > 0);
    }
}
