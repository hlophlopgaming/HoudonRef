//! Pose the standing Z-Anatomy pack in its own coordinate system.

use std::{collections::HashMap, sync::Arc};

use glam::{EulerRot, Mat4, Quat, Vec3};

use super::{Atlas, Fibres, Side, Structure, Tissue};
use crate::{Bounds, Model, ModelId};

const PELVIS: usize = 0;
const WAIST: usize = 1;
const CHEST: usize = 2;
const NECK: usize = 3;
const HEAD: usize = 4;
const JAW: usize = 5;

const GIRDLE: usize = 0;
const SHOULDER: usize = 1;
const ELBOW: usize = 2;
const WRIST: usize = 3;
const HIP: usize = 4;
const KNEE: usize = 5;
const ANKLE: usize = 6;

pub struct PoseJoint {
    pub name: &'static str,
    pub side: Side,
    /// Local XYZ angles in radians.
    pub angles: Vec3,
    /// For digits, the wrist or ankle control and the phalanx number.
    pub digit: Option<(usize, u8)>,
    pivot: Vec3,
    parent: usize,
    matrix: Mat4,
}

impl PoseJoint {
    pub fn position(&self) -> Vec3 {
        self.matrix.transform_point3(self.pivot)
    }

    /// Approximate local XYZ limits, in degrees from the pack's rest pose.
    pub fn degree_limits(&self) -> (Vec3, Vec3) {
        let (min, max) = match self.name {
            "waist" => ([-30., -25., -25.], [30., 25., 25.]),
            "chest" => ([-25., -35., -20.], [25., 35., 20.]),
            "neck" | "head" => ([-40., -55., -30.], [40., 55., 30.]),
            "jaw" => ([-30., -8., -5.], [8., 8., 5.]),
            "girdle" => ([-25., -30., -30.], [25., 30., 30.]),
            "elbow" => ([-145., -65., -20.], [15., 65., 20.]),
            "wrist" => ([-80., -35., -40.], [80., 35., 40.]),
            "knee" => ([-10., -20., -15.], [145., 20., 15.]),
            "ankle" => ([-45., -25., -35.], [55., 25., 35.]),
            _ => ([-180.; 3], [180.; 3]),
        };
        (min.into(), max.into())
    }
}

#[derive(Clone, Copy)]
enum Binding {
    Rigid(usize),
    AbdominalWall,
    Muscle {
        start: usize,
        end: usize,
        origin: Vec3,
        insertion: Vec3,
    },
}

struct Deformed {
    relative: [Mat4; 2],
    model: Arc<Model>,
    fibres: Option<Fibres>,
}

pub(super) struct Pose {
    pub joints: Vec<PoseJoint>,
    bindings: Vec<Binding>,
    deformed: Vec<Option<Deformed>>,
}

