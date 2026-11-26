//! The Atlas's content: the structures of a body (bones, muscles, joints),
//! each its own model, with names, read from a content pack (see
//! `tools/atlas/build_human_pack.py` for the format).

use std::sync::Arc;

use glam::{Mat4, Vec3};
use serde::Deserialize;

use crate::Model;

mod export;
mod fibres;
mod masses;
mod view;

pub use export::{ATTRIBUTION, to_glb};
pub use fibres::Fibres;

pub use masses::{DEEP_MUSCLE_SRGB, MASSES, Mass, NEIGHBOURS, mass_of};
pub use view::{AtlasColors, AtlasView, Layer, LayerState, SKULL, ShownStructure, StructureIndex};

/// The body system a structure belongs to: one of the Atlas's layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum BodySystem {
    Skeletal,
    Muscular,
    Joints,
}

impl BodySystem {
    pub const ALL: [Self; 3] = [Self::Skeletal, Self::Muscular, Self::Joints];
}

/// What a structure is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Tissue {
    Bone,
    Cartilage,
    Tooth,
    Muscle,
    Tendon,
    Fascia,
    Ligament,
    Bursa,
    Capsule,
    Fat,
}

/// Which side of the body a structure is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    /// On the midline, or unpaired.
    Middle,
}

/// One structure of the body: a bone, a muscle, a ligament, …
#[derive(Debug, Clone)]
pub struct Structure {
    /// The source's name, unique in the pack: "Femur.l".
    pub id: String,
    /// The English name without its side: "Femur".
    pub name: String,
    pub side: Side,
    pub system: BodySystem,
    pub tissue: Tissue,
    /// The Latin term, where the source has one.
    pub latin: Option<String>,
    /// The id of the structure this one is part of, if any.
    pub parent: Option<String>,
    /// The Russian name, where the pack has one.
    pub russian: Option<TranslatedName>,
    /// In place in the body: metres, Y up, the body facing +Z.
    pub model: Arc<Model>,
}

/// A structure's name in another language, without its side.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TranslatedName {
    pub name: String,
    /// Still waiting for a person who knows the terminology to check it.
    pub review: bool,
}

/// The pack's Russian names, by the English name without the side.
pub const RUSSIAN_NAMES: &str = "names.ru.ron";

/// Errors reading a content pack.
#[derive(Debug, thiserror::Error)]
pub enum AtlasError {
    #[error("invalid atlas manifest: {0}")]
    Manifest(#[from] ron::error::SpannedError),
    #[error("unsupported atlas pack version {0}")]
    Version(u32),
    #[error("atlas data file {0} is missing")]
    MissingFile(String),
    #[error("atlas data file {0} is too short")]
    ShortFile(String),
    #[error("atlas structure {0} refers to an unknown mesh")]
    UnknownMesh(String),
    #[error("invalid atlas names: {0}")]
    Names(ron::error::SpannedError),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: u32,
    meshes: Vec<MeshEntry>,
    structures: Vec<StructureEntry>,
}

#[derive(Debug, Deserialize)]
struct MeshEntry {
    file: String,
    offset: usize,
    vertices: usize,
    triangles: usize,
    wide: bool,
    min: (f32, f32, f32),
    max: (f32, f32, f32),
}

#[derive(Debug, Deserialize)]
struct StructureEntry {
    id: String,
    system: BodySystem,
    tissue: Tissue,
    mesh: usize,
    /// Column-major, as glTF.
    matrix: [f32; 16],
    latin: Option<String>,
    parent: Option<String>,
}

/// A content pack's manifest, before its data files are read.
#[derive(Debug)]
pub struct AtlasPack {
    manifest: Manifest,
}

/// The manifest's file name inside a pack.
pub const ATLAS_MANIFEST: &str = "atlas.ron";

impl AtlasPack {
    pub fn parse(manifest: &[u8]) -> Result<Self, AtlasError> {
        let manifest: Manifest = ron::de::from_bytes(manifest)?;
        if manifest.version != 1 {
            return Err(AtlasError::Version(manifest.version));
        }
        Ok(Self { manifest })
    }

    /// The data files the pack needs, each once.
    pub fn files(&self) -> Vec<&str> {
        let mut files: Vec<&str> = Vec::new();
        for mesh in &self.manifest.meshes {
            if !files.contains(&mesh.file.as_str()) {
                files.push(&mesh.file);
            }
        }
        files
    }

