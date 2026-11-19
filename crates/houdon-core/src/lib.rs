//! Platform-independent core of HoudonRef: model data, loading and camera math.
//!
//! Nothing in this crate touches the GPU or the UI, so it can be unit-tested
//! and reused by every section of the app (Atlas, Studio, Atelier).

pub mod camera;
pub mod light;
pub mod model;
pub mod room;
pub mod scene;
pub mod units;

pub use camera::OrbitCamera;
pub use light::{Light, LightId, LightKind};
pub use model::{Bounds, LoadError, Model, Vertex};
pub use room::{BuiltinRoom, Room, RoomShape};
pub use scene::{
    MAX_LIGHTS, MIN_EYE_HEIGHT, ModelId, Object, ObjectId, PLAUSIBLE_SIZE, Scene, Transform,
};
pub use units::{Length, LengthUnit, UnitSystem};
