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
    // x: cosine of the inner half angle; y: first shadow view, or -1;
    // z: mask layer, or -1; w: a directional light's mask pattern size.
    spot_inner: vec4<f32>,
};

const MAX_SHADOW_VIEWS: u32 = 96u;

struct Shadows {
    view_proj: array<mat4x4<f32>, MAX_SHADOW_VIEWS>,
    // xy: tile origin in atlas uv; z: tile size in uv.
    tiles: array<vec4<f32>, MAX_SHADOW_VIEWS>,
    // x near, y far plane; z orthographic half width (> 0) or minus the
    // tangent of half the field of view (< 0); w the light's size: radians
    // across for a directional light, metres for the others.
    lens: array<vec4<f32>, MAX_SHADOW_VIEWS>,
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
    // x: 0 the Scene's lights; Solid: 1 studio, 2 matcap, 3 flat.
    shading: vec4<f32>,
    // xyz: the camera's right and up directions.
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    // x, y: geometry cavity ridge and hollow strength (0 when it is off).
    cavity: vec4<f32>,
    // rgb: wireframe colour, linear; a: its opacity.
    wire: vec4<f32>,
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
// One mask per light, white lets the light through.
@group(0) @binding(1) var light_masks: texture_2d_array<f32>;
@group(0) @binding(2) var light_mask_sampler: sampler;
// The Solid mode's matcap, linear colour.
@group(0) @binding(3) var matcap: texture_2d<f32>;
@group(0) @binding(4) var matcap_sampler: sampler;
@group(1) @binding(0) var<uniform> object: ObjectUniforms;
@group(2) @binding(0) var<uniform> shadows: Shadows;
@group(2) @binding(1) var shadow_atlas: texture_depth_2d;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;
// The same depths packed into RGBA8, read raw for the blocker search.
@group(2) @binding(3) var shadow_depths: texture_2d<f32>;
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

// Samples of the soft shadow filter and of the search for casters.
const SHADOW_TAPS: u32 = 24u;
const BLOCKER_TAPS: u32 = 12u;
const GOLDEN_ANGLE: f32 = 2.39996323;

// A depth written by the shadow pass's fs_shadow.
fn unpack_depth(packed: vec4<f32>) -> f32 {
    return dot(packed, vec4<f32>(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0));
}

// Distance from the light along its view, metres, of a depth in the view.
fn view_distance(depth: f32, lens: vec4<f32>) -> f32 {
    let near = lens.x;
    let far = lens.y;
    if lens.z > 0.0 {
        return near + depth * (far - near);
    }
    return near * far / (far - depth * (far - near));
}

// Point `i` of `count` spread evenly over a unit disk (a Vogel spiral),
// turned by `angle`.
fn disk_point(i: u32, count: u32, angle: f32) -> vec2<f32> {
    let r = sqrt((f32(i) + 0.5) / f32(count));
    let a = f32(i) * GOLDEN_ANGLE + angle;
    return vec2<f32>(cos(a), sin(a)) * r;
}

// A different angle for every pixel, so the few filter samples leave fine
// noise instead of bands (interleaved gradient noise).
fn pixel_angle(pixel: vec2<f32>) -> f32 {
    return 6.2831853 * fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

// How much of the light reaches `point`, 0 to 1, from one shadow view.
// `point` is the surface point already pushed off the surface.
//
// Soft shadows as with a light of real size (percentage-closer soft
// shadows): first find how far the casters around are from the light,
// then filter the depth comparisons over the penumbra that gives. Close to
// the caster the penumbra is narrow, farther away it widens. The comparison
// sampler becomes a sampler2DShadow on WebGL2; raw depths come from the
// packed colour atlas, as WebGL2 cannot read them from a depth texture.
fn lit_fraction(view: u32, point: vec3<f32>, angle: f32) -> f32 {
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
    let lens = shadows.lens[view];
    let atlas_size = shadows.params.x;
    let texel = 1.0 / atlas_size;
    let uv = tile.xy + (ndc.xy * vec2<f32>(0.5, -0.5) + 0.5) * tile.z;
    // Stay inside this view's tile, half a texel from its edge.
    let low = tile.xy + vec2<f32>(texel * 1.5);
    let high = tile.xy + vec2<f32>(tile.z - texel * 1.5);

    let orthographic = lens.z > 0.0;
    let receiver = view_distance(depth, lens);
    // Metres the tile spans at the receiver, across.
    let tile_width = select(2.0 * receiver * -lens.z, 2.0 * lens.z, orthographic);
    let uv_per_metre = tile.z / tile_width;
    let min_radius = texel * 1.5;
    let max_radius = tile.z * 0.08;

    // Casters that can hide part of the light from this point lie within
    // the light's size around the line to it.
    let reach = select(lens.w * 0.5, lens.w * 0.5 * receiver, orthographic);
    let search = clamp(reach * uv_per_metre, min_radius * 2.0, max_radius);
    let bias = 0.005 + receiver * 0.002;
    var blockers = 0.0;
    var blocker_sum = 0.0;
    for (var i = 0u; i < BLOCKER_TAPS; i++) {
        let at = clamp(uv + disk_point(i, BLOCKER_TAPS, angle) * search, low, high);
        let raw = unpack_depth(textureLoad(shadow_depths, vec2<i32>(at * atlas_size), 0));
        let distance = view_distance(min(raw, 1.0), lens);
        if distance < receiver - bias {
            blockers += 1.0;
            blocker_sum += distance;
        }
    }
    if blockers == 0.0 {
        return 1.0;
    }
    let blocker = blocker_sum / blockers;
    // The penumbra widens with the gap between caster and receiver: by the
    // light's angle for a directional light, by its size over the caster's
    // distance for the others.
    let spread = select(lens.w / max(blocker, 1e-3), lens.w, orthographic);
    let penumbra = (receiver - blocker) * spread * uv_per_metre;
    let radius = clamp(penumbra * 0.5, min_radius, max_radius);

    var lit = 0.0;
    for (var i = 0u; i < SHADOW_TAPS; i++) {
        let at = clamp(uv + disk_point(i, SHADOW_TAPS, angle) * radius, low, high);
        lit += textureSampleCompareLevel(shadow_atlas, shadow_sampler, at, depth);
    }
    return lit / f32(SHADOW_TAPS);
}

// Two directions across `forward`, the same for every pixel, so a mask
// keeps its orientation.
fn across(forward: vec3<f32>) -> mat2x3<f32> {
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(forward.y) > 0.99);
    let right = normalize(cross(forward, up));
    return mat2x3<f32>(right, cross(right, forward));
}

// How much of `light` its mask lets through to `world`, 0 to 1. A spot
// light stretches the mask over its cone; a directional light repeats it,
// one copy per pattern size, like sun through a window frame.
fn mask_through(light: Light, world: vec3<f32>) -> f32 {
    let layer = light.spot_inner.z;
    if layer < 0.0 {
        return 1.0;
    }
    var uv: vec2<f32>;
    if light.position.w > 0.5 {
        let forward = light.spot.xyz;
        let offset = world - light.position.xyz;
        let ahead = dot(offset, forward);
        if ahead <= 0.0 {
            return 0.0;
        }
        let cos_outer = light.spot.w;
        let tan_outer = sqrt(max(1.0 - cos_outer * cos_outer, 1e-6)) / cos_outer;
        let side = across(forward);
        let at = vec2<f32>(dot(offset, side[0]), dot(offset, side[1])) / (ahead * tan_outer);
        // Half a texel in from the edge, so the repeating sampler never
        // wraps around.
        uv = clamp(at * vec2<f32>(0.5, -0.5) + 0.5, vec2<f32>(0.001), vec2<f32>(0.999));
    } else {
        let side = across(-light.position.xyz);
        uv = vec2<f32>(dot(world, side[0]), -dot(world, side[1])) / light.spot_inner.w + 0.5;
    }
    return textureSampleLevel(light_masks, light_mask_sampler, uv, i32(layer), 0.0).r;
}

// The Solid mode: light that ignores the Scene's lights, fixed to the view
// so form reads the same from every side. `n` is the surface normal, `v`
// the direction to the eye.
fn solid_color(n: vec3<f32>, v: vec3<f32>, albedo: vec3<f32>) -> vec3<f32> {
    let mode = u.shading.x;
    if mode > 2.5 {
        // Flat: one colour, no light.
        return albedo;
    }
    // The normal as the camera sees it: x right, y up, z towards the eye.
    let facing = vec3<f32>(dot(n, u.camera_right.xyz), dot(n, u.camera_up.xyz), dot(n, v));
    if mode > 1.5 {
        // The matcap's sphere, seen face on: the normal picks its point.
        let uv = facing.xy * vec2<f32>(0.49, -0.49) + 0.5;
        return textureSampleLevel(matcap, matcap_sampler, uv, 0.0).rgb * albedo;
    }
    // Studio: a key light from the upper left, a weaker fill from the right
    // and soft light from above.
    let key = max(dot(facing, normalize(vec3<f32>(-0.65, 0.55, 0.5))), 0.0);
    let fill = max(dot(facing, normalize(vec3<f32>(0.75, 0.0, 0.6))), 0.0);
    let sky = 0.12 + 0.08 * facing.y;
    return albedo * (0.9 * key + 0.2 * fill + sky);
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // How the mesh bends here: positive on ridges, negative in hollows.
    @location(3) curvature: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) curvature: f32,
};

