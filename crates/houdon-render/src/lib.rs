//! 3D rendering for HoudonRef: the Studio viewport inside egui and
//! offscreen pictures of a scene.
//!
//! Everything here stays within the WebGL2 / OpenGL ES 3.0 feature level so
//! the same renderer runs on weak desktops, in the browser and on Android
//! (ADR 0001).

mod composite;
mod frame;
mod light_mask;
mod matcap;
mod material;
#[cfg(not(target_arch = "wasm32"))]
mod offscreen;
mod renderer;
mod shadow;

pub use composite::AntiAliasing;
pub use frame::{Frame, FrameObject, FrameSurface, ObjectColors};
#[cfg(not(target_arch = "wasm32"))]
pub use offscreen::{Image, OffscreenError, OffscreenRenderer};
pub use renderer::{SceneRenderer, ViewportCallback};
pub use shadow::{DEFAULT_ATLAS_SIZE, MAX_SHADOW_VIEWS};
