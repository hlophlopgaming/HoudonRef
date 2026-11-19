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
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@group(0) @binding(2) var<uniform> value: Value;
// The surfaces as the camera sees them: normal and distance (see
// FragmentOutput in shader.wgsl). Read texel by texel, never filtered.
@group(0) @binding(3) var normals: texture_2d<f32>;

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
    let encoded = encode(colour / total * screen_cavity(in.uv));
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
    return vec4<f32>(decode(result), 1.0);
}
