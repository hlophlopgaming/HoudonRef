//! Platform-independent core of HoudonRef: model data, loading and camera math.
//!
//! Nothing in this crate touches the GPU or the UI, so it can be unit-tested
//! and reused by every section of the app (Atlas, Studio, Atelier).

pub mod camera;
pub mod model;
pub mod scene;
pub mod units;

pub use camera::OrbitCamera;
pub use model::{Bounds, LoadError, Model, Vertex};
pub use scene::{Lighting, ModelId, Object, ObjectId, Scene, Transform};
pub use units::{Length, LengthUnit, UnitSystem};
