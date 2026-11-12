//! PSX texture quantization: 15-bit colour (5:5:5) then palettize to N colours with a median-cut
//! palette, deterministic. Textures larger than 256x256 are rejected/downscaled by the caller.

#[derive(Clone, Debug, PartialEq)]
pub struct Palettized {
    pub w: u32,
    pub h: u32,
    pub palette: Vec<[u8; 4]>,
    pub indices: Vec<u8>,
}

impl Palettized {
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.indices.len() * 4);
        for &i in &self.indices {
            out.extend_from_slice(&self.palette[i as usize]);
        }
        out
    }
}

/// Snap an 8-bit channel to 5 bits and back (PSX 15-bit colour).
pub fn to15(c: u8) -> u8 {
    let v = (c as u32 * 31 + 127) / 255;
    ((v * 255 + 15) / 31) as u8
}

pub fn quantize15(rgba: &mut [u8]) {
    for px in rgba.chunks_mut(4) {
        px[0] = to15(px[0]);
        px[1] = to15(px[1]);
        px[2] = to15(px[2]);
        px[3] = if px[3] >= 128 { 255 } else { 0 };
    }
}

/// Median-cut palettization to at most `colors` entries (16 or 256).
pub fn palettize(w: u32, h: u32, rgba: &[u8], colors: usize) -> Palettized {
    let mut q = rgba.to_vec();
    quantize15(&mut q);
    let px: Vec<[u8; 4]> = q.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    let mut boxes: Vec<Vec<[u8; 4]>> = vec![px.iter().filter(|p| p[3] > 0).copied().collect()];
    let transparent = px.iter().any(|p| p[3] == 0);
    let target = if transparent { colors - 1 } else { colors };
    while boxes.len() < target {
        // Split the box with the widest channel range.
        let (bi, ch, range) = boxes
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let mut best = (0, 0);
                for c in 0..3 {
                    let mn = b.iter().map(|p| p[c]).min().unwrap_or(0);
                    let mx = b.iter().map(|p| p[c]).max().unwrap_or(0);
                    if (mx - mn) as i32 > best.1 {
                        best = (c, (mx - mn) as i32);
                    }
                }
                (i, best.0, best.1)
            })
            .max_by_key(|x| (x.2, std::cmp::Reverse(x.0)))
            .unwrap_or((0, 0, 0));
        if range == 0 || boxes[bi].len() < 2 {
            break;
        }
        let mut b = boxes.swap_remove(bi);
        b.sort_by_key(|p| (p[ch], p[0], p[1], p[2]));
        let hi = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(hi);
    }
    let mut palette: Vec<[u8; 4]> = boxes
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let n = b.len() as u32;
            let s = b.iter().fold([0u32; 3], |a, p| [a[0] + p[0] as u32, a[1] + p[1] as u32, a[2] + p[2] as u32]);
            [to15((s[0] / n) as u8), to15((s[1] / n) as u8), to15((s[2] / n) as u8), 255]
        })
        .collect();
    if transparent || palette.is_empty() {
        palette.insert(0, [0, 0, 0, 0]);
    }
    let indices = px
        .iter()
        .map(|p| {
            if p[3] == 0 {
                return 0;
            }
            palette
                .iter()
                .enumerate()
                .filter(|(_, c)| c[3] > 0)
                .min_by_key(|(_, c)| {
                    let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
                    d(c[0], p[0]) * 3 + d(c[1], p[1]) * 4 + d(c[2], p[2]) * 2
                })
                .map_or(0, |(i, _)| i as u8)
        })
        .collect();
    Palettized { w, h, palette, indices }
}

/// Nearest-neighbour downscale so neither side exceeds `max` (PSX textures are <= 256x256).
pub fn downscale(w: u32, h: u32, rgba: &[u8], max: u32) -> (u32, u32, Vec<u8>) {
    if w <= max && h <= max {
        return (w, h, rgba.to_vec());
    }
    let k = (w.max(h) as f32 / max as f32).ceil() as u32;
    let (nw, nh) = (w / k, h / k);
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            let i = (((y * k) * w + x * k) * 4) as usize;
            out.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    (nw, nh, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifteen_bit() {
        for c in 0..=255u8 {
            let q = to15(c);
            assert_eq!(to15(q), q, "idempotent");
            assert!((q as i32 - c as i32).abs() <= 5);
        }
    }
    #[test]
    fn palette_size_respected() {
        let mut img = vec![];
        for i in 0..64 * 64u32 {
            img.extend_from_slice(&[(i % 251) as u8, (i * 7 % 253) as u8, (i * 13 % 255) as u8, 255]);
        }
        let p = palettize(64, 64, &img, 16);
        assert!(p.palette.len() <= 16);
        assert_eq!(p.indices.len(), 64 * 64);
        assert!(p.palette.iter().all(|c| to15(c[0]) == c[0]));
        let p2 = palettize(64, 64, &img, 16);
        assert_eq!(p, p2, "deterministic");
    }
}
