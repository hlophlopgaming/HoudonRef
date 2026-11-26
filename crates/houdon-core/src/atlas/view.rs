//! What the Atlas tab shows: which layers, how solid, and the camera, light
//! and shading it is seen with.

use std::collections::HashSet;

use glam::Vec3;

use super::masses::{DEEP_MUSCLE_SRGB, Mass, mass_of};
use super::{Atlas, BodySystem, Fibres, Structure, Tissue};
use crate::scene::ray_box_distance;
use crate::{Bounds, ModelId, Scene};

/// A structure's place in its atlas.
pub type StructureIndex = usize;

/// A group of structures shown or hidden together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Bones, teeth and the cartilage of the skeleton.
    Bones,
    /// Muscles and their tendons.
    Muscles,
    /// The sheets around muscles and the bursae between them, which would
    /// hide the muscles' forms; off at first.
    Fasciae,
    /// Ligaments, joint capsules and the cartilage and fat of joints.
    Joints,
}

impl Layer {
    pub const ALL: [Self; 4] = [Self::Bones, Self::Muscles, Self::Fasciae, Self::Joints];

    /// The layer a structure is shown with.
    pub fn of(structure: &Structure) -> Self {
        match (structure.system, structure.tissue) {
            (BodySystem::Joints, _) => Self::Joints,
            (_, Tissue::Muscle | Tissue::Tendon) => Self::Muscles,
            (_, Tissue::Fascia | Tissue::Bursa) => Self::Fasciae,
            (_, Tissue::Ligament | Tissue::Capsule | Tissue::Fat) => Self::Joints,
            (BodySystem::Skeletal, _) => Self::Bones,
            (_, Tissue::Bone | Tissue::Tooth | Tissue::Cartilage) => Self::Joints,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// How a layer is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerState {
    pub visible: bool,
    /// How solid it is drawn: 1 solid, less to see what lies under it.
    pub opacity: f32,
}

impl LayerState {
    /// The least opacity a see-through layer gets, so it never vanishes.
    pub const MIN_OPACITY: f32 = 0.1;

    pub const SHOWN: Self = Self {
        visible: true,
        opacity: 1.0,
    };
    pub const HIDDEN: Self = Self {
        visible: false,
        opacity: 1.0,
    };
}

impl Tissue {
    /// The colour of the tissue as it looks, linear RGB.
    pub fn natural_color(self) -> Vec3 {
        match self {
            Self::Bone => Vec3::new(0.78, 0.72, 0.58),
            Self::Tooth => Vec3::new(0.86, 0.84, 0.76),
            // Pearly, a little cooler than bone: the ribcage reads as one
            // form, with the cartilage still told apart.
            Self::Cartilage => Vec3::new(0.80, 0.77, 0.68),
            Self::Muscle => Vec3::new(0.50, 0.10, 0.09),
            Self::Tendon => Vec3::new(0.78, 0.74, 0.66),
            Self::Fascia => Vec3::new(0.70, 0.66, 0.62),
            Self::Bursa => Vec3::new(0.66, 0.70, 0.74),
            Self::Ligament => Vec3::new(0.74, 0.70, 0.62),
            Self::Capsule => Vec3::new(0.68, 0.66, 0.62),
            Self::Fat => Vec3::new(0.80, 0.70, 0.40),
        }
    }
}

/// How the Atlas colours its structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AtlasColors {
    /// Each muscle in the colour of its sculptural mass; other tissues in
    /// their natural colours.
    #[default]
    Masses,
    /// Every tissue as it looks.
    Natural,
    /// Everything in the clay tone.
    Clay,
    /// The skull's bones each in a colour of its own, so the sutures read;
    /// everything else as it looks.
    Bones,
}

/// The skull's bones, teeth and nose cartilages, by the start of their
/// names, each with the colour (8-bit sRGB) the Bones mode gives it.
/// Neighbouring bones differ.
pub const SKULL: &[(&str, [u8; 3])] = &[
    ("Frontal bone", [230, 196, 120]),
    ("Parietal bone", [120, 170, 220]),
    ("Temporal bone", [140, 200, 140]),
    ("Occipital bone", [220, 140, 140]),
    ("Sphenoid bone", [190, 150, 220]),
    ("Ethmoid bone", [240, 220, 160]),
    ("Zygomatic bone", [230, 150, 90]),
    ("Maxilla", [110, 190, 190]),
    ("Mandible", [200, 120, 180]),
    ("Nasal bone", [240, 230, 110]),
    ("Lacrimal bone", [160, 120, 90]),
    ("Palatine bone", [150, 160, 230]),
    ("Vomer", [200, 200, 200]),
    ("Inferior nasal concha", [170, 210, 120]),
    ("Upper ", [245, 242, 230]),
    ("Lower ", [245, 242, 230]),
    ("Nasal septal cartilage", [160, 200, 210]),
    ("Lateral process of nasal septal cartilage", [160, 200, 210]),
    ("Major alar cartilage", [160, 200, 210]),
];

fn skull_color(structure: &Structure) -> Option<[u8; 3]> {
    if structure.system != BodySystem::Skeletal {
        return None;
    }
    SKULL
        .iter()
        .find(|(name, _)| structure.name.starts_with(name))
        .map(|&(_, color)| color)
}

/// One structure to draw.
#[derive(Debug, Clone, Copy)]
pub struct ShownStructure<'a> {
    pub index: StructureIndex,
    pub structure: &'a Structure,
    pub model_id: ModelId,
    pub opacity: f32,
    /// Selected, or in the mass the legend points at: drawn highlighted.
    pub selected: bool,
    /// Under the pointer: drawn with a faint highlight.
    pub hovered: bool,
    /// Fibre lines to draw on it, when fibres are on and it is a muscle.
    pub fibres: Option<Fibres>,
}

