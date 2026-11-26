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
mod offscreen;
mod quality;
mod renderer;
mod shadow;

pub use composite::AntiAliasing;
pub use frame::{Frame, FrameObject, FrameSurface, Highlight, ObjectColors};
pub use offscreen::{ExportSize, Image, OffscreenError, OffscreenRenderer, PendingImage};
pub use quality::{QualityGovernor, QualityLevel, RenderQuality};
pub use renderer::{SceneRenderer, ViewportCallback};
pub use shadow::{DEFAULT_ATLAS_SIZE, MAX_SHADOW_VIEWS};
