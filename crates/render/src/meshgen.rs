//! Procedural greybox geometry: primitives (box, cylinder, sphere, cone, spike, wedge, tube)
//! built from `isl_data::Prim`s, with large faces subdivided so affine warping stays readable.

use crate::textures;
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
use isl_data::{Prim, Shape};

#[derive(Clone, Copy, Debug)]
pub struct MeshVertex {
    pub pos: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub color: [u8; 3],
    pub layer: u16,
    pub glow: bool,
}

/// Non-indexed triangle list in local space.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub verts: Vec<MeshVertex>,
}

impl Mesh {
    pub fn tris(&self) -> usize {
        self.verts.len() / 3
    }
    pub fn append(&mut self, o: &Mesh, m: Mat4) {
        let nm = Mat3::from_mat4(m).inverse().transpose();
        for v in &o.verts {
            self.verts.push(MeshVertex { pos: m.transform_point3(v.pos), normal: (nm * v.normal).normalize_or_zero(), ..*v });
        }
    }
    fn tri(&mut self, a: (Vec3, Vec2), b: (Vec3, Vec2), c: (Vec3, Vec2), col: [u8; 3], layer: u16, glow: bool) {
        let n = (b.0 - a.0).cross(c.0 - a.0).normalize_or_zero();
        for (p, uv) in [a, b, c] {
            self.verts.push(MeshVertex { pos: p, normal: n, uv, color: col, layer, glow });
        }
    }
    fn tri_n(&mut self, a: (Vec3, Vec3, Vec2), b: (Vec3, Vec3, Vec2), c: (Vec3, Vec3, Vec2), col: [u8; 3], layer: u16, glow: bool) {
        for (p, n, uv) in [a, b, c] {
            self.verts.push(MeshVertex { pos: p, normal: n, uv, color: col, layer, glow });
        }
    }
    /// A quad subdivided into a grid so large faces don't over-warp.
    #[allow(clippy::too_many_arguments)]
    pub fn quad(&mut self, o: Vec3, u: Vec3, v: Vec3, uvs: Vec2, col: [u8; 3], layer: u16, glow: bool, max_cell: f32) {
        let nu = ((u.length() / max_cell).ceil() as usize).clamp(1, 12);
        let nv = ((v.length() / max_cell).ceil() as usize).clamp(1, 12);
        for i in 0..nu {
            for j in 0..nv {
                let (a0, a1) = (i as f32 / nu as f32, (i + 1) as f32 / nu as f32);
                let (b0, b1) = (j as f32 / nv as f32, (j + 1) as f32 / nv as f32);
                let p = |a: f32, b: f32| (o + u * a + v * b, Vec2::new(a * uvs.x, b * uvs.y));
                self.tri(p(a0, b0), p(a1, b0), p(a1, b1), col, layer, glow);
                self.tri(p(a0, b0), p(a1, b1), p(a0, b1), col, layer, glow);
            }
        }
    }
}

/// Axis-aligned box from min/max with per-face tiling (metres per texture repeat).
pub fn add_box(m: &mut Mesh, mn: Vec3, mx: Vec3, col: [u8; 3], layer: u16, tile: f32, faces: [bool; 6], glow: bool) {
    let s = mx - mn;
    let t = |a: f32| if tile > 0.0 { (a / tile).max(1.0).round() } else { 1.0 };
    let cell = 8.0;
    // +X, -X, +Y, -Y, +Z, -Z
    if faces[0] {
        m.quad(
            Vec3::new(mx.x, mn.y, mx.z),
            Vec3::new(0.0, 0.0, -s.z),
            Vec3::new(0.0, s.y, 0.0),
            Vec2::new(t(s.z), t(s.y)),
            col,
            layer,
            glow,
            cell,
        );
    }
    if faces[1] {
        m.quad(
            Vec3::new(mn.x, mn.y, mn.z),
            Vec3::new(0.0, 0.0, s.z),
            Vec3::new(0.0, s.y, 0.0),
            Vec2::new(t(s.z), t(s.y)),
            col,
            layer,
            glow,
            cell,
        );
    }
    if faces[2] {
        m.quad(
            Vec3::new(mn.x, mx.y, mn.z),
            Vec3::new(s.x, 0.0, 0.0),
            Vec3::new(0.0, 0.0, s.z),
            Vec2::new(t(s.x), t(s.z)),
            col,
            layer,
            glow,
            cell,
        );
    }
    if faces[3] {
        m.quad(
            Vec3::new(mn.x, mn.y, mx.z),
            Vec3::new(s.x, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -s.z),
            Vec2::new(t(s.x), t(s.z)),
            col,
            layer,
            glow,
            cell,
        );
    }
    if faces[4] {
        m.quad(
            Vec3::new(mn.x, mn.y, mx.z),
            Vec3::new(s.x, 0.0, 0.0),
            Vec3::new(0.0, s.y, 0.0),
            Vec2::new(t(s.x), t(s.y)),
            col,
            layer,
            glow,
            cell,
        );
    }
    if faces[5] {
        m.quad(
            Vec3::new(mx.x, mn.y, mn.z),
            Vec3::new(-s.x, 0.0, 0.0),
            Vec3::new(0.0, s.y, 0.0),
            Vec2::new(t(s.x), t(s.y)),
            col,
            layer,
            glow,
            cell,
        );
    }
}

