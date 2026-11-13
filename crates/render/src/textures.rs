//! Procedural texture library (no texture packs needed): noise-based rust, concrete, wet asphalt,
//! kaiju hide and scales, etc. Each is generated at 64x64, quantized to 15-bit colour and
//! palettized to 16 colours - exactly the import rules for third-party textures.

use isl_assets::quantize;

pub const SIZE: u32 = 64;

pub const NAMES: &[&str] = &[
    "white",
    "plate",
    "grate",
    "steel",
    "gold",
    "rust",
    "glow",
    "scale",
    "belly",
    "hide",
    "fin",
    "bone",
    "fur",
    "chitin",
    "wing",
    "pale",
    "camo",
    "tread",
    "hazard",
    "concrete",
    "brick",
    "corrugated",
    "stone",
    "wood",
    "asphalt",
    "water",
    "sand",
    "particle",
    "spark",
    "smoke",
    "shadow",
    "window",
];

pub fn layer(name: &str) -> u32 {
    NAMES.iter().position(|n| *n == name).unwrap_or(1) as u32
}

fn hash(x: i32, y: i32, s: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ s.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0
}

/// Tileable value noise at a given cell period.
fn vnoise(x: f32, y: f32, period: i32, s: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    let w = |a: i32| a.rem_euclid(period);
    let sm = |t: f32| t * t * (3.0 - 2.0 * t);
    let a = hash(w(xi), w(yi), s);
    let b = hash(w(xi + 1), w(yi), s);
    let c = hash(w(xi), w(yi + 1), s);
    let d = hash(w(xi + 1), w(yi + 1), s);
    let u = sm(fx);
    let v = sm(fy);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f32, y: f32, s: u32) -> f32 {
    let mut t = 0.0;
    let mut amp = 0.5;
    let mut f = 4.0;
    for o in 0..4 {
        let p = f as i32;
        t += amp * vnoise(x * f / SIZE as f32, y * f / SIZE as f32, p, s + o);
        amp *= 0.5;
        f *= 2.0;
    }
    t
}

