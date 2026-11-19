//! The built-in light masks (Kenney's Light Masks, CC0) and the masks the
//! artist has loaded. Files are used exactly as shipped.
//!
//! Desktop builds carry every mask. The web build carries the few in
//! [`BUILT_IN`] and fetches the others from the site when they are chosen,
//! so they are not part of the wasm download.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use houdon_core::MaskImage;

/// Includes masks from `assets/masks/kenney` by name.
macro_rules! kenney {
    ($($name:literal),* $(,)?) => {
        &[$((
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/masks/kenney/",
                $name,
                ".png"
            )) as &[u8],
        )),*]
    };
}

/// Every mask, by name.
pub const ALL: &[&str] = &[
    "circle_a",
    "circle_a_noise",
    "circle_a_streaks",
    "circle_a_streaks_noise",
    "circle_b",
    "circle_b_noise",
    "circle_b_streaks",
    "circle_b_streaks_noise",
    "circle_c",
    "circle_c_noise",
    "circle_c_streaks",
    "circle_c_streaks_noise",
    "circle_d",
    "circle_d_noise",
    "circle_d_streaks",
    "circle_d_streaks_noise",
    "circle_rings_a",
    "circle_rings_a_noise",
    "circle_rings_a_streaks",
    "circle_rings_b",
    "circle_rings_b_noise",
    "circle_rings_b_streaks",
    "circle_rings_c",
    "circle_rings_c_noise",
    "circle_rings_c_streaks",
    "circle_rings_d",
    "circle_rings_d_noise",
    "circle_rings_d_streaks",
    "cone_a",
    "cone_a_blur",
    "cone_a_blur_noise",
    "cone_a_noise",
    "cone_b",
    "cone_b_blur",
    "cone_b_blur_noise",
    "cone_b_noise",
    "cone_c",
    "cone_c_blur",
    "cone_c_blur_noise",
    "cone_c_noise",
    "cone_composed_a",
    "cone_composed_a_noise",
    "cone_composed_b",
    "cone_composed_b_noise",
    "cone_composed_c",
    "cone_composed_c_noise",
    "cone_composed_d",
    "cone_composed_d_noise",
    "cone_composed_e",
    "cone_composed_e_noise",
    "cone_composed_f",
    "cone_composed_f_noise",
    "cone_d",
    "cone_d_blur",
    "cone_d_blur_noise",
    "cone_d_noise",
    "cone_e",
    "cone_e_blur",
    "cone_e_blur_noise",
    "cone_e_noise",
    "fan_a",
    "fan_a_blur",
    "fan_a_gradient",
    "fan_b",
    "fan_b_blur",
    "fan_b_gradient",
    "fan_c",
    "fan_c_blur",
    "fan_c_gradient",
    "fan_d",
    "fan_d_blur",
    "fan_d_gradient",
    "foliage_canopy_a",
    "foliage_canopy_a_blur",
    "foliage_canopy_a_noise",
    "foliage_canopy_b",
    "foliage_canopy_b_blur",
    "foliage_canopy_b_noise",
    "foliage_canopy_c",
    "foliage_canopy_c_blur",
    "foliage_canopy_c_noise",
    "foliage_canopy_d",
    "foliage_canopy_d_noise",
    "ring_a",
    "ring_a_noise",
    "ring_a_streaks",
    "ring_b",
    "ring_b_noise",
    "ring_b_streaks",
    "ring_c",
    "ring_c_noise",
    "ring_c_streaks",
    "shape_a",
    "shape_b",
    "shape_c",
    "shape_d",
    "shape_e",
    "shape_f",
    "shape_g",
    "streaks_composed_a",
    "streaks_composed_a_noise",
    "streaks_composed_b",
    "streaks_composed_b_noise",
    "streaks_composed_c",
    "streaks_composed_c_noise",
    "streaks_composed_d",
    "streaks_composed_d_noise",
    "streaks_composed_e",
    "streaks_composed_e_noise",
    "streaks_composed_f",
    "streaks_composed_f_noise",
    "streaks_composed_g",
    "streaks_composed_g_noise",
    "streaks_composed_h",
    "streaks_composed_h_noise",
    "water_caustics_a",
    "water_caustics_b",
    "water_caustics_c",
    "water_caustics_d",
    "window_a",
    "window_a_blur",
    "window_a_noise",
    "window_b",
    "window_b_blur",
    "window_b_noise",
    "window_c",
    "window_c_blur",
    "window_c_noise",
    "window_d",
    "window_d_blur",
    "window_d_noise",
    "window_e",
    "window_e_blur",
    "window_e_noise",
    "window_f",
    "window_f_blur",
    "window_f_noise",
    "window_g",
    "window_g_blur",
    "window_g_noise",
    "window_h",
    "window_h_blur",
    "window_h_noise",
    "window_i",
    "window_i_blur",
    "window_i_noise",
    "window_j",
    "window_j_blur",
    "window_j_noise",
    "window_k",
    "window_k_blur",
    "window_k_noise",
];