fn ring(m: &mut Mesh, sides: usize, r0: Vec2, r1: Vec2, y0: f32, y1: f32, col: [u8; 3], layer: u16, glow: bool, caps: bool) {
    let p = |i: usize, r: Vec2, y: f32| {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        Vec3::new(a.cos() * r.x, y, a.sin() * r.y)
    };
    for i in 0..sides {
        let (a, b) = (p(i, r0, y0), p(i + 1, r0, y0));
        let (c, d) = (p(i, r1, y1), p(i + 1, r1, y1));
        let na = Vec3::new(a.x, 0.0, a.z).normalize_or_zero();
        let nb = Vec3::new(b.x, 0.0, b.z).normalize_or_zero();
        let u0 = i as f32 / sides as f32;
        let u1 = (i + 1) as f32 / sides as f32;
        m.tri_n((a, na, Vec2::new(u0, 1.0)), (c, na, Vec2::new(u0, 0.0)), (d, nb, Vec2::new(u1, 0.0)), col, layer, glow);
        m.tri_n((a, na, Vec2::new(u0, 1.0)), (d, nb, Vec2::new(u1, 0.0)), (b, nb, Vec2::new(u1, 1.0)), col, layer, glow);
        if caps {
            if r1.x > 0.0 {
                m.tri((Vec3::new(0.0, y1, 0.0), Vec2::splat(0.5)), (d, Vec2::new(1.0, 0.0)), (c, Vec2::new(0.0, 0.0)), col, layer, glow);
            }
            if r0.x > 0.0 {
                m.tri((Vec3::new(0.0, y0, 0.0), Vec2::splat(0.5)), (a, Vec2::new(0.0, 1.0)), (b, Vec2::new(1.0, 1.0)), col, layer, glow);
            }
        }
    }
}