fn texel(name: &str, x: u32, y: u32) -> [f32; 4] {
    let (fx, fy) = (x as f32, y as f32);
    let n = fbm(fx, fy, 7);
    let g = |v: f32| [v, v, v, 1.0];
    match name {
        "white" => [1.0, 1.0, 1.0, 1.0],
        "plate" => {
            let seam = (x.is_multiple_of(32) || y.is_multiple_of(24)) as u8 as f32;
            let rivet = ((x % 32 == 3 || x % 32 == 28) && (y % 24 == 3 || y % 24 == 20)) as u8 as f32;
            g(0.62 + n * 0.25 - seam * 0.3 + rivet * 0.25)
        }
        "grate" => {
            let bar = (x % 8 < 2 || y % 8 < 2) as u8 as f32;
            g(0.25 + bar * 0.45 + n * 0.15)
        }
        "steel" => g(0.7 + (fy * 0.4).sin() * 0.05 + n * 0.2),
        "gold" => [0.95 + n * 0.1, 0.78 + n * 0.1, 0.4, 1.0],
        "rust" => {
            let r = fbm(fx * 1.7, fy * 1.7, 31);
            let pit = (r > 0.62) as u8 as f32;
            [0.55 + r * 0.35, 0.36 + r * 0.15 - pit * 0.1, 0.22 + n * 0.1, 1.0]
        }
        "glow" => g(0.92 + n * 0.08),
        "scale" => {
            // Overlapping scale rows.
            let row = (y / 8) as f32;
            let off = if (y / 8).is_multiple_of(2) { 0.0 } else { 4.0 };
            let lx = ((fx + off) % 8.0) - 4.0;
            let ly = (fy % 8.0) - 1.0;
            let d = (lx * lx + ly * ly).sqrt() / 6.0;
            let _ = row;
            g(0.45 + (1.0 - d).clamp(0.0, 1.0) * 0.45 + n * 0.1)
        }
        "belly" => g(0.75 + ((fy % 6.0) < 1.0) as u8 as f32 * -0.2 + n * 0.15),
        "hide" => {
            let c = fbm(fx * 2.0, fy * 2.0, 11);
            g(0.4 + c * 0.5)
        }
        "fin" => g(0.5 + ((fx * 0.6).sin() * 0.5 + 0.5) * 0.3 + n * 0.15),
        "bone" => [0.9 + n * 0.1, 0.86 + n * 0.08, 0.72, 1.0],
        "fur" => g(0.45 + hash(x as i32, y as i32 / 3, 3) * 0.4),
        "chitin" => {
            let band = ((fy / 10.0).sin() * 0.5 + 0.5) * 0.25;
            g(0.35 + band + n * 0.25)
        }
        "wing" => {
            // Rose-window veins.
            let v = ((fx * 0.4).sin() * (fy * 0.4).cos()).abs();
            g(0.55 + (v < 0.12) as u8 as f32 * -0.3 + n * 0.2)
        }
        "pale" => g(0.85 + n * 0.15),
        "camo" => {
            let c = fbm(fx * 1.3, fy * 1.3, 19);
            if c > 0.55 {
                [0.45, 0.48, 0.3, 1.0]
            } else if c > 0.42 {
                [0.6, 0.58, 0.42, 1.0]
            } else {
                [0.3, 0.32, 0.22, 1.0]
            }
        }
        "tread" => g(if (y % 8) < 3 { 0.25 } else { 0.55 } + n * 0.1),
        "hazard" => {
            let s = ((x + y) / 8).is_multiple_of(2);
            if s { [0.95, 0.8, 0.1, 1.0] } else { [0.1, 0.1, 0.1, 1.0] }
        }
        "concrete" => g(0.6 + n * 0.3 + (hash(x as i32, y as i32, 5) - 0.5) * 0.08),
        "brick" => {
            let row = y / 8;
            let off = if row.is_multiple_of(2) { 0 } else { 8 };
            let mortar = y.is_multiple_of(8) || (x + off).is_multiple_of(16);
            if mortar { [0.75, 0.72, 0.66, 1.0] } else { [0.62 + n * 0.25, 0.32 + n * 0.1, 0.25, 1.0] }
        }
        "corrugated" => g(0.5 + (fx * 0.8).sin() * 0.15 + n * 0.2),
        "stone" => {
            let blk = y.is_multiple_of(16) || (x + (y / 16) * 11).is_multiple_of(21);
            g(if blk { 0.4 } else { 0.68 + n * 0.25 })
        }
        "wood" => g(0.5 + ((fx + n * 8.0) * 0.5).sin() * 0.1 + n * 0.15),
        "asphalt" => {
            let wet = fbm(fx * 0.7, fy * 0.7, 41);
            let line = (x % 64 >= 30 && x % 64 < 33 && (y % 16) < 9) as u8 as f32;
            let v = 0.32 + hash(x as i32, y as i32, 9) * 0.12 + (wet > 0.6) as u8 as f32 * 0.12;
            [v + line * 0.5, v + line * 0.45, v + line * 0.1, 1.0]
        }
        "water" => {
            let w = fbm(fx, fy * 2.0, 23);
            [0.35 + w * 0.3, 0.55 + w * 0.3, 0.65 + w * 0.3, 1.0]
        }
        "sand" => g(0.7 + hash(x as i32, y as i32, 2) * 0.2),
        "particle" => {
            let d = ((fx - 31.5).powi(2) + (fy - 31.5).powi(2)).sqrt() / 32.0;
            [1.0, 1.0, 1.0, if d < 0.8 - hash(x as i32, y as i32, 4) * 0.25 { 1.0 } else { 0.0 }]
        }
        "spark" => {
            let d = (fx - 31.5).abs().min((fy - 31.5).abs());
            [1.0, 1.0, 0.9, if d < 3.0 && ((fx - 31.5).powi(2) + (fy - 31.5).powi(2)).sqrt() < 30.0 { 1.0 } else { 0.0 }]
        }
        "smoke" => {
            let d = ((fx - 31.5).powi(2) + (fy - 31.5).powi(2)).sqrt() / 32.0;
            let v = 0.6 + n * 0.4;
            [v, v, v, if d < 0.55 + n * 0.45 { 1.0 } else { 0.0 }]
        }
        "shadow" => {
            let d = ((fx - 31.5).powi(2) + (fy - 31.5).powi(2)).sqrt() / 32.0;
            [0.0, 0.0, 0.0, if d < 0.9 { 1.0 } else { 0.0 }]
        }
        "window" => {
            let lit = hash(x as i32 / 8, y as i32 / 10, 77) > 0.55;
            let frame = x % 8 < 2 || y % 10 < 3;
            if frame {
                [0.2, 0.2, 0.22, 1.0]
            } else if lit {
                [1.0, 0.85, 0.45, 1.0]
            } else {
                [0.15, 0.18, 0.25, 1.0]
            }
        }
        _ => g(0.7),
    }
}

/// All layers as RGBA8, each 64x64, 15-bit and palettized to 16 colours.
pub fn build_all() -> Vec<Vec<u8>> {
    NAMES
        .iter()
        .map(|name| {
            let mut img = Vec::with_capacity((SIZE * SIZE * 4) as usize);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let c = texel(name, x, y);
                    img.extend_from_slice(&[
                        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                    ]);
                }
            }
            quantize::palettize(SIZE, SIZE, &img, 16).to_rgba()
        })
        .collect()
}