impl Pose {
    pub fn new(atlas: &Atlas) -> Option<Self> {
        // These pivots and bindings are calibrated to the carried human pack.
        atlas.structure("Humerus.l")?;
        atlas.structure("Femur.r")?;
        atlas.structure("Atlas (C1)")?;
        let mut joints = Vec::new();
        let mut add = |name, side, parent, pivot| {
            joints.push(PoseJoint {
                name,
                side,
                angles: Vec3::ZERO,
                digit: None,
                pivot: Vec3::from_array(pivot),
                parent,
                matrix: Mat4::IDENTITY,
            });
        };
        add("pelvis", Side::Middle, PELVIS, [0.0, 0.90, -0.021]);
        add("waist", Side::Middle, PELVIS, [0.0, 0.99, -0.025]);
        add("chest", Side::Middle, WAIST, [0.0, 1.14, -0.036]);
        add("neck", Side::Middle, CHEST, [0.0, 1.46, -0.032]);
        add("head", Side::Middle, NECK, [0.0, 1.552, -0.027]);
        add("jaw", Side::Middle, HEAD, [0.0, 1.570, -0.005]);

        for (side, sign) in [(Side::Left, 1.0), (Side::Right, -1.0)] {
            let base = limb_base(side);
            add("girdle", side, CHEST, [0.018 * sign, 1.408, 0.026]);
            add(
                "shoulder",
                side,
                base + GIRDLE,
                [0.163 * sign, 1.386, -0.025],
            );
            add(
                "elbow",
                side,
                base + SHOULDER,
                [0.210 * sign, 1.098, -0.033],
            );
            add("wrist", side, base + ELBOW, [0.263 * sign, 0.858, 0.017]);
            add("hip", side, PELVIS, [0.053 * sign, 0.859, -0.027]);
            add("knee", side, base + HIP, [0.098 * sign, 0.429, -0.019]);
            add("ankle", side, base + KNEE, [0.075 * sign, 0.079, -0.029]);
        }

        let mut phalanges = HashMap::new();
        for (side, suffix) in [(Side::Left, "l"), (Side::Right, "r")] {
            let base = limb_base(side);
            for (root, site, names) in [
                (
                    base + WRIST,
                    "hand",
                    ["thumb", "index", "middle", "ring", "little"],
                ),
                (
                    base + ANKLE,
                    "foot",
                    [
                        "big-toe",
                        "second-toe",
                        "third-toe",
                        "fourth-toe",
                        "little-toe",
                    ],
                ),
            ] {
                for (name, ordinal) in names
                    .into_iter()
                    .zip(["first", "second", "third", "fourth", "fifth"])
                {
                    let mut parent = root;
                    for part in ["Proximal", "Middle", "Distal"] {
                        let id = format!("{part} phalanx of {ordinal} finger of {site}.{suffix}");
                        let Some(bone) = atlas.structure(&id) else {
                            continue;
                        };
                        let center = bone.model.bounds.center();
                        let direction = (center - joints[parent].pivot).normalize_or(Vec3::NEG_Y);
                        let reach = bone
                            .model
                            .vertices
                            .iter()
                            .map(|v| (Vec3::from(v.position) - center).dot(direction))
                            .fold(0.0, f32::min);
                        phalanges.insert(bone.id.as_str(), joints.len());
                        joints.push(PoseJoint {
                            name,
                            side,
                            angles: Vec3::ZERO,
                            digit: Some((
                                root,
                                match part {
                                    "Proximal" => 1,
                                    "Middle" => 2,
                                    _ => 3,
                                },
                            )),
                            pivot: center + direction * reach,
                            parent,
                            matrix: Mat4::IDENTITY,
                        });
                        parent = joints.len() - 1;
                    }
                }
            }
        }

        let bindings = atlas
            .structures()
            .iter()
            .map(|s| {
                phalanges
                    .get(s.id.as_str())
                    .copied()
                    .map(Binding::Rigid)
                    .unwrap_or_else(|| bind(s))
            })
            .collect();
        let deformed = (0..atlas.structures().len()).map(|_| None).collect();
        Some(Self {
            joints,
            bindings,
            deformed,
        })
    }

    pub fn set_angles(
        &mut self,
        joint: usize,
        angles: Vec3,
        atlas: &Atlas,
        fibres: &[Option<Fibres>],
    ) {
        if !angles.is_finite() {
            return;
        }
        let (min, max) = self.joints[joint].degree_limits();
        let angles = angles.clamp(min.map(f32::to_radians), max.map(f32::to_radians));
        if self.joints[joint].angles == angles {
            return;
        }
        self.joints[joint].angles = angles;
        self.update(atlas, fibres);
    }

    pub fn rotate(&mut self, index: usize, delta: Quat, atlas: &Atlas, fibres: &[Option<Fibres>]) {
        let joint = &self.joints[index];
        let parent = if index == PELVIS {
            Quat::IDENTITY
        } else {
            Quat::from_mat4(&self.joints[joint.parent].matrix).normalize()
        };
        let a = joint.angles;
        let current = Quat::from_euler(EulerRot::XYZ, a.x, a.y, a.z);
        let (x, y, z) = (parent.conjugate() * delta * parent * current)
            .normalize()
            .to_euler(EulerRot::XYZ);
        self.set_angles(index, Vec3::new(x, y, z), atlas, fibres);
    }

    pub fn reset(&mut self) {
        for joint in &mut self.joints {
            joint.angles = Vec3::ZERO;
            joint.matrix = Mat4::IDENTITY;
        }
        self.deformed.iter_mut().for_each(|model| *model = None);
    }

