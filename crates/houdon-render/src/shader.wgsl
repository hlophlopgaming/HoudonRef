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
    // x: cosine of the inner half angle; y: first shadow view, or -1.
    spot_inner: vec4<f32>,
};

const MAX_SHADOW_VIEWS: u32 = 96u;

struct Shadows {
    view_proj: array<mat4x4<f32>, MAX_SHADOW_VIEWS>,
    // xy: tile origin in atlas uv; z: tile size in uv.
    tiles: array<vec4<f32>, MAX_SHADOW_VIEWS>,
    // x: atlas size in texels.
    params: vec4<f32>,
};

struct Globals {
    view_proj: mat4x4<f32>,
    // rgb: ambient colour from above (the backdrop); a: ambient intensity.
    backdrop_color: vec4<f32>,
    // rgb: ambient colour from below (the floor), the reflex.
    floor_color: vec4<f32>,
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
    // rgb: surface colour below world height a.
    color_below: vec4<f32>,
    // rgb: surface colour above world height a.
    color_above: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Globals;
@group(1) @binding(0) var<uniform> object: ObjectUniforms;
@group(2) @binding(0) var<uniform> shadows: Shadows;
@group(2) @binding(1) var shadow_atlas: texture_depth_2d;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;

// Pushes the looked-up point off the surface, metres, against shadow acne.
const NORMAL_OFFSET: f32 = 0.02;

// Which face of a point light's shadow cube looks towards `v`:
// +X, -X, +Y, -Y, +Z, -Z.
fn cube_face(v: vec3<f32>) -> u32 {
    let a = abs(v);
    if a.x >= a.y && a.x >= a.z {
        return select(1u, 0u, v.x > 0.0);
    }
    if a.y >= a.z {
        return select(3u, 2u, v.y > 0.0);
    }
    return select(5u, 4u, v.z > 0.0);
}

// How much of the light reaches `world`, 0 to 1, from one shadow view.
// 3 x 3 filtered depth comparisons soften the edge; the comparison sampler
// becomes a sampler2DShadow on WebGL2.
// `point` is the surface point already pushed off the surface.
fn lit_fraction(view: u32, point: vec3<f32>) -> f32 {
    let clip = shadows.view_proj[view] * vec4<f32>(point, 1.0);
    if clip.w <= 0.0 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    if abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0 || ndc.z < 0.0 {
        return 1.0;
    }
    // Beyond the far plane is behind every caster: still in their shadow.
    let depth = min(ndc.z, 1.0);
    let tile = shadows.tiles[view];
    let texel = 1.0 / shadows.params.x;
    let uv = tile.xy + (ndc.xy * vec2<f32>(0.5, -0.5) + 0.5) * tile.z;
    // Stay inside this view's tile, half a texel from its edge.
    let low = tile.xy + vec2<f32>(texel * 1.5);
    let high = tile.xy + vec2<f32>(tile.z - texel * 1.5);
    var lit = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let at = clamp(uv + vec2<f32>(f32(dx), f32(dy)) * texel, low, high);
            lit += textureSampleCompareLevel(shadow_atlas, shadow_sampler, at, depth);
        }
    }
    return lit / 9.0;
}

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
        if light.spot_inner.y >= 0.0 {
            let point = in.world_position + n * NORMAL_OFFSET;
            var view = u32(light.spot_inner.y);
            // A point light: positional and without a cone, six views. The
            // face is picked for the same point that is looked up.
            if light.position.w > 0.5 && light.spot.w < -1.5 {
                view += cube_face(point - light.position.xyz);
            }
            attenuation *= lit_fraction(view, point);
        }
        direct += max(dot(n, normalize(to_light)), 0.0) * light.color.rgb * attenuation;
    }
    let ambient = mix(u.floor_color.rgb, u.backdrop_color.rgb, n.y * 0.5 + 0.5) * u.backdrop_color.a;
    let blend = smoothstep(object.color_below.a, object.color_above.a, in.world_position.y);
    let albedo = mix(object.color_below.rgb, object.color_above.rgb, blend);
    var color = albedo * (direct + ambient);
    color = mix(color, object.highlight.rgb, object.highlight.a);
    if u.params.y == 1u {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(color, 1.0);
}
