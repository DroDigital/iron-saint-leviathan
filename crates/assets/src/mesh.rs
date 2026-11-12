//! Compact mesh format (`ISLMSH1`): positions (f32x3), normals (i8x3), uvs (u16x2 normalized),
//! colours (u8x4), u16 indices. Produced by `xtask import` from glTF, after large-polygon
//! subdivision so the affine warp stays readable.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[u8; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn tri_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Split triangles whose longest edge exceeds `max_edge` (keeps affine warping charming).
    pub fn subdivide(&mut self, max_edge: f32) {
        for _ in 0..6 {
            let mut changed = false;
            let mut out = Vec::with_capacity(self.indices.len());
            for t in self.indices.clone().chunks(3) {
                let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                let d = |i: usize, j: usize| {
                    let p = self.positions[i];
                    let q = self.positions[j];
                    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
                };
                let e = [(d(a, b), a, b, c), (d(b, c), b, c, a), (d(c, a), c, a, b)];
                let (len, i, j, k) = e.iter().copied().fold((0.0, 0, 0, 0), |m, x| if x.0 > m.0 { x } else { m });
                if len > max_edge {
                    let m = self.midpoint(i, j);
                    out.extend_from_slice(&[i as u32, m, k as u32, m, j as u32, k as u32]);
                    changed = true;
                } else {
                    out.extend_from_slice(t);
                }
            }
            self.indices = out;
            if !changed {
                break;
            }
        }
    }

    fn midpoint(&mut self, i: usize, j: usize) -> u32 {
        let lerp3 = |a: [f32; 3], b: [f32; 3]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5];
        self.positions.push(lerp3(self.positions[i], self.positions[j]));
        if !self.normals.is_empty() {
            let n = lerp3(self.normals[i], self.normals[j]);
            self.normals.push(n);
        }
        if !self.uvs.is_empty() {
            let (a, b) = (self.uvs[i], self.uvs[j]);
            self.uvs.push([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
        }
        if !self.colors.is_empty() {
            let (a, b) = (self.colors[i], self.colors[j]);
            self.colors.push([((a[0] as u16 + b[0] as u16) / 2) as u8, ((a[1] as u16 + b[1] as u16) / 2) as u8, ((a[2] as u16 + b[2] as u16) / 2) as u8, a[3]]);
        }
        (self.positions.len() - 1) as u32
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut o = b"ISLMSH1\0".to_vec();
        o.extend_from_slice(&(self.positions.len() as u32).to_le_bytes());
        o.extend_from_slice(&(self.indices.len() as u32).to_le_bytes());
        for (i, p) in self.positions.iter().enumerate() {
            for v in p {
                o.extend_from_slice(&v.to_le_bytes());
            }
            let n = self.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            for v in n {
                o.push((v.clamp(-1.0, 1.0) * 127.0) as i8 as u8);
            }
            let uv = self.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            for v in uv {
                o.extend_from_slice(&((v.rem_euclid(1.0001) * 65535.0) as u16).to_le_bytes());
            }
            o.extend_from_slice(&self.colors.get(i).copied().unwrap_or([255; 4]));
        }
        for i in &self.indices {
            o.extend_from_slice(&(*i as u16).to_le_bytes());
        }
        o
    }

    pub fn decode(b: &[u8]) -> Option<MeshData> {
        if b.len() < 16 || &b[..8] != b"ISLMSH1\0" {
            return None;
        }
        let nv = u32::from_le_bytes(b[8..12].try_into().ok()?) as usize;
        let ni = u32::from_le_bytes(b[12..16].try_into().ok()?) as usize;
        let mut m = MeshData::default();
        let mut i = 16;
        for _ in 0..nv {
            let f = |k: usize| f32::from_le_bytes(b[k..k + 4].try_into().unwrap());
            m.positions.push([f(i), f(i + 4), f(i + 8)]);
            m.normals.push([b[i + 12] as i8 as f32 / 127.0, b[i + 13] as i8 as f32 / 127.0, b[i + 14] as i8 as f32 / 127.0]);
            let u = u16::from_le_bytes([b[i + 15], b[i + 16]]) as f32 / 65535.0;
            let v = u16::from_le_bytes([b[i + 17], b[i + 18]]) as f32 / 65535.0;
            m.uvs.push([u, v]);
            m.colors.push([b[i + 19], b[i + 20], b[i + 21], b[i + 22]]);
            i += 23;
        }
        for _ in 0..ni {
            m.indices.push(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32);
            i += 2;
        }
        Some(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subdivide_and_roundtrip() {
        let mut m = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            colors: vec![[255; 4]; 3],
            indices: vec![0, 1, 2],
        };
        m.subdivide(4.0);
        assert!(m.tri_count() > 4);
        let back = MeshData::decode(&m.encode()).unwrap();
        assert_eq!(back.indices, m.indices);
        assert_eq!(back.positions, m.positions);
    }
}
