// Clay shader: up to 16 directional, point and spot lights plus a
// sky/ground ambient term. Kept within the WebGL2 feature level: lights live
// in a uniform array, no storage buffers, no compute.

const MAX_LIGHTS: u32 = 16u;

struct Light {
    // xyz: position (w = 1) or direction towards the light (w = 0).
    position: vec4<f32>,
    // rgb: colour times intensity.
    color: vec4<f32>,
    // xyz: direction the spot points; w: cosine of the outer half angle,
    // or -2 for lights without a cone.
    spot: vec4<f32>,
    // x: cosine of the inner half angle.
    spot_inner: vec4<f32>,
};

struct Globals {
    view_proj: mat4x4<f32>,
    // rgb: sky colour; a: ambient intensity.
    sky_color: vec4<f32>,
    // rgb: ground colour.
    ground_color: vec4<f32>,
    // rgb: surface albedo.
    base_color: vec4<f32>,
    // x: number of lights; y: 1 if the output needs gamma encoding in the
    // shader (non-sRGB framebuffer).
    params: vec4<u32>,
    lights: array<Light, MAX_LIGHTS>,
};

struct ObjectUniforms {
    model: mat4x4<f32>,
    // Inverse transpose of the model matrix, for normals.
    normal: mat4x4<f32>,
    // rgb: selection highlight colour; a: how much of it to mix in.
    highlight: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Globals;
@group(1) @binding(0) var<uniform> object: ObjectUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = object.model * vec4<f32>(in.position, 1.0);
    out.clip_position = u.view_proj * world;
    out.world_position = world.xyz;
    out.normal = (object.normal * vec4<f32>(in.normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    if !front_facing {
        n = -n;
    }
    var direct = vec3<f32>(0.0);
    let count = min(u.params.x, MAX_LIGHTS);
    for (var i = 0u; i < count; i++) {
        let light = u.lights[i];
        var to_light = light.position.xyz;
        var attenuation = 1.0;
        if light.position.w > 0.5 {
            let offset = light.position.xyz - in.world_position;
            let distance_squared = max(dot(offset, offset), 1e-4);
            to_light = offset * inverseSqrt(distance_squared);
            attenuation = 1.0 / distance_squared;
            if light.spot.w > -1.5 {
                let cos_angle = dot(-to_light, light.spot.xyz);
                attenuation *= smoothstep(light.spot.w, light.spot_inner.x, cos_angle);
            }
        }
        direct += max(dot(n, normalize(to_light)), 0.0) * light.color.rgb * attenuation;
    }
    let ambient = mix(u.ground_color.rgb, u.sky_color.rgb, n.y * 0.5 + 0.5) * u.sky_color.a;
    var color = u.base_color.rgb * (direct + ambient);
    color = mix(color, object.highlight.rgb, object.highlight.a);
    if u.params.y == 1u {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(color, 1.0);
}
