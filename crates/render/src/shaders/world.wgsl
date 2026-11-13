// PSX world shader: vertex snapping (GTE integer screen coords), affine texture mapping
// (noperspective UVs), screen-space Gouraud colour, per-vertex fog, 15-bit colour with
// ordered 4x4 Bayer dithering.

struct Globals {
    view_proj: mat4x4<f32>,
    cam: vec4<f32>,       // xyz camera, w time
    fog_color: vec4<f32>, // rgb fog, a = lightning flash
    fog: vec4<f32>,       // near, far, snap strength, dither strength
    res: vec4<f32>,       // target w, h, viewport w, viewport h
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) layer: u32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(linear) uv: vec2<f32>,
    @location(1) @interpolate(linear) color: vec4<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(linear) fog: f32,
};

@vertex
fn vs(v: VIn) -> VOut {
    var o: VOut;
    var c = g.view_proj * vec4<f32>(v.pos, 1.0);
    if (c.w > 0.5) { // only in front of the near plane, or clipping breaks
        let half = g.res.zw * 0.5;
        let ndc = c.xy / c.w;
        let snapped = (floor(ndc * half) + 0.5) / half;
        c = vec4<f32>(mix(ndc, snapped, g.fog.z) * c.w, c.z, c.w);
    }
    o.clip = c;
    o.uv = v.uv;
    o.color = v.color;
    o.layer = v.layer & 0xFFu;
    let d = distance(v.pos, g.cam.xyz);
    var f = clamp((d - g.fog.x) / max(g.fog.y - g.fog.x, 0.001), 0.0, 1.0);
    if ((v.layer & 0x100u) != 0u) {
        f = f * 0.45;
    }
    o.fog = f;
    return o;
}

fn bayer(p: vec2<u32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return m[(p.y % 4u) * 4u + (p.x % 4u)] / 16.0 - 0.5;
}

fn psx15(c: vec3<f32>, p: vec2<f32>, amount: f32) -> vec3<f32> {
    let d = bayer(vec2<u32>(p)) * amount;
    return floor(clamp(c * 31.0 + vec3<f32>(d), vec3<f32>(0.0), vec3<f32>(31.0)) + 0.5) / 31.0;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv, i.layer);
    if (t.a < 0.5) {
        discard;
    }
    // PSX texture modulation: vertex colour 128 is neutral.
    var c = t.rgb * i.color.rgb * 2.0;
    c = c + vec3<f32>(g.fog_color.a);
    c = mix(c, g.fog_color.rgb, i.fog);
    return vec4<f32>(psx15(c, i.clip.xy, g.fog.w), i.color.a);
}