    /// Reads the meshes from the data files `file` gives by name and puts
    /// every structure in place. The Russian names ([`RUSSIAN_NAMES`]) are
    /// taken too when `file` has them.
    pub fn build(
        &self,
        mut file: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Result<Atlas, AtlasError> {
        let russian: std::collections::HashMap<String, TranslatedName> = match file(RUSSIAN_NAMES) {
            Some(bytes) => {
                let list: Vec<(String, TranslatedName)> =
                    ron::de::from_bytes(&bytes).map_err(AtlasError::Names)?;
                list.into_iter().collect()
            }
            None => Default::default(),
        };
        let mut data = std::collections::HashMap::new();
        for name in self.files() {
            let bytes = file(name).ok_or_else(|| AtlasError::MissingFile(name.to_owned()))?;
            data.insert(name.to_owned(), bytes);
        }
        let meshes = self
            .manifest
            .meshes
            .iter()
            .map(|mesh| read_mesh(mesh, &data[&mesh.file]))
            .collect::<Result<Vec<_>, _>>()?;
        let structures = self
            .manifest
            .structures
            .iter()
            .map(|entry| {
                let (positions, indices) = meshes
                    .get(entry.mesh)
                    .ok_or_else(|| AtlasError::UnknownMesh(entry.id.clone()))?;
                let (name, side) = split_side(&entry.id);
                Ok(Structure {
                    model: Arc::new(placed(&entry.id, positions, indices, entry.matrix)),
                    id: entry.id.clone(),
                    name,
                    side,
                    system: entry.system,
                    tissue: entry.tissue,
                    latin: entry.latin.clone(),
                    parent: entry.parent.clone(),
                    russian: russian.get(&split_side(&entry.id).0).cloned(),
                })
            })
            .collect::<Result<Vec<_>, AtlasError>>()?;
        Ok(Atlas { structures })
    }
}

/// A body's structures, ready to draw.
#[derive(Debug, Clone, Default)]
pub struct Atlas {
    structures: Vec<Structure>,
}

impl Atlas {
    pub fn structures(&self) -> &[Structure] {
        &self.structures
    }

    pub fn structure(&self, id: &str) -> Option<&Structure> {
        self.structures.iter().find(|s| s.id == id)
    }
}

/// "Femur.l" is the left femur; "Sternum" has no side.
fn split_side(id: &str) -> (String, Side) {
    if let Some(name) = id.strip_suffix(".l") {
        (name.to_owned(), Side::Left)
    } else if let Some(name) = id.strip_suffix(".r") {
        (name.to_owned(), Side::Right)
    } else {
        (id.to_owned(), Side::Middle)
    }
}

/// A mesh's positions (in its own space) and triangle indices.
fn read_mesh(mesh: &MeshEntry, data: &[u8]) -> Result<(Vec<Vec3>, Vec<u32>), AtlasError> {
    let short = || AtlasError::ShortFile(mesh.file.clone());
    let positions_end = mesh.offset + mesh.vertices * 6;
    let indices_start = positions_end.next_multiple_of(4);
    let index_size = if mesh.wide { 4 } else { 2 };
    let indices_end = indices_start + mesh.triangles * 3 * index_size;
    let positions_bytes = data.get(mesh.offset..positions_end).ok_or_else(short)?;
    let index_bytes = data.get(indices_start..indices_end).ok_or_else(short)?;
    let min = Vec3::from(mesh.min);
    let span = (Vec3::from(mesh.max) - min) / 65535.0;
    let positions = positions_bytes
        .as_chunks::<6>()
        .0
        .iter()
        .map(|b| {
            let q = |i: usize| f32::from(u16::from_le_bytes([b[i], b[i + 1]]));
            min + Vec3::new(q(0), q(2), q(4)) * span
        })
        .collect();
    let indices = if mesh.wide {
        index_bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect()
    } else {
        index_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u32::from(u16::from_le_bytes(*b)))
            .collect()
    };
    Ok((positions, indices))
}

/// The mesh moved into place. A mirrored placement turns the triangles
/// round so they still face outwards.
fn placed(id: &str, positions: &[Vec3], indices: &[u32], matrix: [f32; 16]) -> Model {
    let matrix = Mat4::from_cols_array(&matrix);
    let positions = positions
        .iter()
        .map(|&p| matrix.transform_point3(p))
        .collect();
    let mut indices = indices.to_vec();
    if matrix.determinant() < 0.0 {
        for triangle in indices.as_chunks_mut::<3>().0 {
            triangle.swap(1, 2);
        }
    }
    Model::from_mesh(id, positions, indices)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The human pack, read once for all tests.
    pub(crate) fn human() -> Atlas {
        static HUMAN: std::sync::OnceLock<Atlas> = std::sync::OnceLock::new();
        HUMAN.get_or_init(read_human).clone()
    }

    fn read_human() -> Atlas {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/atlas/human/");
        let read = |name: &str| std::fs::read(format!("{directory}{name}")).ok();
        let pack = AtlasPack::parse(&read(ATLAS_MANIFEST).unwrap()).unwrap();
        pack.build(read).unwrap()
    }

    #[test]
    fn the_human_pack_has_its_bones_muscles_and_joints() {
        let atlas = human();
        let count = |system| {
            atlas
                .structures()
                .iter()
                .filter(|s| s.system == system)
                .count()
        };
        assert_eq!(count(BodySystem::Skeletal), 277);
        assert_eq!(count(BodySystem::Muscular), 669);
        assert_eq!(count(BodySystem::Joints), 349);
        let muscles = atlas
            .structures()
            .iter()
            .filter(|s| s.tissue == Tissue::Muscle)
            .count();
        assert!(muscles > 400, "{muscles} muscles");
        let mut ids: Vec<_> = atlas.structures().iter().map(|s| &s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), atlas.structures().len(), "ids are unique");
    }

