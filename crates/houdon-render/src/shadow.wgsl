// The pass that renders the shadow casters as one light view sees them:
// into the depth atlas, and the same depth packed into the colour atlas.

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
    fibre_axis: vec4<f32>,
    fibre_centre: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ShadowView;
@group(1) @binding(0) var<uniform> object: ObjectUniforms;

@vertex
fn vs_shadow(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return view.view_proj * object.model * vec4<f32>(position, 1.0);
}

// A depth in [0, 1) as four 8-bit channels; the shader's unpack_depth
// reverses it. 1 (the clear colour, white) stays the farthest.
@fragment
fn fs_shadow(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let d = clamp(position.z, 0.0, 0.99999994);
    var packed = fract(d * vec4<f32>(1.0, 255.0, 65025.0, 16581375.0));
    packed -= packed.yzww * vec4<f32>(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return packed;
}