/// The Atlas tab: the body, its layers and the view of it.
pub struct AtlasView {
    atlas: Atlas,
    /// The GPU cache key of each structure's model, in the atlas's order.
    model_ids: Vec<ModelId>,
    layers: [LayerState; 4],
    bounds: Bounds,
    selected: Option<StructureIndex>,
    /// Hidden one by one; a hidden structure hides its parts too.
    hidden: HashSet<StructureIndex>,
    /// When set, only these are shown, whatever their layers.
    isolated: Option<HashSet<StructureIndex>>,
    pub colors: AtlasColors,
    /// Fibre lines on the muscles.
    pub show_fibres: bool,
    /// A mass to highlight, as the legend is pointed at.
    pub emphasis: Option<&'static Mass>,
    /// The structure under the pointer.
    pub hovered: Option<StructureIndex>,
    /// While something is selected, everything else fades to a ghost, so
    /// the selection reads whole even where other structures cover it.
    pub fade_others: bool,
    /// Each structure's fibres, in the atlas's order.
    fibres: Vec<Option<Fibres>>,
    /// The camera, lights, room and shading the body is seen with; it
    /// holds no objects of its own.
    pub scene: Scene,
}

impl AtlasView {
    /// How solid the rest of the body stays while something is selected.
    pub const GHOST_OPACITY: f32 = 0.12;