    #[test]
    fn structures_stand_in_a_human_body_in_metres_with_y_up() {
        let atlas = human();
        let skull = atlas.structure("Frontal bone").unwrap();
        let foot = atlas.structure("Calcaneus.l").unwrap();
        assert!(
            (1.5..1.9).contains(&skull.model.bounds.max.y),
            "the head is at a person's height: {}",
            skull.model.bounds.max.y
        );
        assert!(foot.model.bounds.min.y < 0.15, "the heel is on the ground");
        let size = skull.model.bounds.size();
        assert!(
            (0.08..0.2).contains(&size.x),
            "a frontal bone is about 13 cm wide: {size}"
        );
    }

    #[test]
    fn left_and_right_are_mirror_images() {
        let atlas = human();
        let left = atlas.structure("Femur.l").unwrap();
        let right = atlas.structure("Femur.r").unwrap();
        assert_eq!((left.side, right.side), (Side::Left, Side::Right));
        assert_eq!(left.name, right.name);
        assert_ne!(left.id, right.id);
        let (l, r) = (left.model.bounds.center(), right.model.bounds.center());
        assert!(l.x * r.x < 0.0, "on opposite sides: {l} {r}");
        assert!((l.x + r.x).abs() < 0.01 && (l.y - r.y).abs() < 0.01);
        // Mirrored triangles still face outwards: the normal at the
        // outermost vertex points away from the body's midline.
        for femur in [left, right] {
            let outer = femur
                .model
                .vertices
                .iter()
                .max_by(|a, b| a.position[0].abs().total_cmp(&b.position[0].abs()))
                .unwrap();
            assert!(outer.normal[0] * outer.position[0] > 0.0, "{}", femur.id);
        }
    }

    #[test]
    fn structures_have_english_names_and_mostly_latin_terms() {
        let atlas = human();
        assert!(atlas.structures().iter().all(|s| !s.name.is_empty()));
        let latin = atlas
            .structures()
            .iter()
            .filter(|s| s.latin.is_some())
            .count();
        assert!(
            latin * 10 > atlas.structures().len() * 8,
            "{latin} Latin terms"
        );
        let masseter = atlas.structure("Superficial part of masseter.l");
        let pectoralis = atlas
            .structures()
            .iter()
            .find(|s| s.name.contains("pectoralis major") && s.latin.is_some());
        assert!(masseter.is_some() || pectoralis.is_some());
        assert!(
            pectoralis
                .unwrap()
                .latin
                .as_ref()
                .unwrap()
                .contains("pectoralis")
        );
    }

    #[test]
    fn every_bone_and_muscle_has_a_russian_name_marked_for_review() {
        let atlas = human();
        for structure in atlas.structures() {
            if structure.system == BodySystem::Joints {
                continue;
            }
            let russian = structure
                .russian
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no Russian name", structure.id));
            assert!(!russian.name.is_empty());
            assert!(russian.review, "{} is marked for review", structure.id);
        }
        let ru = |id: &str| atlas.structure(id).unwrap().russian.clone().unwrap().name;
        assert_eq!(ru("Femur.l"), "Бедренная кость");
        assert_eq!(ru("Fifth rib.l"), "Пятое ребро");
        assert_eq!(ru("Vertebra T7"), "Грудной позвонок T7");
        assert_eq!(
            ru("Distal phalanx of fifth finger of hand.r"),
            "Дистальная фаланга пятого пальца кисти"
        );
    }

    #[test]
    fn a_missing_data_file_is_an_error() {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/atlas/human/");
        let manifest = std::fs::read(format!("{directory}{ATLAS_MANIFEST}")).unwrap();
        let pack = AtlasPack::parse(&manifest).unwrap();
        assert_eq!(pack.files(), ["skeletal.bin", "muscular.bin", "joints.bin"]);
        assert!(matches!(
            pack.build(|_| None),
            Err(AtlasError::MissingFile(_))
        ));
        assert!(AtlasPack::parse(b"(version: 2, meshes: [], structures: [])").is_err());
    }
}
