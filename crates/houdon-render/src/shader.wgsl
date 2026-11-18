// Clay shader: one directional key light plus a sky/ground ambient term.
// Kept within the WebGL2 feature level: no storage buffers, no compute.

struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the light (world space); w: 1 if the output
    // needs gamma encoding in the shader (non-sRGB framebuffer).
    light_dir: vec4<f32>,
    // rgb: key light colour; a: intensity.
    light_color: vec4<f32>,
    // rgb: sky colour; a: ambient intensity.
    sky_color: vec4<f32>,
    // rgb: ground colour.
    ground_color: vec4<f32>,
    // rgb: surface albedo.
    base_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = u.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    if !front_facing {
        n = -n;
    }
    let l = normalize(u.light_dir.xyz);
    let key = max(dot(n, l), 0.0) * u.light_color.rgb * u.light_color.a;
    let ambient = mix(u.ground_color.rgb, u.sky_color.rgb, n.y * 0.5 + 0.5) * u.sky_color.a;
    var color = u.base_color.rgb * (key + ambient);
    if u.light_dir.w > 0.5 {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(color, 1.0);
}