/// The masks every build carries, available offline.
pub const BUILT_IN: &[(&str, &[u8])] = kenney![
    "window_c",
    "window_e",
    "window_e_blur",
    "window_i",
    "window_k_blur",
    "foliage_canopy_a",
    "foliage_canopy_c_blur",
    "foliage_canopy_d",
    "water_caustics_a",
    "cone_composed_a",
    "streaks_composed_a",
    "circle_rings_a",
];

/// The web build carries no others: it fetches them.
#[cfg(target_arch = "wasm32")]
const EMBEDDED: &[(&str, &[u8])] = &[];

/// Desktop builds carry all the others too.
#[cfg(not(target_arch = "wasm32"))]
const EMBEDDED: &[(&str, &[u8])] = kenney![
    "circle_a",
    "circle_a_noise",
    "circle_a_streaks",
    "circle_a_streaks_noise",
    "circle_b",
    "circle_b_noise",
    "circle_b_streaks",
    "circle_b_streaks_noise",
    "circle_c",
    "circle_c_noise",
    "circle_c_streaks",
    "circle_c_streaks_noise",
    "circle_d",
    "circle_d_noise",
    "circle_d_streaks",
    "circle_d_streaks_noise",
    "circle_rings_a",
    "circle_rings_a_noise",
    "circle_rings_a_streaks",
    "circle_rings_b",
    "circle_rings_b_noise",
    "circle_rings_b_streaks",
    "circle_rings_c",
    "circle_rings_c_noise",
    "circle_rings_c_streaks",
    "circle_rings_d",
    "circle_rings_d_noise",
    "circle_rings_d_streaks",
    "cone_a",
    "cone_a_blur",
    "cone_a_blur_noise",
    "cone_a_noise",
    "cone_b",
    "cone_b_blur",
    "cone_b_blur_noise",
    "cone_b_noise",
    "cone_c",
    "cone_c_blur",
    "cone_c_blur_noise",
    "cone_c_noise",
    "cone_composed_a",
    "cone_composed_a_noise",
    "cone_composed_b",
    "cone_composed_b_noise",
    "cone_composed_c",
    "cone_composed_c_noise",
    "cone_composed_d",
    "cone_composed_d_noise",
    "cone_composed_e",
    "cone_composed_e_noise",
    "cone_composed_f",
    "cone_composed_f_noise",
    "cone_d",
    "cone_d_blur",
    "cone_d_blur_noise",
    "cone_d_noise",
    "cone_e",
    "cone_e_blur",
    "cone_e_blur_noise",
    "cone_e_noise",
    "fan_a",
    "fan_a_blur",
    "fan_a_gradient",
    "fan_b",
    "fan_b_blur",
    "fan_b_gradient",
    "fan_c",
    "fan_c_blur",
    "fan_c_gradient",
    "fan_d",
    "fan_d_blur",
    "fan_d_gradient",
    "foliage_canopy_a",
    "foliage_canopy_a_blur",
    "foliage_canopy_a_noise",
    "foliage_canopy_b",
    "foliage_canopy_b_blur",
    "foliage_canopy_b_noise",
    "foliage_canopy_c",
    "foliage_canopy_c_blur",
    "foliage_canopy_c_noise",
    "foliage_canopy_d",
    "foliage_canopy_d_noise",
    "ring_a",
    "ring_a_noise",
    "ring_a_streaks",
    "ring_b",
    "ring_b_noise",
    "ring_b_streaks",
    "ring_c",
    "ring_c_noise",
    "ring_c_streaks",
    "shape_a",
    "shape_b",
    "shape_c",
    "shape_d",
    "shape_e",
    "shape_f",
    "shape_g",
    "streaks_composed_a",
    "streaks_composed_a_noise",
    "streaks_composed_b",
    "streaks_composed_b_noise",
    "streaks_composed_c",
    "streaks_composed_c_noise",
    "streaks_composed_d",
    "streaks_composed_d_noise",
    "streaks_composed_e",
    "streaks_composed_e_noise",
    "streaks_composed_f",
    "streaks_composed_f_noise",
    "streaks_composed_g",
    "streaks_composed_g_noise",
    "streaks_composed_h",
    "streaks_composed_h_noise",
    "water_caustics_a",
    "water_caustics_b",
    "water_caustics_c",
    "water_caustics_d",
    "window_a",
    "window_a_blur",
    "window_a_noise",
    "window_b",
    "window_b_blur",
    "window_b_noise",
    "window_c",
    "window_c_blur",
    "window_c_noise",
    "window_d",
    "window_d_blur",
    "window_d_noise",
    "window_e",
    "window_e_blur",
    "window_e_noise",
    "window_f",
    "window_f_blur",
    "window_f_noise",
    "window_g",
    "window_g_blur",
    "window_g_noise",
    "window_h",
    "window_h_blur",
    "window_h_noise",
    "window_i",
    "window_i_blur",
    "window_i_noise",
    "window_j",
    "window_j_blur",
    "window_j_noise",
    "window_k",
    "window_k_blur",
    "window_k_noise",
];

