//! Sculptural masses: the muscles an artist draws as one form, each with a
//! colour of its own. The grouping and the palette are this project's own,
//! chosen for drawing.

use glam::Vec3;

use super::{Structure, Tissue};

/// A mass: its key (for its name in the locale files: `mass-<key>`), its
/// colour (8-bit sRGB) and the name fragments that place a muscle in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mass {
    pub key: &'static str,
    pub srgb: [u8; 3],
    patterns: &'static [&'static str],
}

impl Mass {
    /// The mass's colour, linear RGB.
    pub fn color(&self) -> Vec3 {
        Vec3::from(self.srgb.map(|c| srgb_to_linear(f32::from(c) / 255.0)))
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Every mass, in the legend's order: torso, back, arm, hand, hip and leg,
/// foot, head and neck. A muscle goes to the first mass with a fragment of
/// its name, so specific fragments come before general ones.
pub const MASSES: &[Mass] = &[
    Mass {
        key: "pectoral",
        srgb: [79, 166, 106],
        patterns: &["pectoralis major"],
    },
    Mass {
        key: "deltoid",
        srgb: [217, 102, 91],
        patterns: &["deltoid"],
    },
    Mass {
        key: "serratus",
        srgb: [227, 180, 60],
        patterns: &["serratus anterior"],
    },
    Mass {
        key: "external-oblique",
        srgb: [70, 179, 176],
        patterns: &["external abdominal oblique"],
    },
    Mass {
        key: "rectus-abdominis",
        srgb: [165, 140, 190],
        patterns: &["rectus abdominis", "pyramidalis"],
    },
    Mass {
        key: "trapezius",
        srgb: [78, 134, 200],
        patterns: &["trapezius"],
    },
    Mass {
        key: "latissimus",
        srgb: [178, 85, 143],
        patterns: &["latissimus dorsi", "teres major"],
    },
    Mass {
        key: "scapular",
        srgb: [201, 138, 60],
        patterns: &[
            "infraspinatus",
            "supraspinatus",
            "teres minor",
            "subscapularis",
            "rhomboid",
            "levator scapulae",
        ],
    },
    Mass {
        key: "erector",
        srgb: [107, 142, 61],
        patterns: &[
            "iliocostalis",
            "longissimus thoracis",
            "longissimus colli",
            "spinalis",
            "multifidus",
            "rotatores",
            "interspinales",
            "intertransversarii",
            "levatores",
        ],
    },
    Mass {
        key: "arm-flexors",
        srgb: [122, 92, 199],
        patterns: &["biceps brachii", "brachialis", "coracobrachialis"],
    },
    Mass {
        key: "triceps",
        srgb: [79, 176, 217],
        patterns: &["triceps", "anconeus"],
    },
    // The foot's short muscles share fragments with the forearm's.
    Mass {
        key: "foot",
        srgb: [191, 169, 138],
        patterns: &[
            "of foot",
            "extensor digitorum brevis",
            "extensor hallucis brevis",
            "flexor digitorum brevis",
            "flexor hallucis brevis",
            "abductor hallucis",
            "adductor hallucis",
            "quadratus plantae",
            "plantar interossei",
        ],
    },
    Mass {
        key: "shin",
        srgb: [73, 176, 160],
        patterns: &[
            "tibialis anterior",
            "extensor digitorum longus",
            "extensor hallucis longus",
            "fibularis tertius",
        ],
    },
    Mass {
        key: "forearm-flexors",
        srgb: [111, 179, 107],
        patterns: &[
            "flexor carpi",
            "palmaris longus",
            "flexor digitorum superficialis",
            "flexor digitorum profundus",
            "flexor pollicis longus",
            "pronator",
        ],
    },
    Mass {
        key: "forearm-extensors",
        srgb: [217, 139, 74],
        patterns: &[
            "brachioradialis",
            "extensor carpi",
            "extensor digitorum",
            "extensor digiti minimi",
            "extensor indicis",
            "extensor pollicis",
            "abductor pollicis longus",
            "supinator",
        ],
    },
    Mass {
        key: "hand",
        srgb: [199, 178, 153],
        patterns: &[
            "of hand",
            "pollicis brevis",
            "opponens pollicis",
            "adductor pollicis",
            "palmar interossei",
        ],
    },
    Mass {
        key: "gluteals",
        srgb: [192, 80, 77],
        patterns: &["gluteus maximus", "gluteus medius", "gluteus minimus"],
    },
    Mass {
        key: "tensor-fasciae-latae",
        srgb: [224, 162, 58],
        patterns: &["tensor fasciae latae", "iliotibial tract"],
    },
    Mass {
        key: "quadriceps",
        srgb: [91, 155, 213],
        patterns: &["rectus femoris", "vastus"],
    },
    Mass {
        key: "sartorius",
        srgb: [236, 226, 140],
        patterns: &["sartorius"],
    },
    Mass {
        key: "adductors",
        srgb: [155, 127, 209],
        patterns: &[
            "adductor longus",
            "adductor brevis",
            "adductor magnus",
            "adductor minimus",
            "gracilis",
            "pectineus",
        ],
    },
    Mass {
        key: "hamstrings",
        srgb: [79, 154, 90],
        patterns: &["biceps femoris", "semitendinosus", "semimembranosus"],
    },
    Mass {
        key: "calf",
        srgb: [208, 116, 110],
        patterns: &["gastrocnemius", "soleus", "plantaris", "calcaneal tendon"],
    },
    Mass {
        key: "fibular",
        srgb: [168, 194, 86],
        patterns: &["fibularis longus", "fibularis brevis"],
    },
    Mass {
        key: "sternocleidomastoid",
        srgb: [224, 143, 58],
        patterns: &["sternocleidomastoid"],
    },
    Mass {
        key: "neck",
        srgb: [143, 176, 138],
        patterns: &[
            "scalenus",
            "splenius",
            "sternohyoid",
            "sternothyroid",
            "omohyoid",
            "thyrohyoid",
            "stylohyoid",
            "geniohyoid",
            "mylohyoid",
            "digastric",
        ],
    },
    Mass {
        key: "masticatory",
        srgb: [168, 95, 176],
        patterns: &["masseter", "temporalis muscle", "pterygoid"],
    },
    Mass {
        key: "expression",
        srgb: [230, 161, 160],
        patterns: &[
            "frontalis",
            "occipitalis",
            "temporoparietalis",
            "epicranial aponeurosis",
            "orbicularis",
            "corrugator",
            "procerus",
            "nasalis",
            "levator labii",
            "levator anguli",
            "levator nasolabialis",
            "zygomaticus",
            "risorius",
            "depressor anguli",
            "depressor labii",
            "depressor septi",
            "mentalis",
            "bucinator",
            "platysma",
        ],
    },
];

/// Deep and minor muscles, drawn in one quiet colour: the eye's, the
/// throat's, the pelvic floor's, the deep rotators and the like.
pub const DEEP_MUSCLE_SRGB: [u8; 3] = [138, 108, 102];

/// The mass a muscle (or a tendon) belongs to; `None` for the deep and
/// minor ones and for every other tissue.
pub fn mass_of(structure: &Structure) -> Option<&'static Mass> {
    if !matches!(structure.tissue, Tissue::Muscle | Tissue::Tendon) {
        return None;
    }
    let name = structure.name.to_lowercase();
    MASSES
        .iter()
        .find(|mass| mass.patterns.iter().any(|p| name.contains(p)))
}

/// Neighbouring masses on the body, whose colours must tell them apart.
pub const NEIGHBOURS: &[(&str, &str)] = &[
    ("pectoral", "deltoid"),
    ("pectoral", "serratus"),
    ("pectoral", "rectus-abdominis"),
    ("pectoral", "external-oblique"),
    ("pectoral", "arm-flexors"),
    ("deltoid", "arm-flexors"),
    ("deltoid", "triceps"),
    ("deltoid", "trapezius"),
    ("deltoid", "scapular"),
    ("serratus", "external-oblique"),
    ("serratus", "latissimus"),
    ("external-oblique", "rectus-abdominis"),
    ("external-oblique", "latissimus"),
    ("trapezius", "latissimus"),
    ("trapezius", "scapular"),
    ("trapezius", "sternocleidomastoid"),
    ("trapezius", "neck"),
    ("latissimus", "erector"),
    ("latissimus", "scapular"),
    ("arm-flexors", "triceps"),
    ("arm-flexors", "forearm-flexors"),
    ("arm-flexors", "forearm-extensors"),
    ("triceps", "forearm-extensors"),
    ("forearm-flexors", "forearm-extensors"),
    ("forearm-flexors", "hand"),
    ("forearm-extensors", "hand"),
    ("gluteals", "tensor-fasciae-latae"),
    ("gluteals", "hamstrings"),
    ("gluteals", "erector"),
    ("gluteals", "external-oblique"),
    ("tensor-fasciae-latae", "quadriceps"),
    ("tensor-fasciae-latae", "sartorius"),
    ("quadriceps", "sartorius"),
    ("quadriceps", "adductors"),
    ("quadriceps", "shin"),
    ("sartorius", "adductors"),
    ("adductors", "hamstrings"),
    ("hamstrings", "calf"),
    ("calf", "fibular"),
    ("calf", "shin"),
    ("shin", "fibular"),
    ("shin", "foot"),
    ("fibular", "foot"),
    ("sternocleidomastoid", "neck"),
    ("sternocleidomastoid", "masticatory"),
    ("masticatory", "expression"),
    ("neck", "expression"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::tests::human;

    /// CIE Lab of an 8-bit sRGB colour (D65).
    fn lab(srgb: [u8; 3]) -> Vec3 {
        let [r, g, b] = srgb.map(|c| srgb_to_linear(f32::from(c) / 255.0));
        let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
        let f = |t: f32| {
            if t > 0.008856 {
                t.cbrt()
            } else {
                7.787 * t + 16.0 / 116.0
            }
        };
        Vec3::new(
            116.0 * f(y) - 16.0,
            500.0 * (f(x) - f(y)),
            200.0 * (f(y) - f(z)),
        )
    }

    fn mass(key: &str) -> &'static Mass {
        MASSES.iter().find(|m| m.key == key).unwrap()
    }

    #[test]
    fn neighbouring_masses_have_clearly_different_colours() {
        for (a, b) in NEIGHBOURS {
            let distance = lab(mass(a).srgb).distance(lab(mass(b).srgb));
            assert!(distance > 25.0, "{a} and {b} are {distance:.1} apart");
        }
        let mut keys: Vec<_> = MASSES.iter().map(|m| m.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), MASSES.len(), "keys are unique");
    }

