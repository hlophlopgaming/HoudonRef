// Scene shader: metallic-roughness materials (or plain colours) under up to
// 16 directional, point and spot lights with shadows, plus the room's
// ambient light. Kept within the WebGL2 feature level: lights live in a
// uniform array, no storage buffers, no compute.

const PI: f32 = 3.14159265;

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
    // xyz: camera position.
    eye: vec4<f32>,
    // x: number of lights; y: 1 if the output needs gamma encoding in the
    // shader (non-sRGB framebuffer).
    params: vec4<u32>,
    lights: array<Light, MAX_LIGHTS>,
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
// ObjectUniforms, per drawn part: model matrix; its inverse transpose for
// normals; highlight (rgb, a = amount); plain colours below and above a
// world height (in a); material base colour; material x = metallic,
// y = roughness, z = 1 to use the material, w = normal map scale.

@group(0) @binding(0) var<uniform> u: Globals;
@group(1) @binding(0) var<uniform> object: ObjectUniforms;
@group(2) @binding(0) var<uniform> shadows: Shadows;
@group(2) @binding(1) var shadow_atlas: texture_depth_2d;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;
@group(3) @binding(0) var base_color_texture: texture_2d<f32>;
@group(3) @binding(1) var metallic_roughness_texture: texture_2d<f32>;
@group(3) @binding(2) var normal_texture: texture_2d<f32>;
@group(3) @binding(3) var material_sampler: sampler;

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
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = object.model * vec4<f32>(in.position, 1.0);
    out.clip_position = u.view_proj * world;
    out.world_position = world.xyz;
    out.normal = (object.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    return out;
}

// Bends `n` by a tangent-space normal from the normal map, with a tangent
// frame worked out from screen-space derivatives (no tangents needed).
// Returns `n` unchanged where the texture coordinates do not vary.
fn perturb_normal(n: vec3<f32>, world: vec3<f32>, uv: vec2<f32>, mapped: vec3<f32>) -> vec3<f32> {
    let dp1 = dpdx(world);
    let dp2 = dpdy(world);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    let t = dp2perp * duv1.x + dp1perp * duv2.x;
    let b = dp2perp * duv1.y + dp1perp * duv2.y;
    let size = max(dot(t, t), dot(b, b));
    let frame = mat3x3<f32>(t * inverseSqrt(max(size, 1e-20)), b * inverseSqrt(max(size, 1e-20)), n);
    return select(n, normalize(frame * mapped), size > 1e-20);
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    return (n_dot_v / (n_dot_v * (1.0 - k) + k)) * (n_dot_l / (n_dot_l * (1.0 - k) + k));
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    if !front_facing {
        n = -n;
    }

    // Textures and derivatives first, where control flow is still uniform.
    let base_sample = textureSample(base_color_texture, material_sampler, in.uv);
    let mr_sample = textureSample(metallic_roughness_texture, material_sampler, in.uv);
    let normal_sample = textureSample(normal_texture, material_sampler, in.uv).xyz * 2.0 - 1.0;
    let mapped = normalize(vec3<f32>(normal_sample.xy * object.material.w, normal_sample.z));
    let bumped = perturb_normal(n, in.world_position, in.uv, mapped);

    let use_material = object.material.z > 0.5;
    let blend = smoothstep(object.color_below.a, object.color_above.a, in.world_position.y);
    let plain = mix(object.color_below.rgb, object.color_above.rgb, blend);
    let albedo = select(plain, object.base_color.rgb * base_sample.rgb, use_material);
    let metallic = select(object.material.x, object.material.x * mr_sample.b, use_material);
    let roughness = clamp(select(object.material.y, object.material.y * mr_sample.g, use_material), 0.04, 1.0);
    n = select(n, bumped, use_material);

    let v = normalize(u.eye.xyz - in.world_position);
    let n_dot_v = max(dot(n, v), 1e-4);
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);

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
        let l = normalize(to_light);
        let n_dot_l = max(dot(n, l), 0.0);
        let h = normalize(l + v);
        let f = fresnel_schlick(max(dot(h, v), 0.0), f0);
        let specular = distribution_ggx(max(dot(n, h), 0.0), roughness)
            * geometry_smith(n_dot_v, n_dot_l, roughness) * f
            / (4.0 * n_dot_v * max(n_dot_l, 1e-4));
        let diffuse = (1.0 - f) * (1.0 - metallic) * albedo;
        // Diffuse keeps the old Lambert scale (no 1/pi), so specular gets pi.
        direct += (diffuse + specular * PI) * light.color.rgb * attenuation * n_dot_l;
    }
    let ambient_light = mix(u.floor_color.rgb, u.backdrop_color.rgb, n.y * 0.5 + 0.5) * u.backdrop_color.a;
    let ambient = ambient_light * ((1.0 - metallic) * albedo + f0 * (1.0 - roughness) * 0.5);
    var color = direct + ambient;
    color = mix(color, object.highlight.rgb, object.highlight.a);
    if u.params.y == 1u {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(color, 1.0);
}
