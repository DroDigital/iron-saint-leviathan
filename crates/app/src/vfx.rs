//! Code-generated VFX: sparks, spray, spore clouds, blood-mist, debris, smoke and shockwave rings.
//! No VFX packs; tiny generated sprites from the texture library.

use glam::Vec3;
use isl_render::{billboard, ground_quad, textures, DrawList};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Add,
    Alpha,
    Debris,
}

#[derive(Clone, Debug)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub life: f32,
    pub max: f32,
    pub size: f32,
    pub grow: f32,
    pub layer: u32,
    pub color: [u8; 4],
    pub gravity: f32,
    pub blend: Blend,
}

#[derive(Clone, Debug)]
pub struct Ring {
    pub pos: Vec3,
    pub r0: f32,
    pub r1: f32,
    pub life: f32,
    pub max: f32,
    pub color: [u8; 4],
    /// Telegraph rings stay visible and pulse until they expire.
    pub telegraph: bool,
}

pub struct Vfx {
    pub parts: Vec<Particle>,
    pub rings: Vec<Ring>,
    seed: u32,
    pub photosensitive: bool,
}

impl Default for Vfx {
    fn default() -> Self {
        Vfx { parts: Vec::with_capacity(2048), rings: vec![], seed: 99, photosensitive: false }
    }
}

