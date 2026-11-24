//! The models built into the Studio: test figures and reference heads.

const MALE_GLB: &[u8] = include_bytes!("../../../../assets/models/test/male.glb");
const FEMALE_GLB: &[u8] = include_bytes!("../../../../assets/models/test/female.glb");
const ASARO_HEAD_GLB: &[u8] = include_bytes!("../../../../assets/models/heads/asaro_head.glb");
const PLANAR_HEAD_GLB: &[u8] = include_bytes!("../../../../assets/models/heads/planar_head.glb");
const BASE_HEAD_GLB: &[u8] = include_bytes!("../../../../assets/models/heads/base_head.glb");
const WOLFSARO_HEAD_GLB: &[u8] =
    include_bytes!("../../../../assets/models/heads/wolfsaro_head.glb");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Builtin {
    Male,
    Female,
    AsaroHead,
    PlanarHead,
    BaseHead,
    WolfsaroHead,
}

impl Builtin {
    pub(super) const ALL: [Self; 6] = [
        Self::Male,
        Self::Female,
        Self::AsaroHead,
        Self::PlanarHead,
        Self::BaseHead,
        Self::WolfsaroHead,
    ];

    pub(super) fn name_and_bytes(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Male => ("male", MALE_GLB),
            Self::Female => ("female", FEMALE_GLB),
            Self::AsaroHead => ("asaro-head", ASARO_HEAD_GLB),
            Self::PlanarHead => ("planar-head", PLANAR_HEAD_GLB),
            Self::BaseHead => ("base-head", BASE_HEAD_GLB),
            Self::WolfsaroHead => ("wolfsaro-head", WOLFSARO_HEAD_GLB),
        }
    }

    /// The real size of the largest side, metres, for models whose file
    /// units are not metres. Figures are sized by the figure height instead.
    pub(super) fn real_size(self) -> Option<f32> {
        match self {
            Self::Male | Self::Female => None,
            // Life-size heads on their stands.
            Self::AsaroHead => Some(0.32),
            Self::PlanarHead => Some(0.32),
            // Head and neck, no stand.
            Self::BaseHead => Some(0.3),
            // A bust: head, ears, shoulders and pedestal, the head itself
            // life-size next to a human one.
            Self::WolfsaroHead => Some(0.6),
        }
    }

    /// Turn around the vertical axis so the model faces the front (+Z).
    pub(super) fn facing(self) -> f32 {
        match self {
            // The file looks along -X.
            Self::AsaroHead => std::f32::consts::FRAC_PI_2,
            _ => 0.0,
        }
    }

    /// Message id of the button that adds the model.
    pub(super) fn add_message_id(self) -> &'static str {
        match self {
            Self::Male => "studio-add-male",
            Self::Female => "studio-add-female",
            Self::AsaroHead => "studio-add-asaro-head",
            Self::PlanarHead => "studio-add-planar-head",
            Self::BaseHead => "studio-add-base-head",
            Self::WolfsaroHead => "studio-add-wolfsaro-head",
        }
    }

    /// Translated name shown in the objects list.
    pub(super) fn message_id(self) -> &'static str {
        match self {
            Self::Male => "model-male-figure",
            Self::Female => "model-female-figure",
            Self::AsaroHead => "model-asaro-head",
            Self::PlanarHead => "model-planar-head",
            Self::BaseHead => "model-base-head",
            Self::WolfsaroHead => "model-wolfsaro-head",
        }
    }
}