/// Groups of masks in the picker, by the start of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Windows,
    Foliage,
    Caustics,
    Cones,
    Streaks,
    Circles,
    Fans,
    Shapes,
}

impl Category {
    pub const ALL: [Self; 8] = [
        Self::Windows,
        Self::Foliage,
        Self::Caustics,
        Self::Cones,
        Self::Streaks,
        Self::Circles,
        Self::Fans,
        Self::Shapes,
    ];

    pub fn of(name: &str) -> Self {
        match name.split('_').next().unwrap_or_default() {
            "window" => Self::Windows,
            "foliage" => Self::Foliage,
            "water" => Self::Caustics,
            "cone" => Self::Cones,
            "streaks" => Self::Streaks,
            "circle" | "ring" => Self::Circles,
            "fan" => Self::Fans,
            _ => Self::Shapes,
        }
    }

    pub fn message_id(self) -> &'static str {
        match self {
            Self::Windows => "mask-category-windows",
            Self::Foliage => "mask-category-foliage",
            Self::Caustics => "mask-category-caustics",
            Self::Cones => "mask-category-cones",
            Self::Streaks => "mask-category-streaks",
            Self::Circles => "mask-category-circles",
            Self::Fans => "mask-category-fans",
            Self::Shapes => "mask-category-shapes",
        }
    }
}

/// Where a mask that is not carried is fetched from, next to the page.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn url(name: &str) -> String {
    format!("masks/kenney/{name}.png")
}

/// Side of the picker's thumbnails, pixels.
const THUMBNAIL_SIZE: u32 = 64;