impl Vfx {
    fn r(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 20000) as f32 / 10000.0 - 1.0
    }
    fn rv(&mut self) -> Vec3 {
        Vec3::new(self.r(), self.r(), self.r())
    }
    fn push(&mut self, p: Particle) {
        if self.parts.len() < 3000 {
            self.parts.push(p);
        }
    }
    pub fn sparks(&mut self, pos: Vec3, n: usize, color: [u8; 4]) {
        for _ in 0..n {
            let v = self.rv() * 14.0 + Vec3::Y * 6.0;
            let pp = Particle { pos, vel: v, life: 0.0, max: 0.35 + self.r().abs() * 0.3, size: 0.7, grow: -0.5, layer: textures::layer("spark"), color, gravity: 20.0, blend: Blend::Add };
            self.push(pp);
        }
    }
    pub fn mist(&mut self, pos: Vec3, n: usize, color: [u8; 4]) {
        for _ in 0..n {
            let v = self.rv() * 4.0;
            let pp = Particle { pos: pos + self.rv(), vel: v, life: 0.0, max: 0.9, size: 2.5, grow: 2.5, layer: textures::layer("smoke"), color, gravity: -0.5, blend: Blend::Alpha };
            self.push(pp);
        }
    }
    pub fn dust(&mut self, pos: Vec3, n: usize, scale: f32) {
        for _ in 0..n {
            let v = Vec3::new(self.r(), self.r().abs() * 0.4, self.r()) * 6.0 * scale;
            let pp = Particle { pos: pos + Vec3::new(self.r(), 0.0, self.r()) * 2.0 * scale, vel: v, life: 0.0, max: 1.4, size: 3.0 * scale, grow: 3.0, layer: textures::layer("smoke"), color: [150, 140, 120, 150], gravity: -0.3, blend: Blend::Alpha };
            self.push(pp);
        }
    }
    pub fn debris(&mut self, pos: Vec3, n: usize, color: [u8; 3], spread: f32) {
        for _ in 0..n {
            let v = Vec3::new(self.r() * 8.0, 6.0 + self.r().abs() * 10.0, self.r() * 8.0);
            let c = [color[0] / 2, color[1] / 2, color[2] / 2, 255];
            let pp = Particle { pos: pos + self.rv() * spread, vel: v, life: 0.0, max: 2.0, size: 1.0 + self.r().abs() * 1.5, grow: 0.0, layer: textures::layer("concrete"), color: c, gravity: 24.0, blend: Blend::Debris };
            self.push(pp);
        }
    }
    pub fn spores(&mut self, pos: Vec3, radius: f32) {
        for _ in 0..40 {
            let p = pos + Vec3::new(self.r(), self.r().abs() * 0.5, self.r()) * radius;
            let pp = Particle { pos: p, vel: self.rv() * 1.5, life: 0.0, max: 3.5, size: 2.0, grow: 0.6, layer: textures::layer("particle"), color: [200, 220, 120, 140], gravity: -0.4, blend: Blend::Alpha };
            self.push(pp);
        }
    }
    pub fn fire(&mut self, pos: Vec3, n: usize) {
        for _ in 0..n {
            let pp = Particle { pos: pos + self.rv() * 1.5, vel: Vec3::Y * 5.0 + self.rv() * 2.0, life: 0.0, max: 0.6, size: 2.0, grow: -1.5, layer: textures::layer("particle"), color: [255, 140, 40, 220], gravity: -2.0, blend: Blend::Add };
            self.push(pp);
        }
    }
    pub fn splash(&mut self, pos: Vec3, n: usize) {
        for _ in 0..n {
            let v = Vec3::new(self.r() * 5.0, 8.0 + self.r().abs() * 8.0, self.r() * 5.0);
            let pp = Particle { pos, vel: v, life: 0.0, max: 0.9, size: 1.2, grow: 0.5, layer: textures::layer("particle"), color: [200, 230, 255, 200], gravity: 22.0, blend: Blend::Alpha };
            self.push(pp);
        }
    }
    pub fn ring(&mut self, pos: Vec3, r0: f32, r1: f32, secs: f32, color: [u8; 4], telegraph: bool) {
        self.rings.push(Ring { pos, r0, r1, life: 0.0, max: secs, color, telegraph });
    }

    pub fn update(&mut self, dt: f32) {
        for p in self.parts.iter_mut() {
            p.life += dt;
            p.vel.y -= p.gravity * dt;
            p.pos += p.vel * dt;
            p.size = (p.size + p.grow * dt).max(0.05);
            if p.blend == Blend::Debris && p.pos.y < 0.0 {
                p.pos.y = 0.0;
                p.vel *= 0.4;
                p.vel.y = p.vel.y.abs() * 0.3;
            }
        }
        self.parts.retain(|p| p.life < p.max);
        for r in self.rings.iter_mut() {
            r.life += dt;
        }
        self.rings.retain(|r| r.life < r.max);
    }

    pub fn draw(&self, dl: &mut DrawList, right: Vec3, up: Vec3) {
        for p in &self.parts {
            let fade = 1.0 - p.life / p.max;
            match p.blend {
                Blend::Add => {
                    let mut c = p.color;
                    c[3] = (c[3] as f32 * fade) as u8;
                    billboard(&mut dl.add, p.pos, p.size, right, up, p.layer, c);
                }
                Blend::Alpha => {
                    let mut c = p.color;
                    c[3] = (c[3] as f32 * fade) as u8;
                    billboard(&mut dl.trans, p.pos, p.size, right, up, p.layer, c);
                }
                Blend::Debris => {
                    let s = p.size * 0.5;
                    let mut m = isl_render::meshgen::Mesh::default();
                    isl_render::meshgen::add_box(&mut m, p.pos - Vec3::splat(s), p.pos + Vec3::splat(s), [p.color[0], p.color[1], p.color[2]], p.layer as u16, 0.0, [true; 6], true);
                    for v in &m.verts {
                        dl.opaque.push(isl_render::Vertex { pos: v.pos.to_array(), uv: v.uv.to_array(), color: p.color, layer: p.layer });
                    }
                }
            }
        }
        for r in &self.rings {
            let k = r.life / r.max;
            let rad = r.r0 + (r.r1 - r.r0) * if r.telegraph { 1.0 } else { k };
            let pulse = if r.telegraph && !self.photosensitive { 0.6 + 0.4 * (r.life * 18.0).sin().abs() } else { 1.0 - k };
            let mut c = r.color;
            c[3] = (c[3] as f32 * pulse) as u8;
            let n = 24;
            for i in 0..n {
                let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
                let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
                let w = if r.telegraph { 1.2 } else { 1.8 };
                let p = |a: f32, rr: f32| r.pos + Vec3::new(a.sin() * rr, 0.25, a.cos() * rr);
                let (o0, o1, i0, i1) = (p(a0, rad + w), p(a1, rad + w), p(a0, (rad - w).max(0.0)), p(a1, (rad - w).max(0.0)));
                let lay = textures::layer("white") | isl_render::GLOW;
                let v = |q: Vec3| isl_render::Vertex { pos: q.to_array(), uv: [0.5, 0.5], color: c, layer: lay };
                dl.add.extend_from_slice(&[v(o0), v(o1), v(i1), v(o0), v(i1), v(i0)]);
            }
        }
    }

    /// Blob shadow (no real shadows on PSX).
    pub fn blob(dl: &mut DrawList, pos: Vec3, radius: f32, ground: f32) {
        ground_quad(&mut dl.trans, Vec3::new(pos.x, ground + 0.15, pos.z), radius, radius, textures::layer("shadow"), [0, 0, 0, 120]);
    }
}
