//! Platform-independent core of HoudonRef: model data, loading and camera math.
//!
//! Nothing in this crate touches the GPU or the UI, so it can be unit-tested
//! and reused by every section of the app (Atlas, Studio, Atelier).

pub mod camera;
pub mod light;
pub mod light_preset;
pub mod mask;
pub mod matcap;
pub mod model;
pub mod preset;
pub mod room;
pub mod scene;
pub mod shading;
pub mod units;

pub use camera::OrbitCamera;
pub use light::{Light, LightId, LightKind, MAX_SHADOW_ANGLE};
pub use light_preset::{LightPreset, LightPresetError};
pub use mask::{DEFAULT_PATTERN_SIZE, LightMask, MaskError, MaskImage};
pub use matcap::{Matcap, MatcapError, MatcapImage};
pub use model::{Bounds, LoadError, Material, MeshPart, Model, TextureImage, Vertex};
pub use preset::{CameraPreset, CameraTarget, MAX_PRESET_NAME_BYTES, PresetError, RoomPreset};
pub use room::{BuiltinRoom, MIN_ROOM_REACH, Room, RoomShape};
pub use scene::{
    MAX_LIGHTS, MIN_EYE_HEIGHT, ModelId, Object, ObjectId, PLAUSIBLE_SIZE, Scene, Transform,
};
pub use shading::{
    Cavity, ColorSource, DEFAULT_CLAY_COLOR, Lines, Shading, ShadingMode, SolidLighting, Surface,
    ValueMode, ValueStudy,
};
pub use units::{Length, LengthUnit, UnitSystem};
