// Puts the rendered scene on screen, optionally as a value study: squint
// (blur), grey, or flat tones by lightness. One full-screen triangle.

struct Value {
    // x: mode (0 off, 1 grey, 2 tones); y: number of tones; z: balance;
    // w: 1 if the picture holds linear colour (sRGB target), 0 if encoded.
    params: vec4<f32>,
    // xy: one texel in uv; z: squint radius in texels.
    blur: vec4<f32>,
    // Screen-space cavity: x ridge and y hollow strength (0 when off);
    // z: texels per output pixel.
    cavity: vec4<f32>,
    // Contour lines: x their half width in texels (0 when off); y how much
    // the normal must turn for a fold line.
    lines: vec4<f32>,
    // rgb: line colour, linear.
    line_color: vec4<f32>,
    // Ambient occlusion: x strength (0 when off), y radius in metres,
    // z samples.
    ambient_occlusion: vec4<f32>,
    // x: tangent of half the vertical field of view; y: width over height.
    lens: vec4<f32>,
};

// The distance the surfaces picture's distances are relative to, metres
// (MAX_SURFACE_DISTANCE in shader.wgsl).
const MAX_SURFACE_DISTANCE: f32 = 200.0;

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@group(0) @binding(2) var<uniform> value: Value;
// The surfaces as the camera sees them: normal and distance (see
// FragmentOutput in shader.wgsl). Read texel by texel, never filtered.
@group(0) @binding(3) var normals: texture_2d<f32>;
// How much of each point's hemisphere is covered (see fs_occlusion), at the
// size of the surfaces picture. Only the composite reads it.
@group(1) @binding(0) var occlusion: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_composite(@builtin(vertex_index) index: u32) -> VertexOutput {
    // A triangle that covers the viewport.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOutput;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    return select(c, pow(c, vec3<f32>(1.0 / 2.2)), value.params.w > 0.5);
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    return select(c, pow(c, vec3<f32>(2.2)), value.params.w > 0.5);
}

// The surface at a texel of the picture: xy the normal's x and y as the
// camera sees them, z the distance from the eye (0 to 1, 1 beyond it all).
fn surface_at(texel: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(normals));
    let s = textureLoad(normals, clamp(texel, vec2<i32>(0), size - 1), 0);
    return vec3<f32>(s.xy * 2.0 - 1.0, s.z + s.w / 255.0);
}

// The screen-space cavity: how the normals turn from one pixel to the next.
// Turning outwards (a ridge) brightens, inwards (a hollow) darkens. Where
// the distance jumps, at an outline, there is no cavity.
fn screen_cavity(uv: vec2<f32>) -> f32 {
    if value.cavity.x + value.cavity.y <= 0.0 {
        return 1.0;
    }
    let size = vec2<f32>(textureDimensions(normals));
    let centre = vec2<i32>(uv * size);
    let step = i32(max(value.cavity.z, 1.0));
    let here = surface_at(centre);
    let right = surface_at(centre + vec2<i32>(step, 0));
    let left = surface_at(centre - vec2<i32>(step, 0));
    // Texture rows go down; the normal's y goes up.
    let up = surface_at(centre - vec2<i32>(0, step));
    let down = surface_at(centre + vec2<i32>(0, step));
    let farthest = max(max(right.z, left.z), max(up.z, down.z));
    let jump = max(abs(right.z - left.z), abs(up.z - down.z));
    if max(here.z, farthest) >= 0.999 || jump > here.z * 0.05 {
        return 1.0;
    }
    let bend = (right.x - left.x) + (up.y - down.y);
    let ridge = value.cavity.x * max(bend, 0.0);
    let hollow = value.cavity.y * max(-bend, 0.0);
    return clamp(1.0 + 1.5 * (ridge - hollow), 0.3, 2.0);
}

// Whether a contour line passes here: an outline, where the surface
// behind a neighbour is farther away (or there is none), or a fold, where
// the normal turns sharply. Lines are drawn on the nearer surface.
fn contour(uv: vec2<f32>) -> f32 {
    if value.lines.x <= 0.0 {
        return 0.0;
    }
    let size = vec2<f32>(textureDimensions(normals));
    let centre = vec2<i32>(uv * size);
    let reach = i32(max(round(value.lines.x), 1.0));
    let here = surface_at(centre);
    if here.z >= 0.999 {
        return 0.0;
    }
    let right = surface_at(centre + vec2<i32>(reach, 0));
    let left = surface_at(centre - vec2<i32>(reach, 0));
    let up = surface_at(centre - vec2<i32>(0, reach));
    let down = surface_at(centre + vec2<i32>(0, reach));
    let gap = here.z * 0.05 + 1e-4;
    let behind = max(max(right.z, left.z), max(up.z, down.z)) - here.z;
    if behind > gap {
        return 1.0;
    }
    let turn = length(right.xy - left.xy) + length(up.xy - down.xy);
    return select(0.0, 1.0, turn > value.lines.y);
}

// The point of the camera's space seen at `uv`, `distance` metres away:
// x right, y up, the camera looking along -z.
fn view_point(uv: vec2<f32>, distance: f32) -> vec3<f32> {
    let t = value.lens.x;
    let ray = vec3<f32>((uv.x * 2.0 - 1.0) * t * value.lens.y, (1.0 - uv.y * 2.0) * t, -1.0);
    return normalize(ray) * distance;
}

