// Puts the rendered scene on screen, optionally as a value study: squint
// (blur), grey, or flat tones by lightness. One full-screen triangle.

struct Value {
    // x: mode (0 off, 1 grey, 2 tones); y: number of tones; z: balance;
    // w: 1 if the picture holds linear colour (sRGB target), 0 if encoded.
    params: vec4<f32>,
    // xy: one texel in uv; z: squint radius in texels.
    blur: vec4<f32>,
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@group(0) @binding(2) var<uniform> value: Value;

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
    let encoded = encode(colour / total);
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
