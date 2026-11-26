//! The direction a muscle's fibres run, for drawing them as fine lines
//! along it: the muscle's longest axis through its middle.

use glam::{Mat3, Vec3};

use super::{Structure, Tissue};

/// Fibre lines for one muscle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fibres {
    /// The direction the fibres run, unit length.
    pub axis: Vec3,
    /// A point on the muscle's middle line.
    pub centre: Vec3,
    /// How many lines go round the muscle.
    pub lines: f32,
}

/// Distance between neighbouring fibre lines on the surface, metres.
const SPACING: f32 = 0.007;

impl Fibres {
    /// The fibres of a muscle, `None` for other tissues.
    pub fn of(structure: &Structure) -> Option<Self> {
        if structure.tissue != Tissue::Muscle {
            return None;
        }
        let points: Vec<Vec3> = structure
            .model
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
            .collect();
        if points.len() < 4 {
            return None;
        }
        let n = points.len() as f32;
        let centre = points.iter().copied().sum::<Vec3>() / n;
        let mut covariance = Mat3::ZERO;
        for &p in &points {
            let d = p - centre;
            covariance += Mat3::from_cols(d * d.x, d * d.y, d * d.z);
        }
        // The dominant direction by power iteration.
        let mut axis = Vec3::new(0.3, 1.0, 0.2).normalize();
        for _ in 0..32 {
            axis = (covariance * axis).normalize_or(Vec3::Y);
        }
        let radius = points
            .iter()
            .map(|&p| {
                let d = p - centre;
                (d - axis * d.dot(axis)).length()
            })
            .sum::<f32>()
            / n;
        let lines = (std::f32::consts::TAU * radius / SPACING)
            .round()
            .clamp(6.0, 96.0);
        Some(Self {
            axis,
            centre,
            lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::tests::human;

    #[test]
    fn long_muscles_run_along_the_limb() {
        let atlas = human();
        let fibres = |id: &str| Fibres::of(atlas.structure(id).unwrap()).unwrap();
        // The sartorius and the biceps run up and down the thigh and arm.
        for id in ["Rectus femoris muscle.l", "Long head of biceps brachii.l"] {
            let f = fibres(id);
            assert!(f.axis.y.abs() > 0.8, "{id}: {}", f.axis);
            assert!((6.0..=96.0).contains(&f.lines));
        }
        // The broad pectoral has more lines round it than a finger muscle.
        let pectoral = fibres("Sternocostal head of pectoralis major muscle.l");
        let finger = atlas
            .structures()
            .iter()
            .find(|s| s.name.starts_with("Lumbrical muscles of hand"))
            .and_then(Fibres::of)
            .unwrap();
        assert!(pectoral.lines > finger.lines);
        assert!(
            Fibres::of(atlas.structure("Femur.l").unwrap()).is_none(),
            "bones have none"
        );
    }
}