    fn update(&mut self, atlas: &Atlas, fibres: &[Option<Fibres>]) {
        let mut changed = vec![false; self.joints.len()];
        for (index, flag) in changed.iter_mut().enumerate() {
            let joint = &self.joints[index];
            let parent = if index == PELVIS {
                Mat4::IDENTITY
            } else {
                self.joints[joint.parent].matrix
            };
            let rotation = Quat::from_euler(
                EulerRot::XYZ,
                joint.angles.x,
                joint.angles.y,
                joint.angles.z,
            );
            let matrix = parent
                * Mat4::from_rotation_translation(rotation, joint.pivot - rotation * joint.pivot);
            *flag = matrix != joint.matrix;
            self.joints[index].matrix = matrix;
        }

        for (index, binding) in self.bindings.iter().copied().enumerate() {
            let (start, end, upper) = match binding {
                Binding::Rigid(_) => continue,
                Binding::Muscle { start, end, .. } => (start, end, None),
                Binding::AbdominalWall => (PELVIS, WAIST, Some(CHEST)),
            };
            if !changed[start] && !changed[end] && !upper.is_some_and(|i| changed[i]) {
                continue;
            }
            let inverse = self.joints[start].matrix.inverse();
            let relative = [
                inverse * self.joints[end].matrix,
                upper.map_or(Mat4::IDENTITY, |i| inverse * self.joints[i].matrix),
            ];
            if relative.iter().all(|m| m.abs_diff_eq(Mat4::IDENTITY, 1e-6)) {
                self.deformed[index] = None;
                continue;
            }
            if self.deformed[index].as_ref().is_some_and(|model| {
                model
                    .relative
                    .into_iter()
                    .zip(relative)
                    .all(|(a, b)| a.abs_diff_eq(b, 1e-6))
            }) {
                continue;
            }
            // Motion shared by both attachments just moves the finished muscle.
            let rest = &atlas.structures()[index].model;
            let (model, fibres) = match binding {
                Binding::Muscle {
                    origin, insertion, ..
                } => deform(rest, fibres[index], relative[0], origin, insertion),
                Binding::AbdominalWall => {
                    deform_mesh(rest, fibres[index], |p| abdominal_point(p, relative))
                }
                Binding::Rigid(_) => unreachable!(),
            };
            self.deformed[index] = Some(Deformed {
                relative,
                model: Arc::new(model),
                fibres,
            });
        }
    }

    pub fn geometry<'a>(
        &'a self,
        index: usize,
        rest: (&'a Arc<Model>, ModelId, Option<Fibres>),
    ) -> (&'a Arc<Model>, ModelId, Mat4, Option<Fibres>) {
        let (model, fibres) = self.deformed[index]
            .as_ref()
            .map_or((rest.0, rest.2), |posed| (&posed.model, posed.fibres));
        let bone = match self.bindings[index] {
            Binding::Rigid(bone) | Binding::Muscle { start: bone, .. } => bone,
            Binding::AbdominalWall => PELVIS,
        };
        let matrix = self.joints[bone].matrix;
        let fibres = fibres.map(|mut fibres| {
            fibres.axis = matrix.transform_vector3(fibres.axis);
            fibres.centre = matrix.transform_point3(fibres.centre);
            fibres
        });
        (model, rest.1, matrix, fibres)
    }
}

fn limb_base(side: Side) -> usize {
    if side == Side::Right {
        JAW + 8
    } else {
        JAW + 1
    }
}