    #[test]
    fn every_mass_has_muscles_and_the_big_ones_land_where_they_belong() {
        let atlas = human();
        for mass in MASSES {
            assert!(
                atlas.structures().iter().any(|s| mass_of(s) == Some(mass)),
                "{} has no muscle",
                mass.key
            );
        }
        let of = |id: &str| mass_of(atlas.structure(id).unwrap()).map(|m| m.key);
        assert_eq!(of("Clavicular part of deltoid muscle.l"), Some("deltoid"));
        assert_eq!(of("Long head of biceps femoris.l"), Some("hamstrings"));
        assert_eq!(of("Long head of biceps brachii.l"), Some("arm-flexors"));
        assert_eq!(of("Extensor digitorum longus.l"), Some("shin"));
        assert_eq!(of("Extensor digitorum.l"), Some("forearm-extensors"));
        assert_eq!(of("Extensor digitorum brevis.l"), Some("foot"));
        assert_eq!(of("Abductor pollicis brevis.l"), Some("hand"));
        assert_eq!(of("Temporalis muscle.l"), Some("masticatory"));
        assert_eq!(of("Zygomaticus major muscle.l"), Some("expression"));
        assert_eq!(of("Internal abdominal oblique muscle.l"), None, "deep");
        assert_eq!(of("Femur.l"), None, "not a muscle");
        let muscles = atlas
            .structures()
            .iter()
            .filter(|s| s.tissue == Tissue::Muscle)
            .count();
        let placed = atlas
            .structures()
            .iter()
            .filter(|s| s.tissue == Tissue::Muscle && mass_of(s).is_some())
            .count();
        assert!(
            placed * 2 > muscles,
            "{placed} of {muscles} muscles in a mass"
        );
    }
}
