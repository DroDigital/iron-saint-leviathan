// Fullscreen passes: fake-FMV (PS1 video compression look) and the final present/upscale with
// optional CRT (scanlines, shadow mask, slight bloom) and composite (chroma bleed).

struct P {
    a: vec4<f32>, // fmv: amount, time, src w, src h  | present: rect x0, y0, x1, y1 (output px)
    b: vec4<f32>, // present: out w, out h, src w, src h
    c: vec4<f32>, // present: crt on, scanline strength, composite on, time
};
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> p: P;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn load(c: vec2<i32>) -> vec3<f32> {
    let d = vec2<i32>(textureDimensions(src));
    return textureLoad(src, clamp(c, vec2<i32>(0), d - vec2<i32>(1)), 0).rgb;
}

fn rgb2yuv(c: vec3<f32>) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return vec3<f32>(y, (c.b - y) * 0.565, (c.r - y) * 0.713);
}
fn yuv2rgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + 1.403 * c.z, c.x - 0.344 * c.y - 0.714 * c.z, c.x + 1.770 * c.y);
}

@fragment
fn fs_fmv(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(pos.xy);
    let amount = p.a.x;
    let blk = (px / 8) * 8;
    // Macroblocks: blend toward the block's DC (average of 4 taps).
    let dc = (load(blk + vec2<i32>(2, 2)) + load(blk + vec2<i32>(5, 2)) + load(blk + vec2<i32>(2, 5)) + load(blk + vec2<i32>(5, 5))) * 0.25;
    let here = load(px);
    var yuv = rgb2yuv(mix(here, dc, amount * 0.45));
    // Chroma: subsampled per 2x2 and smeared to the right.
    let cs = rgb2yuv(load((px / 2) * 2 - vec2<i32>(2, 0)));
    yuv = vec3<f32>(yuv.x, mix(yuv.yz, cs.yz, amount));
    // Posterize luma like a starved bitrate.
    let levels = mix(64.0, 18.0, amount);
    yuv.x = floor(yuv.x * levels + 0.5) / levels;
    return vec4<f32>(clamp(yuv2rgb(yuv), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

@fragment
fn fs_present(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let r = p.a;
    if (pos.x < r.x || pos.y < r.y || pos.x >= r.z || pos.y >= r.w) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let src_size = p.b.zw;
    let uv = (pos.xy - r.xy) / (r.zw - r.xy);
    let sp = uv * src_size;
    let ip = vec2<i32>(floor(sp));
    var c = load(ip);
    if (p.c.z > 0.5) {
        // Composite: chroma bleeds horizontally.
        let l = rgb2yuv(load(ip - vec2<i32>(1, 0)));
        let l2 = rgb2yuv(load(ip - vec2<i32>(2, 0)));
        var y = rgb2yuv(c);
        y = vec3<f32>(y.x, (y.yz + l.yz + l2.yz) / 3.0);
        c = yuv2rgb(y);
    }
    if (p.c.x > 0.5) {
        // Slight bloom from neighbours.
        let n = (load(ip + vec2<i32>(1, 0)) + load(ip - vec2<i32>(1, 0)) + load(ip + vec2<i32>(0, 1)) + load(ip - vec2<i32>(0, 1))) * 0.25;
        c = c + max(n - vec3<f32>(0.6), vec3<f32>(0.0)) * 0.6;
        // Scanlines in source space.
        let f = fract(sp.y);
        let scan = 1.0 - p.c.y * pow(abs(f - 0.5) * 2.0, 2.0);
        c = c * scan;
        // Aperture-grille shadow mask in output space.
        let m = i32(pos.x) % 3;
        var mask = vec3<f32>(0.85);
        if (m == 0) { mask.r = 1.08; } else if (m == 1) { mask.g = 1.08; } else { mask.b = 1.08; }
        c = c * mask;
    }
    return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