fn bind(structure: &Structure) -> Binding {
    let centre = structure.model.bounds.center();
    let side = match structure.side {
        Side::Middle if centre.x < 0.0 => Side::Right,
        Side::Middle => Side::Left,
        side => side,
    };
    let base = limb_base(side);
    let name = structure.name.to_ascii_lowercase();
    let has = |patterns: &[&str]| patterns.iter().any(|p| name.contains(p));
    if name == "linea alba"
        || has(&[
            "rectus abdominis",
            "abdominal oblique",
            "transversus abdominis",
            "pyramidalis",
        ])
    {
        return Binding::AbdominalWall;
    }
    let y = centre.y;
    let x = centre.x.abs();
    let limb = |joint| base + joint;

    let rigid = if has(&[
        "mandible",
        "lower canine",
        "lower lateral incisor",
        "lower medial incisor",
        "lower first",
        "lower second",
        "lower third",
    ]) {
        JAW
    } else if y > 1.552
        || has(&[
            "occipital bone",
            "temporal bone",
            "maxilla",
            "upper first",
            "upper second",
        ])
    {
        HEAD
    } else if y > 1.46 || has(&["hyoid", "vertebra c", "atlas (c1)", "axis (c2)"]) {
        NECK
    } else if has(&["clavicle", "scapula"]) {
        limb(GIRDLE)
    } else if has(&[
        "vertebra t",
        "rib",
        "sternum",
        "xiphoid",
        "costal cartilage",
    ]) {
        CHEST
    } else if has(&["vertebra l"]) {
        WAIST
    } else if has(&["hip bone", "sacrum", "coccyx"]) {
        PELVIS
    } else if has(&["humerus"]) {
        limb(SHOULDER)
    } else if has(&["radius", "ulna"]) && structure.tissue == Tissue::Bone {
        limb(ELBOW)
    } else if has(&["femur"]) && structure.tissue == Tissue::Bone {
        limb(HIP)
    } else if has(&["tibia", "fibula", "patella"]) && structure.tissue == Tissue::Bone {
        limb(KNEE)
    } else if has(&["metacarpal", "finger of hand", "carpal"])
        || (x > 0.208 && y < 0.855 && y > 0.66)
    {
        limb(WRIST)
    } else if x > 0.196 && y < 1.11 && y >= 0.855 {
        limb(ELBOW)
    } else if x > 0.171 && y >= 1.11 {
        limb(SHOULDER)
    } else if y < 0.080 {
        limb(ANKLE)
    } else if y < 0.433 {
        limb(KNEE)
    } else if y < 0.804 {
        limb(HIP)
    } else if y < 0.976 {
        PELVIS
    } else if y < 1.13 {
        WAIST
    } else {
        CHEST
    };

    if !matches!(
        structure.tissue,
        Tissue::Muscle | Tissue::Tendon | Tissue::Fascia
    ) {
        return Binding::Rigid(rigid);
    }

    // shortcut: the large crossing muscles have explicit paired attachments;
    // small structures follow their nearest body region until individually rigged.
    let (start, end, direction) = if has(&["biceps brachii", "long head of triceps brachii"]) {
        (limb(GIRDLE), limb(ELBOW), Vec3::NEG_Y)
    } else if has(&[
        "triceps brachii",
        "brachialis muscle",
        "brachioradialis",
        "anconeus",
        "pronator teres",
    ]) {
        (limb(SHOULDER), limb(ELBOW), Vec3::NEG_Y)
    } else if has(&[
        "deltoid",
        "coracobrachialis",
        "infraspinatus",
        "supraspinatus",
        "subscapularis",
        "teres minor",
        "teres major",
    ]) {
        (
            limb(GIRDLE),
            limb(SHOULDER),
            Vec3::new(centre.x.signum() * 0.8, -1.0, 0.0).normalize(),
        )
    } else if has(&["pectoralis major", "latissimus dorsi"]) {
        (
            CHEST,
            limb(SHOULDER),
            Vec3::new(centre.x.signum(), 0.5, 0.0).normalize(),
        )
    } else if has(&[
        "sternocleidomastoid",
        "splenius capitis",
        "longissimus capitis",
    ]) {
        (CHEST, HEAD, Vec3::Y)
    } else if has(&[
        "pectoral",
        "serratus anterior",
        "rhomboid",
        "trapezius",
        "levator scapulae",
    ]) && structure.side != Side::Middle
    {
        (
            CHEST,
            limb(GIRDLE),
            Vec3::new(centre.x.signum(), 0.25, 0.0).normalize(),
        )
    } else if has(&["iliocostalis lumborum", "quadratus lumborum"]) {
        (PELVIS, CHEST, Vec3::Y)
    } else if has(&[
        "biceps femoris",
        "semitendinosus",
        "semimembranosus",
        "sartorius",
        "gracilis",
    ]) {
        (PELVIS, limb(KNEE), Vec3::NEG_Y)
    } else if has(&["rectus femoris", "vastus"]) {
        (
            if has(&["rectus femoris"]) {
                PELVIS
            } else {
                limb(HIP)
            },
            limb(KNEE),
            Vec3::NEG_Y,
        )
    } else if has(&[
        "gluteus",
        "iliacus",
        "piriformis",
        "adductor longus",
        "adductor magnus",
        "pectineus",
        "obturator",
        "gemellus",
        "quadratus femoris",
    ]) {
        (PELVIS, limb(HIP), Vec3::NEG_Y)
    } else if has(&["gastrocnemius", "plantaris"]) {
        (limb(HIP), limb(ANKLE), Vec3::NEG_Y)
    } else if has(&[
        "soleus",
        "calcaneal tendon",
        "tibialis",
        "fibularis",
        "hallucis longus",
        "digitorum longus",
    ]) {
        (limb(KNEE), limb(ANKLE), Vec3::NEG_Y)
    } else if has(&[
        "flexor carpi",
        "extensor carpi",
        "palmaris longus",
        "extensor indicis",
        "extensor digitorum",
        "flexor digitorum",
        "pollicis longus",
    ]) && y > 0.8
    {
        (limb(ELBOW), limb(WRIST), Vec3::NEG_Y)
    } else {
        return Binding::Rigid(rigid);
    };
    if structure.model.vertices.is_empty() {
        return Binding::Rigid(rigid);
    }

    let points = || {
        structure
            .model
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
    };
    let (lo, hi) = points()
        .map(|p| p.dot(direction))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p), hi.max(p))
        });
    let band = (hi - lo) * 0.045;
    let average = |edge: f32| {
        let (sum, count) = points()
            .filter(|p| (p.dot(direction) - edge).abs() <= band)
            .fold((Vec3::ZERO, 0), |(sum, count), p| (sum + p, count + 1));
        sum / count.max(1) as f32
    };
    let origin = average(lo);
    let insertion = average(hi);
    if origin.distance_squared(insertion) < 0.000_001 {
        Binding::Rigid(rigid)
    } else {
        Binding::Muscle {
            start,
            end,
            origin,
            insertion,
        }
    }
}