// The geometry cavity: brighter on ridges, darker in hollows.
fn geometry_cavity(curvature: f32) -> f32 {
    let ridge = u.cavity.x * max(curvature, 0.0);
    let hollow = u.cavity.y * max(-curvature, 0.0);
    return clamp(1.0 + 2.0 * (ridge - hollow), 0.0, 3.0);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = object.model * vec4<f32>(in.position, 1.0);
    out.clip_position = u.view_proj * world;
    out.world_position = world.xyz;
    out.normal = (object.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.curvature = in.curvature;
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

// The surfaces pass (fs_surface) draws, for the screen-space overlays, each
// surface as the camera sees it: rg the normal's x (right) and y (up) from
// -1..1 to 0..1; ba the distance from the eye over MAX_SURFACE_DISTANCE in
// two 8-bit halves. It is never multisampled: averaging the halves would
// break them. Where nothing was drawn it stays cleared to the farthest.
const MAX_SURFACE_DISTANCE: f32 = 200.0;

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

    if u.shading.x > 0.5 {
        var solid = solid_color(n, v, albedo) * geometry_cavity(in.curvature);
        solid = mix(solid, object.highlight.rgb, object.highlight.a);
        if u.params.y == 1u {
            solid = pow(solid, vec3<f32>(1.0 / 2.2));
        }
        return vec4<f32>(solid, 1.0);
    }

    var direct = vec3<f32>(0.0);
    let shadow_angle = pixel_angle(in.clip_position.xy);
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
        attenuation *= mask_through(light, in.world_position);
        if light.spot_inner.y >= 0.0 {
            let point = in.world_position + n * NORMAL_OFFSET;
            var view = u32(light.spot_inner.y);
            // A point light: positional and without a cone, six views. The
            // face is picked for the same point that is looked up.
            if light.position.w > 0.5 && light.spot.w < -1.5 {
                view += cube_face(point - light.position.xyz);
            }
            attenuation *= lit_fraction(view, point, shadow_angle);
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
    var color = (direct + ambient) * geometry_cavity(in.curvature);
    color = mix(color, object.highlight.rgb, object.highlight.a);
    if u.params.y == 1u {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(color, 1.0);
}

// The wireframe: each mesh edge as a line, pulled a little towards the eye
// so it is not hidden by the faces it borders.
@vertex
fn vs_wire(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    var clip = u.view_proj * object.model * vec4<f32>(position, 1.0);
    clip.z -= 5e-5 * clip.w;
    return clip;
}

@fragment
fn fs_wire() -> @location(0) vec4<f32> {
    var color = u.wire.rgb;
    if u.params.y == 1u {
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(color, u.wire.a);
}

// The surfaces pass: normal and distance as the camera sees them (see
// MAX_SURFACE_DISTANCE). The mesh's own normal, without normal maps.
@fragment
fn fs_surface(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    if !front_facing {
        n = -n;
    }
    let facing = vec2<f32>(dot(n, u.camera_right.xyz), dot(n, u.camera_up.xyz));
    let distance = clamp(length(u.eye.xyz - in.world_position) / MAX_SURFACE_DISTANCE, 0.0, 0.999);
    let high = floor(distance * 255.0) / 255.0;
    let low = fract(distance * 255.0);
    return vec4<f32>(facing * 0.5 + 0.5, high, low);
}
