struct U { res: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var font: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VIn { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> };
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };

@vertex
fn vs(v: VIn) -> VOut {
    var o: VOut;
    let x = (v.pos.x + u.res.z) / u.res.x * 2.0 - 1.0;
    let y = 1.0 - (v.pos.y + u.res.w) / u.res.y * 2.0;
    o.clip = vec4<f32>(x, y, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(font, samp, i.uv);
    if (t.a < 0.5) {
        discard;
    }
    let c = floor(clamp(t.rgb * i.color.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 31.0 + 0.5) / 31.0;
    return vec4<f32>(c, i.color.a);
}