/// Build one primitive in prim-local space (centred on `at`, rotated by `rot`).
pub fn prim_mesh(p: &Prim) -> Mesh {
    let mut m = Mesh::default();
    let s = Vec3::from(p.size);
    let h = s * 0.5;
    let layer = textures::layer(&p.tex) as u16;
    let col = p.col;
    let g = p.glow;
    match p.shape {
        Shape::Box => add_box(&mut m, -h, h, col, layer, 0.0, [true; 6], g),
        Shape::Cylinder => ring(&mut m, 8, Vec2::new(h.x, h.z), Vec2::new(h.x, h.z), -h.y, h.y, col, layer, g, true),
        Shape::Tube => ring(&mut m, 6, Vec2::new(h.x, h.z), Vec2::new(h.x * 0.7, h.z * 0.7), -h.y, h.y, col, layer, g, true),
        Shape::Cone => ring(&mut m, 8, Vec2::new(h.x, h.z), Vec2::ZERO, -h.y, h.y, col, layer, g, true),
        Shape::Spike => ring(&mut m, 4, Vec2::new(h.x, h.z), Vec2::ZERO, -h.y, h.y, col, layer, g, true),
        Shape::Sphere => {
            let (st, sl) = (4, 6);
            for i in 0..st {
                let t0 = i as f32 / st as f32 * std::f32::consts::PI;
                let t1 = (i + 1) as f32 / st as f32 * std::f32::consts::PI;
                let y0 = -t0.cos();
                let y1 = -t1.cos();
                let r0 = t0.sin();
                let r1 = t1.sin();
                for j in 0..sl {
                    let a0 = j as f32 / sl as f32 * std::f32::consts::TAU;
                    let a1 = (j + 1) as f32 / sl as f32 * std::f32::consts::TAU;
                    let q = |r: f32, y: f32, a: f32| Vec3::new(a.cos() * r, y, a.sin() * r);
                    let (n00, n01, n10, n11) = (q(r0, y0, a0), q(r0, y0, a1), q(r1, y1, a0), q(r1, y1, a1));
                    let sc = |n: Vec3| n * h;
                    let uv = |a: f32, t: f32| Vec2::new(a / std::f32::consts::TAU, t / std::f32::consts::PI);
                    m.tri_n((sc(n00), n00, uv(a0, t0)), (sc(n10), n10, uv(a0, t1)), (sc(n11), n11, uv(a1, t1)), col, layer, g);
                    m.tri_n((sc(n00), n00, uv(a0, t0)), (sc(n11), n11, uv(a1, t1)), (sc(n01), n01, uv(a1, t0)), col, layer, g);
                }
            }
        }
        Shape::Wedge => {
            // Triangular prism: full at -z, edge at +z top.
            let a = Vec3::new(-h.x, -h.y, -h.z);
            let b = Vec3::new(h.x, -h.y, -h.z);
            let c = Vec3::new(h.x, h.y, -h.z);
            let d = Vec3::new(-h.x, h.y, -h.z);
            let e = Vec3::new(-h.x, -h.y, h.z);
            let f = Vec3::new(h.x, -h.y, h.z);
            let uv = |x: f32, y: f32| Vec2::new(x, y);
            m.tri((a, uv(0., 1.)), (b, uv(1., 1.)), (c, uv(1., 0.)), col, layer, g);
            m.tri((a, uv(0., 1.)), (c, uv(1., 0.)), (d, uv(0., 0.)), col, layer, g);
            m.tri((d, uv(0., 0.)), (c, uv(1., 0.)), (f, uv(1., 1.)), col, layer, g);
            m.tri((d, uv(0., 0.)), (f, uv(1., 1.)), (e, uv(0., 1.)), col, layer, g);
            m.tri((a, uv(0., 0.)), (e, uv(0., 1.)), (f, uv(1., 1.)), col, layer, g);
            m.tri((a, uv(0., 0.)), (f, uv(1., 1.)), (b, uv(1., 0.)), col, layer, g);
            m.tri((a, uv(0., 0.)), (d, uv(0., 1.)), (e, uv(1., 0.)), col, layer, g);
            m.tri((b, uv(0., 0.)), (f, uv(1., 0.)), (c, uv(0., 1.)), col, layer, g);
        }
    }
    let r = Vec3::from(p.rot) * (std::f32::consts::PI / 180.0);
    let tr = Mat4::from_rotation_translation(Quat::from_euler(glam::EulerRot::XYZ, r.x, r.y, r.z), Vec3::from(p.at));
    let mut out = Mesh::default();
    out.append(&m, tr);
    out
}

/// Mesh for a whole part (all prims), in socket space.
pub fn part_mesh(prims: &[Prim]) -> Mesh {
    let mut m = Mesh::default();
    for p in prims {
        m.append(&prim_mesh(p), Mat4::IDENTITY);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primitives_have_triangles() {
        for s in [Shape::Box, Shape::Cylinder, Shape::Sphere, Shape::Cone, Shape::Spike, Shape::Wedge, Shape::Tube] {
            let p = Prim { shape: s, at: [0.0; 3], size: [1.0; 3], rot: [0.0; 3], tex: "plate".into(), col: [200; 3], glow: false };
            let m = prim_mesh(&p);
            assert!(m.tris() >= 4 && m.verts.len() % 3 == 0, "{s:?}");
        }
    }
    #[test]
    fn large_faces_subdivide() {
        let mut m = Mesh::default();
        add_box(&mut m, Vec3::ZERO, Vec3::new(40.0, 40.0, 40.0), [1; 3], 0, 4.0, [true; 6], false);
        assert!(m.tris() > 12 * 4);
    }
}