    pub fn new(atlas: Atlas) -> Self {
        let model_ids = atlas.structures().iter().map(|_| ModelId::next()).collect();
        let fibres = atlas.structures().iter().map(Fibres::of).collect();
        let bounds = atlas
            .structures()
            .iter()
            .fold(Bounds::EMPTY, |b, s| b.union(&s.model.bounds));
        let mut scene = Scene::new();
        // A plain backdrop: the room is sized for the Studio's objects.
        scene.room.visible = false;
        // A light page, as in a book of anatomy for artists: the studio
        // light without cast shadows, clean outlines between forms.
        scene.room.backdrop_color = Vec3::splat(0.72);
        scene.shading.mode = crate::ShadingMode::Solid;
        scene.shading.solid = crate::SolidLighting::Studio;
        scene.shading.lines.contours = true;
        scene.shading.lines.color = Vec3::new(0.10, 0.07, 0.06);
        // Outlines only: the meshes' fine folds and thin layers lying on
        // each other would scatter fold lines like dirt.
        scene.shading.lines.fold_sensitivity = 0.0;
        let mut view = Self {
            atlas,
            model_ids,
            layers: [
                LayerState::SHOWN,
                LayerState::SHOWN,
                LayerState::HIDDEN,
                LayerState::HIDDEN,
            ],
            bounds,
            selected: None,
            hidden: HashSet::new(),
            isolated: None,
            colors: AtlasColors::default(),
            show_fibres: true,
            emphasis: None,
            hovered: None,
            fade_others: true,
            fibres,
            scene,
        };
        view.frame(1.0);
        view
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn layer(&self, layer: Layer) -> LayerState {
        self.layers[layer.index()]
    }

    pub fn set_layer(&mut self, layer: Layer, state: LayerState) {
        self.layers[layer.index()] = LayerState {
            visible: state.visible,
            opacity: state.opacity.clamp(LayerState::MIN_OPACITY, 1.0),
        };
    }

    /// Everything shown, with how solid each is.
    pub fn shown(&self) -> impl Iterator<Item = ShownStructure<'_>> {
        self.atlas
            .structures()
            .iter()
            .zip(&self.model_ids)
            .enumerate()
            .filter_map(|(index, (structure, &model_id))| {
                let state = self.layers[Layer::of(structure).index()];
                let shown = match &self.isolated {
                    Some(isolated) => isolated.contains(&index),
                    None => state.visible && !self.is_hidden(index),
                };
                let opacity = self.resting_opacity(structure);
                let opacity = match self.selected {
                    // The selection reads solid even in a see-through layer.
                    Some(selected) if selected == index => 1.0,
                    Some(_) if self.fade_others => opacity.min(Self::GHOST_OPACITY),
                    _ => opacity,
                };
                shown.then_some(ShownStructure {
                    index,
                    structure,
                    model_id,
                    opacity,
                    selected: self.selected == Some(index)
                        || self.emphasis.is_some() && mass_of(structure) == self.emphasis,
                    hovered: self.hovered == Some(index),
                    // A ghost keeps only its shape: lines on it would be
                    // noise in front of the selection.
                    fibres: self.fibres[index]
                        .filter(|_| self.show_fibres && opacity > Self::GHOST_OPACITY),
                })
            })
    }

    /// How solid `structure` is when nothing fades it: its layer's
    /// opacity, or fully solid while isolated.
    fn resting_opacity(&self, structure: &Structure) -> f32 {
        if self.isolated.is_some() {
            1.0
        } else {
            self.layers[Layer::of(structure).index()].opacity
        }
    }

    /// Whether `index` or a structure it is part of was hidden.
    fn is_hidden(&self, index: StructureIndex) -> bool {
        let mut current = Some(index);
        // Parents are few and shallow; the bound guards against a cycle.
        for _ in 0..16 {
            let Some(i) = current else {
                return false;
            };
            if self.hidden.contains(&i) {
                return true;
            }
            current = self.atlas.structures()[i]
                .parent
                .as_deref()
                .and_then(|parent| self.index_of(parent));
        }
        false
    }

    pub fn index_of(&self, id: &str) -> Option<StructureIndex> {
        self.atlas.structures().iter().position(|s| s.id == id)
    }

    pub fn selected(&self) -> Option<StructureIndex> {
        self.selected
    }

    pub fn select(&mut self, index: Option<StructureIndex>) {
        self.selected = index.filter(|&i| i < self.atlas.structures().len());
    }

    /// Shows only `indices`, whatever their layers.
    pub fn isolate(&mut self, indices: impl IntoIterator<Item = StructureIndex>) {
        let isolated: HashSet<_> = indices.into_iter().collect();
        self.isolated = (!isolated.is_empty()).then_some(isolated);
    }

    pub fn is_isolating(&self) -> bool {
        self.isolated.is_some()
    }

    /// Whether any structure was hidden one by one.
    pub fn hides_any(&self) -> bool {
        !self.hidden.is_empty()
    }

