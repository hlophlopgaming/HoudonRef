//! Renders scenes without a window and checks what an artist would see.
//! One test binary, so every test shares one GPU device (see `renderer`).

use std::sync::{Mutex, MutexGuard, OnceLock};

use houdon_core::{Model, Scene, model::DEFAULT_FIGURE_HEIGHT};
use houdon_render::{AntiAliasing, Frame, Image, OffscreenRenderer};

pub(crate) const MALE: &[u8] = include_bytes!("../../../../assets/models/test/male.glb");
pub(crate) const SIZE: u32 = 64;

/// The one renderer all tests share, like the app has one GPU device for
/// its whole run. Tests take turns with it. Opening and dropping a device
/// per test, from parallel test threads, once crashed Mesa's software
/// Vulkan driver (lavapipe) on the CI runner.
static RENDERER: OnceLock<Result<Mutex<OffscreenRenderer>, String>> = OnceLock::new();

/// The shared renderer, or `None` with a note when this machine has no GPU
/// adapter.
///
/// CI sets `HOUDON_REQUIRE_GPU=1`, so there a missing adapter fails the test
/// instead of silently skipping it.
pub(crate) fn renderer() -> Option<MutexGuard<'static, OffscreenRenderer>> {
    let shared = RENDERER.get_or_init(|| {
        OffscreenRenderer::new_headless()
            .map(Mutex::new)
            .map_err(|error| error.to_string())
    });
    match shared {
        // A test that failed while holding the renderer leaves it usable.
        Ok(renderer) => Some(
            renderer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        ),
        Err(error) if std::env::var_os("HOUDON_REQUIRE_GPU").is_some() => {
            panic!("HOUDON_REQUIRE_GPU is set but no renderer is available: {error}")
        }
        Err(error) => {
            eprintln!("skipping render test: {error}");
            None
        }
    }
}

pub(crate) fn figure_scene() -> Scene {
    let mut model = Model::from_gltf_slice("male", MALE).unwrap();
    model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
    let mut scene = Scene::new();
    // Most tests look at objects against a plain backdrop colour.
    scene.room.visible = false;
    let id = scene.add_model(model);
    scene.add_object(id).unwrap();
    scene.frame_camera(1.0);
    scene
}

pub(crate) fn render(renderer: &mut OffscreenRenderer, scene: &Scene) -> Image {
    let image = renderer.render(&Frame::from_scene(scene, 1.0), SIZE, SIZE);
    assert_eq!((image.width, image.height), (SIZE, SIZE));
    image
}

pub(crate) fn background_pixels(image: &Image, background: [u8; 3]) -> usize {
    image
        .pixels()
        .filter(|p| {
            p[..3]
                .iter()
                .zip(background)
                .all(|(a, b)| a.abs_diff(b) <= 1)
        })
        .count()
}

mod ambient_occlusion;
mod anti_aliasing;
mod export;
mod lights;
mod masks;
mod materials;
mod objects;
mod overlays;
mod quality;
mod room;
mod shadows;
mod soft_shadows;
mod solid;
mod values;

use lights::*;
use materials::*;
use objects::*;
use room::*;
use shadows::*;
use solid::*;