// Where a point of the camera's space shows in the picture.
fn picture_uv(p: vec3<f32>) -> vec2<f32> {
    let t = value.lens.x;
    let ndc = p.xy / (max(-p.z, 1e-4) * vec2<f32>(t * value.lens.y, t));
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

// Ambient occlusion: how much of the hemisphere above a point other
// surfaces cover, from the picture of distances and normals. Samples on a
// spiral turned per pixel, the turns repeating every 4 x 4 pixels so that
// `ambient_occlusion` can average the noise away over one such block.
fn covered(uv: vec2<f32>, texel: vec2<i32>) -> f32 {
    let size = vec2<f32>(textureDimensions(normals));
    let here = surface_at(texel);
    if here.z >= 0.999 {
        return 0.0;
    }
    let distance = here.z * MAX_SURFACE_DISTANCE;
    let n = vec3<f32>(here.xy, sqrt(max(1.0 - dot(here.xy, here.xy), 0.0)));
    let helper = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let tangent = normalize(cross(helper, n));
    let bitangent = cross(n, tangent);
    // Start a little off the surface: a pixel of surface seen at a grazing
    // angle spans a range of distances, which must not count as cover.
    let p = view_point(uv, distance) + n * (0.004 * distance);
    let radius = value.ambient_occlusion.y;
    let count = u32(value.ambient_occlusion.z);
    // A 4 x 4 Bayer matrix: neighbouring pixels get far-apart turns.
    var order = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let cell = (texel.x & 3) + 4 * (texel.y & 3);
    let turn = 6.2831853 * (order[cell] + 0.5) / 16.0;
    var covered = 0.0;
    for (var i = 0u; i < count; i++) {
        let f = (f32(i) + 0.5) / f32(count);
        let a = f32(i) * 2.39996323 + turn;
        // Cosine-weighted directions, nearer samples more often, none
        // almost along the surface.
        let r = sqrt(f) * 0.9;
        let direction = tangent * (cos(a) * r) + bitangent * (sin(a) * r) + n * sqrt(1.0 - r * r);
        let sample = p + direction * radius * mix(0.15, 1.0, f * f);
        let at = picture_uv(sample);
        if any(at < vec2<f32>(0.0)) || any(at > vec2<f32>(1.0)) {
            continue;
        }
        let seen = surface_at(vec2<i32>(at * size)).z * MAX_SURFACE_DISTANCE;
        let behind = length(sample) - seen;
        // Covered when a surface stands in front of the sample, but not one
        // far in front of the point: that is another object, not a corner.
        let near = 1.0 - smoothstep(0.0, 1.0, abs(length(p) - seen) / radius - 1.0);
        covered += select(0.0, near, behind > 0.02 * radius + 0.006 * seen);
    }
    return covered / f32(max(count, 1u));
}

@fragment
fn fs_occlusion(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(covered(in.uv, vec2<i32>(in.position.xy)), 0.0, 0.0, 1.0);
}

// The light ambient occlusion leaves: the cover averaged over the 4 x 4
// block of pixels around, which holds every turn of the samples once,
// leaving out pixels of another surface so edges stay sharp.
fn ambient_occlusion(uv: vec2<f32>) -> f32 {
    let strength = value.ambient_occlusion.x;
    if strength <= 0.0 {
        return 1.0;
    }
    let size = vec2<i32>(textureDimensions(occlusion));
    let centre = vec2<i32>(uv * vec2<f32>(size));
    let here = surface_at(centre);
    if here.z >= 0.999 {
        return 1.0;
    }
    var sum = 0.0;
    var total = 0.0;
    for (var y = -1; y <= 2; y++) {
        for (var x = -1; x <= 2; x++) {
            let texel = clamp(centre + vec2<i32>(x, y), vec2<i32>(0), size - 1);
            let there = surface_at(texel);
            let same = abs(there.z - here.z) <= here.z * 0.02 + 1e-4
                && dot(there.xy - here.xy, there.xy - here.xy) < 0.04;
            if same {
                sum += textureLoad(occlusion, texel, 0).r;
                total += 1.0;
            }
        }
    }
    return 1.0 - strength * sum / max(total, 1.0);
}

@fragment
fn fs_composite(in: VertexOutput) -> @location(0) vec4<f32> {
    // Squint: a Gaussian blur on a 5 x 5 grid scaled by the radius.
    let step = value.blur.xy * value.blur.z * 0.5;
    var colour = vec3<f32>(0.0);
    var total = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let offset = vec2<f32>(f32(x), f32(y));
            let weight = exp(-dot(offset, offset) / 4.5);
            colour += textureSampleLevel(picture, picture_sampler, in.uv + offset * step, 0.0).rgb * weight;
            total += weight;
        }
    }
    // Lightness on encoded (perceptual) values, as an artist judges it.
    let encoded = encode(
        colour / total * screen_cavity(in.uv) * ambient_occlusion(in.uv),
    );
    let lightness = dot(encoded, vec3<f32>(0.2126, 0.7152, 0.0722));
    var result = encoded;
    let mode = value.params.x;
    if mode > 1.5 {
        let tones = max(value.params.y, 2.0);
        // Positive balance lifts more of the picture into lighter tones.
        let shifted = pow(clamp(lightness, 0.0, 1.0), exp2(-value.params.z));
        let tone = min(floor(shifted * tones), tones - 1.0);
        result = vec3<f32>(mix(0.08, 0.95, tone / (tones - 1.0)));
    } else if mode > 0.5 {
        result = vec3<f32>(lightness);
    }
    result = mix(result, encode(value.line_color.rgb), contour(in.uv));
    return vec4<f32>(decode(result), 1.0);
}