fn deform(
    rest: &Model,
    fibres: Option<Fibres>,
    relative: Mat4,
    origin: Vec3,
    insertion: Vec3,
) -> (Model, Option<Fibres>) {
    let delta = insertion - origin;
    let length = delta.length();
    let axis = delta / length;
    let line = relative.transform_point3(insertion) - origin;
    let align = Quat::from_rotation_arc(axis, line.normalize_or(axis));
    // Preserve the volume of the belly without inflating a folded muscle indefinitely.
    let thickness = (length / line.length().max(length * 0.25))
        .sqrt()
        .clamp(0.66, 1.6);
    deform_mesh(rest, fibres, |p: Vec3| {
        let along = (p - origin).dot(axis) / length;
        let t = along.clamp(0.0, 1.0);
        let distal = ((t - 0.20) / 0.60).clamp(0.0, 1.0);
        let weight = distal * distal * (3.0 - 2.0 * distal);
        let rigid = p.lerp(relative.transform_point3(p), weight);
        let transverse = p - origin - delta * along;
        let belly = origin + line * along + align * transverse * thickness;
        let blend = (t.min(1.0 - t) * 4.0).clamp(0.0, 1.0);
        rigid.lerp(belly, blend * blend * (3.0 - 2.0 * blend))
    })
}

fn abdominal_point(p: Vec3, [waist, chest]: [Mat4; 2]) -> Vec3 {
    // Use rest height to blend the whole wall together, including its sheath.
    let smooth = |low: f32, high: f32| {
        let t = ((p.y - low) / (high - low)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    p.lerp(waist.transform_point3(p), smooth(0.88, 1.08))
        .lerp(chest.transform_point3(p), smooth(0.99, 1.23))
}

fn deform_mesh(
    rest: &Model,
    fibres: Option<Fibres>,
    deform_point: impl Fn(Vec3) -> Vec3,
) -> (Model, Option<Fibres>) {
    let moved: Vec<_> = rest
        .vertices
        .iter()
        .map(|v| deform_point(Vec3::from(v.position)))
        .collect();
    let mut model = Model::from_mesh(&rest.name, moved, rest.indices.clone());
    model.parts.clone_from(&rest.parts);
    model.materials.clone_from(&rest.materials);
    let fibres = fibres.map(|mut f| {
        let tip = deform_point(f.centre + f.axis * 0.01);
        f.centre = deform_point(f.centre);
        f.axis = (tip - f.centre).normalize_or(f.axis);
        f
    });
    (model, fibres)
}

pub(super) fn posed_bounds(bounds: Bounds, matrix: Mat4) -> Bounds {
    if matrix == Mat4::IDENTITY || bounds.is_empty() {
        return bounds;
    }
    let extent = bounds.size() * 0.5;
    let half = matrix.x_axis.truncate().abs() * extent.x
        + matrix.y_axis.truncate().abs() * extent.y
        + matrix.z_axis.truncate().abs() * extent.z;
    let center = matrix.transform_point3(bounds.center());
    Bounds {
        min: center - half,
        max: center + half,
    }
}