    /// Hides `index` and the structures that are part of it.
    pub fn hide(&mut self, index: StructureIndex) {
        if let Some(isolated) = &mut self.isolated {
            isolated.remove(&index);
            if isolated.is_empty() {
                self.isolated = None;
            }
        }
        self.hidden.insert(index);
        if self.selected == Some(index) {
            self.selected = None;
        }
    }

    pub fn set_hidden(&mut self, index: StructureIndex, hidden: bool) {
        if hidden {
            self.hide(index);
        } else {
            self.hidden.remove(&index);
        }
    }

    pub fn is_hidden_alone(&self, index: StructureIndex) -> bool {
        self.hidden.contains(&index)
    }

    /// Ends isolation and shows every hidden structure again; the layers
    /// stay as they are.
    pub fn show_all(&mut self) {
        self.isolated = None;
        self.hidden.clear();
    }

    /// The nearest shown structure along a ray, if any. See-through layers
    /// are passed through when something solid lies on the ray.
    pub fn pick(&self, origin: Vec3, direction: Vec3) -> Option<StructureIndex> {
        self.pick_hit(origin, direction).map(|(index, _)| index)
    }

    /// As [`Self::pick`], with how far along the ray the structure was
    /// hit, so the point itself can be found.
    pub fn pick_hit(&self, origin: Vec3, direction: Vec3) -> Option<(StructureIndex, f32)> {
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        let mut solid: Option<(f32, StructureIndex)> = None;
        let mut faint: Option<(f32, StructureIndex)> = None;
        for shown in self.shown() {
            let model = &shown.structure.model;
            let Some(entry) = ray_box_distance(origin, direction, &model.bounds) else {
                continue;
            };
            // A structure faded for the selection is still picked as the
            // solid thing it is: a click on it means it.
            let nearest = if self.resting_opacity(shown.structure) >= 0.5 {
                &mut solid
            } else {
                &mut faint
            };
            if nearest.is_some_and(|(t, _)| entry > t) {
                continue;
            }
            if let Some(t) = model.ray_hit(origin, direction)
                && nearest.is_none_or(|(n, _)| t < n)
            {
                *nearest = Some((t, shown.index));
            }
        }
        solid.or(faint).map(|(t, index)| (index, t))
    }

