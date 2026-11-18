//! 3D rendering for HoudonRef: the Studio viewport inside egui and
//! offscreen pictures of a scene.
//!
//! Everything here stays within the WebGL2 / OpenGL ES 3.0 feature level so
//! the same renderer runs on weak desktops, in the browser and on Android
//! (ADR 0001).

mod frame;
#[cfg(not(target_arch = "wasm32"))]
mod offscreen;
mod renderer;

pub use frame::{Frame, FrameObject};
#[cfg(not(target_arch = "wasm32"))]
pub use offscreen::{Image, OffscreenError, OffscreenRenderer};
pub use renderer::{CLAY_COLOR, SceneRenderer, ViewportCallback};
