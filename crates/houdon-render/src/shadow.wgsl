// Depth-only pass that renders the shadow casters as one light view sees them.

struct ShadowView {
    view_proj: mat4x4<f32>,
};

struct ObjectUniforms {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    highlight: vec4<f32>,
    color_below: vec4<f32>,
    color_above: vec4<f32>,
    base_color: vec4<f32>,
    material: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ShadowView;
@group(1) @binding(0) var<uniform> object: ObjectUniforms;

@vertex
fn vs_shadow(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return view.view_proj * object.model * vec4<f32>(position, 1.0);
}