    /// Structures whose name contains every word of `query`, in English or
    /// Latin (or `other`, a translated name, where given), best matches
    /// first: bones and muscles before fasciae and joints, then names that
    /// start with the query, then shorter names.
    pub fn search(
        &self,
        query: &str,
        other: impl Fn(&Structure) -> Option<String>,
    ) -> Vec<StructureIndex> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(bool, bool, usize, StructureIndex)> = self
            .atlas
            .structures()
            .iter()
            .enumerate()
            .filter_map(|(index, structure)| {
                let names = [
                    Some(structure.name.clone()),
                    structure.latin.clone(),
                    other(structure),
                ];
                names
                    .into_iter()
                    .flatten()
                    .map(|name| name.to_lowercase())
                    .find(|name| words.iter().all(|w| name.contains(w.as_str())))
                    .map(|name| {
                        let minor = matches!(Layer::of(structure), Layer::Fasciae | Layer::Joints);
                        (minor, !name.starts_with(&words[0]), name.len(), index)
                    })
            })
            .collect();
        found.sort();
        found.into_iter().map(|(_, _, _, index)| index).collect()
    }

    /// Points the camera at the selected structure, or at what is shown.
    pub fn frame_selected(&mut self, aspect: f32) {
        match self.selected {
            Some(index) => {
                let bounds = self.atlas.structures()[index].model.bounds;
                self.scene.camera.frame(&bounds, aspect);
            }
            None => self.frame(aspect),
        }
    }

    /// The colour `structure` is drawn in, linear RGB.
    pub fn color(&self, structure: &Structure) -> Vec3 {
        match self.colors {
            AtlasColors::Clay => self.scene.shading.clay_color,
            AtlasColors::Natural => structure.tissue.natural_color(),
            AtlasColors::Bones => match skull_color(structure) {
                Some(srgb) => Vec3::from(srgb.map(|c| (f32::from(c) / 255.0).powf(2.2))),
                None => structure.tissue.natural_color(),
            },
            AtlasColors::Masses => match (mass_of(structure), structure.tissue) {
                (Some(mass), _) => mass.color(),
                (None, Tissue::Muscle) => {
                    Vec3::from(DEEP_MUSCLE_SRGB.map(|c| (f32::from(c) / 255.0).powf(2.2)))
                }
                (None, tissue) => tissue.natural_color(),
            },
        }
    }

    /// The skull's bones, teeth and nose cartilages.
    pub fn skull(&self) -> Vec<StructureIndex> {
        self.atlas
            .structures()
            .iter()
            .enumerate()
            .filter(|(_, s)| skull_color(s).is_some())
            .map(|(i, _)| i)
            .collect()
    }

    /// Shows the skull alone, framed from `preset`'s viewpoint.
    pub fn show_skull(&mut self, preset: &crate::CameraPreset, aspect: f32) {
        self.isolate(self.skull());
        self.view_from(preset, aspect);
    }

    /// Moves the camera to `preset`'s viewpoint around what is shown.
    pub fn view_from(&mut self, preset: &crate::CameraPreset, aspect: f32) {
        let bounds = self.shown_bounds();
        preset.aim(&mut self.scene.camera, &bounds, aspect);
    }

    /// The structures of `mass`.
    pub fn structures_of(&self, mass: &Mass) -> Vec<StructureIndex> {
        self.atlas
            .structures()
            .iter()
            .enumerate()
            .filter(|(_, s)| mass_of(s) == Some(mass))
            .map(|(i, _)| i)
            .collect()
    }

    /// The bounds of what is shown, or of the whole body when nothing is.
    pub fn shown_bounds(&self) -> Bounds {
        let bounds = self
            .shown()
            .fold(Bounds::EMPTY, |b, s| b.union(&s.structure.model.bounds));
        if bounds.is_empty() {
            self.bounds
        } else {
            bounds
        }
    }

    /// Points the camera at what is shown.
    pub fn frame(&mut self, aspect: f32) {
        let bounds = self.shown_bounds();
        self.scene.camera.frame(&bounds, aspect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::tests::human;

    #[test]
    fn every_structure_has_one_layer_and_the_layers_split_sensibly() {
        let atlas = human();
        let count = |layer| {
            atlas
                .structures()
                .iter()
                .filter(|s| Layer::of(s) == layer)
                .count()
        };
        assert!(count(Layer::Bones) > 250);
        assert!(count(Layer::Muscles) > 400);
        assert!(count(Layer::Fasciae) > 100);
        assert!(count(Layer::Joints) > 300);
        let femur = atlas.structure("Femur.l").unwrap();
        assert_eq!(Layer::of(femur), Layer::Bones);
    }

    #[test]
    fn hidden_layers_are_not_shown_and_see_through_ones_carry_their_opacity() {
        let mut view = AtlasView::new(human());
        let shown = |view: &AtlasView, layer| {
            view.shown()
                .filter(|s| Layer::of(s.structure) == layer)
                .map(|s| s.opacity)
                .collect::<Vec<_>>()
        };
        assert!(!shown(&view, Layer::Muscles).is_empty(), "muscles at first");
        assert!(
            shown(&view, Layer::Fasciae).is_empty(),
            "fasciae hidden at first"
        );
        let see_through = |opacity| LayerState {
            visible: true,
            opacity,
        };
        view.set_layer(Layer::Muscles, see_through(0.3));
        assert!(shown(&view, Layer::Muscles).iter().all(|&o| o == 0.3));
        view.set_layer(Layer::Muscles, see_through(0.0));
        assert!(
            shown(&view, Layer::Muscles)
                .iter()
                .all(|&o| o == LayerState::MIN_OPACITY),
            "never quite invisible"
        );
        view.set_layer(Layer::Muscles, LayerState::HIDDEN);
        assert!(shown(&view, Layer::Muscles).is_empty());
        assert!(!shown(&view, Layer::Bones).is_empty());
    }

    fn ray_at(view: &AtlasView, id: &str) -> (Vec3, Vec3) {
        // From in front of the body (+Z) straight at the structure.
        let centre = view.atlas().structure(id).unwrap().model.bounds.center();
        (centre + Vec3::new(0.0, 0.0, 2.0), Vec3::NEG_Z)
    }

    #[test]
    fn picking_finds_the_muscle_in_front_and_the_bone_once_muscles_are_hidden() {
        let mut view = AtlasView::new(human());
        let femur = view.index_of("Femur.l").unwrap();
        let (origin, direction) = ray_at(&view, "Femur.l");
        let hit = view.pick(origin, direction).unwrap();
        let structure = &view.atlas().structures()[hit];
        assert_eq!(Layer::of(structure), Layer::Muscles, "{}", structure.id);
        view.set_layer(Layer::Muscles, LayerState::HIDDEN);
        assert_eq!(view.pick(origin, direction), Some(femur));
        assert_eq!(
            view.pick(origin, -direction),
            None,
            "nothing behind the eye"
        );
    }

    #[test]
    fn search_finds_by_english_and_latin_best_first() {
        let view = AtlasView::new(human());
        let names = |query| {
            view.search(query, |_| None)
                .into_iter()
                .map(|i| view.atlas().structures()[i].id.clone())
                .collect::<Vec<_>>()
        };
        let deltoid = names("deltoid");
        assert!(
            deltoid[0].contains("deltoid muscle"),
            "the muscle first: {deltoid:?}"
        );
        let femur = names("femur");
        assert!(femur[..2].contains(&"Femur.l".to_owned()), "{femur:?}");
        assert!(
            names("sternocleidomastoid")
                .iter()
                .any(|id| id.starts_with("Sternocleidomastoid"))
        );
        let latin = names("musculus deltoideus");
        let by_latin = names("deltoide");
        assert!(!by_latin.is_empty(), "Latin: {latin:?}");
        assert!(names("").is_empty());
        let words = names("major pectoralis");
        assert!(
            words.iter().all(|id| id.contains("pectoralis major")),
            "{words:?}"
        );
        assert!(!words.is_empty());
        let russian = view.search("бедренная кость", |s| {
            (s.name == "Femur").then(|| "Бедренная кость".to_owned())
        });
        assert_eq!(russian.len(), 2);
    }

    #[test]
    fn isolating_hiding_and_showing_all() {
        let mut view = AtlasView::new(human());
        let all = view.shown().count();
        let femurs = view.search("femur", |_| None);
        view.isolate(femurs.iter().copied());
        assert_eq!(view.shown().count(), femurs.len(), "only what is isolated");
        view.set_layer(Layer::Bones, LayerState::HIDDEN);
        assert_eq!(
            view.shown().count(),
            femurs.len(),
            "isolation wins over layers"
        );
        view.set_layer(Layer::Bones, LayerState::SHOWN);
        view.hide(femurs[0]);
        assert_eq!(view.shown().count(), femurs.len() - 1);
        view.show_all();
        assert_eq!(view.shown().count(), all);
        view.hide(femurs[0]);
        assert_eq!(view.shown().count(), all - 1);
        // A hidden structure hides its parts.
        let child = view
            .atlas()
            .structures()
            .iter()
            .position(|s| {
                s.parent.is_some()
                    && Layer::of(s) != Layer::Fasciae
                    && Layer::of(s) != Layer::Joints
            })
            .unwrap();
        let parent = view
            .index_of(view.atlas().structures()[child].parent.as_deref().unwrap())
            .unwrap();
        view.show_all();
        view.hide(parent);
        assert!(
            view.shown().all(|s| s.index != child),
            "the part goes with it"
        );
        view.select(Some(child));
        assert!(
            view.shown().all(|s| !s.selected),
            "selection of a hidden part shows nothing"
        );
    }

    #[test]
    fn selecting_fades_the_rest_and_keeps_the_selection_solid() {
        let mut view = AtlasView::new(human());
        let biceps = view.index_of("Long head of biceps brachii.l").unwrap();
        let femur = view.index_of("Femur.l").unwrap();
        let opacity = |view: &AtlasView, i| view.shown().find(|s| s.index == i).unwrap().opacity;
        assert_eq!(
            opacity(&view, femur),
            1.0,
            "nothing fades before a selection"
        );
        view.set_layer(
            Layer::Muscles,
            LayerState {
                visible: true,
                opacity: 0.4,
            },
        );
        view.select(Some(biceps));
        assert_eq!(opacity(&view, biceps), 1.0, "solid in a see-through layer");
        assert_eq!(opacity(&view, femur), AtlasView::GHOST_OPACITY);
        let other_muscle = view.index_of("Long head of biceps brachii.r").unwrap();
        let fibres = |view: &AtlasView, i| view.shown().find(|s| s.index == i).unwrap().fibres;
        assert!(fibres(&view, biceps).is_some());
        assert!(fibres(&view, other_muscle).is_none(), "no lines on a ghost");
        view.fade_others = false;
        assert_eq!(opacity(&view, femur), 1.0);
        view.fade_others = true;
        view.select(None);
        assert_eq!(opacity(&view, femur), 1.0);
        assert_eq!(opacity(&view, biceps), 0.4);
    }

    #[test]
    fn a_faded_structure_is_still_picked_first() {
        let mut view = AtlasView::new(human());
        let femur = view.index_of("Femur.l").unwrap();
        let (origin, direction) = ray_at(&view, "Femur.l");
        let muscle = view.pick(origin, direction).unwrap();
        // The bone behind is selected; the muscle in front fades but a
        // click on it still finds it, not the bone.
        view.select(Some(femur));
        let (hit, distance) = view.pick_hit(origin, direction).unwrap();
        assert_eq!(hit, muscle);
        assert!(distance > 0.0 && distance < 2.0);
    }

    #[test]
    fn only_the_structure_under_the_pointer_is_hovered() {
        let mut view = AtlasView::new(human());
        let biceps = view.index_of("Long head of biceps brachii.l").unwrap();
        view.hovered = Some(biceps);
        let hovered: Vec<_> = view
            .shown()
            .filter(|s| s.hovered)
            .map(|s| s.index)
            .collect();
        assert_eq!(hovered, [biceps]);
    }

    #[test]
    fn cartilage_looks_like_part_of_the_skeleton() {
        let bone = Tissue::Bone.natural_color();
        let cartilage = Tissue::Cartilage.natural_color();
        let brightness = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        assert!(
            (brightness(cartilage) - brightness(bone)).abs() < 0.1,
            "as light as bone: {cartilage} {bone}"
        );
        assert!(
            cartilage.z / cartilage.x > bone.z / bone.x,
            "a little cooler"
        );
    }

    #[test]
    fn the_view_opens_like_a_page_of_an_anatomy_book() {
        let mut view = AtlasView::new(human());
        assert_eq!(view.scene.shading.mode, crate::ShadingMode::Solid);
        assert!(view.scene.shading.lines.contours);
        assert!(view.scene.room.backdrop_color.x > 0.5, "a light page");
        let biceps = view.index_of("Long head of biceps brachii.l").unwrap();
        let femur = view.index_of("Femur.l").unwrap();
        let fibres = |view: &AtlasView, i| view.shown().find(|s| s.index == i).unwrap().fibres;
        assert!(fibres(&view, biceps).is_some());
        assert!(fibres(&view, femur).is_none());
        view.show_fibres = false;
        assert!(fibres(&view, biceps).is_none());
        // The legend points at a mass: its muscles light up, no others.
        let mass = mass_of(&view.atlas().structures()[biceps]).unwrap();
        view.emphasis = Some(mass);
        assert!(view.shown().find(|s| s.index == biceps).unwrap().selected);
        assert!(!view.shown().find(|s| s.index == femur).unwrap().selected);
    }

    #[test]
    fn colours_follow_the_mode() {
        let mut view = AtlasView::new(human());
        let deltoid = view
            .atlas()
            .structure("Clavicular part of deltoid muscle.l")
            .unwrap()
            .clone();
        let pectoral = view
            .atlas()
            .structure("Clavicular head of pectoralis major muscle.l")
            .unwrap()
            .clone();
        let femur = view.atlas().structure("Femur.l").unwrap().clone();
        assert_ne!(view.color(&deltoid), view.color(&pectoral), "masses differ");
        assert_eq!(view.color(&femur), Tissue::Bone.natural_color());
        view.colors = AtlasColors::Natural;
        assert_eq!(view.color(&deltoid), view.color(&pectoral));
        view.colors = AtlasColors::Clay;
        assert_eq!(view.color(&femur), view.color(&deltoid));
        let mass = mass_of(&deltoid).unwrap();
        let parts = view.structures_of(mass);
        assert!(
            parts.len() >= 6,
            "three parts on each side: {}",
            parts.len()
        );
    }

    #[test]
    fn the_skull_is_its_bones_and_shows_alone_in_every_classic_view() {
        let mut view = AtlasView::new(human());
        let names: Vec<String> = view
            .skull()
            .iter()
            .map(|&i| view.atlas().structures()[i].id.clone())
            .collect();
        for bone in [
            "Frontal bone",
            "Parietal bone.l",
            "Temporal bone.r",
            "Occipital bone",
            "Sphenoid bone",
            "Zygomatic bone.l",
            "Maxilla.r",
            "Mandible",
            "Nasal bone.l",
            "Vomer",
        ] {
            assert!(names.iter().any(|n| n == bone), "{bone} in {names:?}");
        }
        assert!(names.iter().any(|n| n.contains("molar")), "teeth too");
        assert!(!names.iter().any(|n| n.starts_with("Femur")));
        let aspect = 1.5;
        for preset in crate::CameraPreset::built_ins() {
            view.show_skull(&preset, aspect);
            assert_eq!(view.shown().count(), names.len(), "only the skull");
            let bounds = view.shown_bounds();
            let view_proj = view.scene.camera.view_projection(aspect);
            let mut lo = glam::Vec2::splat(f32::MAX);
            let mut hi = glam::Vec2::splat(f32::MIN);
            for corner in 0..8 {
                let pick = |bit, a: f32, b: f32| if corner & bit == 0 { a } else { b };
                let point = Vec3::new(
                    pick(1, bounds.min.x, bounds.max.x),
                    pick(2, bounds.min.y, bounds.max.y),
                    pick(4, bounds.min.z, bounds.max.z),
                );
                let ndc = view_proj.project_point3(point).truncate();
                lo = lo.min(ndc);
                hi = hi.max(ndc);
            }
            let name = preset.name().to_owned();
            assert!(
                lo.cmpge(glam::Vec2::splat(-1.3)).all() && hi.cmple(glam::Vec2::splat(1.3)).all(),
                "{name}: in view {lo} {hi}"
            );
            assert!(
                (hi - lo).max_element() > 0.8,
                "{name}: fills the view {lo} {hi}"
            );
        }
    }

    #[test]
    fn the_bones_mode_tells_neighbouring_skull_bones_apart() {
        let mut view = AtlasView::new(human());
        view.colors = AtlasColors::Bones;
        let color = |id: &str| view.color(view.atlas().structure(id).unwrap());
        assert_ne!(color("Frontal bone"), color("Parietal bone.l"));
        assert_ne!(color("Parietal bone.l"), color("Temporal bone.l"));
        assert_eq!(color("Parietal bone.l"), color("Parietal bone.r"));
        assert_eq!(color("Femur.l"), Tissue::Bone.natural_color());
    }

    #[test]
    fn the_camera_frames_the_whole_body_and_each_structure_keeps_its_gpu_key() {
        let view = AtlasView::new(human());
        let bounds = view.shown_bounds();
        assert!(
            (1.5..2.0).contains(&bounds.size().y),
            "a person: {}",
            bounds.size().y
        );
        let target = view.scene.camera.target;
        assert!(
            (target - bounds.center()).length() < 0.05,
            "aimed at the body: {target}"
        );
        let first: Vec<_> = view.shown().map(|s| s.model_id).collect();
        let again: Vec<_> = view.shown().map(|s| s.model_id).collect();
        assert_eq!(first, again);
    }
}