type Arrival = (&'static str, Result<Vec<u8>, String>);

/// Mask images decoded so far, their thumbnails and the ones on their way.
#[derive(Default)]
pub struct MaskLibrary {
    images: HashMap<&'static str, Arc<MaskImage>>,
    thumbnails: HashMap<&'static str, egui::TextureHandle>,
    loading: HashSet<&'static str>,
    failed: HashSet<&'static str>,
    arrived: Arc<Mutex<Vec<Arrival>>>,
}

impl MaskLibrary {
    /// The mask `name`, or `None` while it is still loading (then it is
    /// requested, and [`MaskLibrary::poll`] picks it up).
    pub fn image(&mut self, ctx: &egui::Context, name: &'static str) -> Option<Arc<MaskImage>> {
        if let Some(image) = self.images.get(name) {
            return Some(image.clone());
        }
        if let Some((_, bytes)) = BUILT_IN.iter().chain(EMBEDDED).find(|(n, _)| *n == name) {
            self.decoded(name, bytes);
            return self.images.get(name).cloned();
        }
        self.fetch(ctx, name);
        None
    }

    /// Whether `name` could not be loaded.
    pub fn failed(&self, name: &str) -> bool {
        self.failed.contains(name)
    }

    /// The picker thumbnail of `name`, once its image is there.
    pub fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        name: &'static str,
    ) -> Option<egui::TextureHandle> {
        if let Some(handle) = self.thumbnails.get(name) {
            return Some(handle.clone());
        }
        let image = self.image(ctx, name)?;
        let small = image.resampled(THUMBNAIL_SIZE);
        let picture =
            egui::ColorImage::from_gray([THUMBNAIL_SIZE as usize, THUMBNAIL_SIZE as usize], &small);
        let handle = ctx.load_texture(format!("mask-{name}"), picture, Default::default());
        self.thumbnails.insert(name, handle.clone());
        Some(handle)
    }

    /// Takes masks whose bytes arrived. Returns the names now ready.
    pub fn poll(&mut self) -> Vec<&'static str> {
        let arrived: Vec<Arrival> = std::mem::take(&mut *self.arrived.lock().unwrap());
        let mut ready = Vec::new();
        for (name, bytes) in arrived {
            self.loading.remove(name);
            match bytes {
                Ok(bytes) => {
                    self.decoded(name, &bytes);
                    if self.images.contains_key(name) {
                        ready.push(name);
                    }
                }
                Err(error) => {
                    log::warn!("fetching mask {name} failed: {error}");
                    self.failed.insert(name);
                }
            }
        }
        ready
    }

    fn decoded(&mut self, name: &'static str, bytes: &[u8]) {
        match MaskImage::decode(bytes) {
            Ok(image) => {
                self.images.insert(name, Arc::new(image));
            }
            Err(error) => {
                log::error!("mask {name} is unreadable: {error}");
                self.failed.insert(name);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn fetch(&mut self, _ctx: &egui::Context, name: &'static str) {
        // Desktop builds carry every mask; a name not among them is unknown.
        self.failed.insert(name);
    }

    #[cfg(target_arch = "wasm32")]
    fn fetch(&mut self, ctx: &egui::Context, name: &'static str) {
        if self.failed.contains(name) || !self.loading.insert(name) {
            return;
        }
        let arrived = Arc::clone(&self.arrived);
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let bytes = fetch_bytes(&url(name)).await;
            arrived.lock().unwrap().push((name, bytes));
            ctx.request_repaint();
        });
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or("no window")?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let response: web_sys::Response = response.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kenney_mask_is_listed_once() {
        let mut names = ALL.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 152);
        for (name, _) in BUILT_IN {
            assert!(ALL.contains(name), "{name}");
        }
    }

    #[test]
    fn built_in_masks_load_offline_as_shipped() {
        let ctx = egui::Context::default();
        let mut library = MaskLibrary::default();
        for (name, _) in BUILT_IN {
            let image = library.image(&ctx, name).expect(name);
            assert_eq!((image.width, image.height), (512, 512), "{name} unchanged");
        }
    }

    #[test]
    fn every_category_has_masks() {
        for category in Category::ALL {
            assert!(
                ALL.iter().any(|name| Category::of(name) == category),
                "{category:?}"
            );
        }
    }
}
