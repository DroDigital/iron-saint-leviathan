//! Static district geometry (pre-lit, uploaded once and rebuilt when chunks fall) plus the
//! animated water surface.

use glam::{Vec2, Vec3};
use isl_data::{DistrictDef, DistrictState};
use isl_render::meshgen::{self, Mesh};
use isl_render::{Lighting, Vertex, textures};
use isl_sim::city::{BuildingKind, CheckpointKind, City};
use isl_sim::tide::TideClock;

fn tl(n: &str) -> u16 {
    textures::layer(n) as u16
}

/// District lighting for a state and time of day.
pub fn lighting(d: &DistrictDef, state: DistrictState, tide: &TideClock, city: &City) -> (Lighting, [f32; 3], f32, f32) {
    let sv = &d.states[state.index()];
    let day = tide.daylight();
    let c = |v: [u8; 3]| Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 255.0;
    let night_tint = Vec3::new(0.35, 0.42, 0.7);
    let amb = c(sv.ambient) * (0.6 + day * 0.9) * if day < 0.25 { night_tint * 1.6 } else { Vec3::ONE };
    let sun = c(sv.sun) * day * 1.15;
    let sun_dir = Vec3::new(-0.45, -1.0, -0.35 + (tide.hour_of_day() as f32 - 13.0) * 0.06).normalize();
    let mut l = Lighting { ambient: amb + Vec3::splat(0.12), dirs: vec![(sun_dir, sun)], points: vec![] };
    l.dirs.push((Vec3::new(0.6, -0.3, 0.7).normalize(), amb * 0.4));
    if !tide.blackout() {
        for p in &city.lamps {
            l.points.push((*p, Vec3::new(1.0, 0.8, 0.45) * (1.3 - day), 22.0));
        }
    }
    for f in &city.fires {
        l.points.push((*f, Vec3::new(1.0, 0.45, 0.15) * 1.4, 34.0));
    }
    let fog = c(sv.fog) * (0.35 + day * 0.65);
    let fog_c = [fog.x, fog.y, fog.z];
    (l, fog_c, sv.fog_near, sv.fog_far)
}

pub fn build(city: &City, d: &DistrictDef, light: &Lighting) -> Vec<Vertex> {
    let mut m = Mesh::default();
    let half = city.half();
    // Ground tiles (streets on the block grid, sea floor below the shore).
    let tile = 18.0;
    let mut z = -half - 40.0;
    while z < half {
        let mut x = -half;
        while x < half {
            let c = Vec2::new(x + tile * 0.5, z + tile * 0.5);
            let y00 = city.ground(x, z);
            let y11 = city.ground(x + tile, z + tile);
            let sea = c.y < city.shore_z;
            let street = !sea && (((c.x + half) / d.block).fract() < 0.22 || ((c.y - city.shore_z) / d.block).fract() < 0.22);
            let (tex, col) = if sea {
                ("sand", [120, 120, 100])
            } else if street {
                ("asphalt", [150, 150, 150])
            } else {
                ("concrete", [d.palette[0][0], d.palette[0][1], d.palette[0][2]])
            };
            let o = Vec3::new(x, y00, z);
            let u = Vec3::new(tile, 0.0, 0.0);
            let v = Vec3::new(0.0, y11 - y00, tile);
            m.quad(o, v, u, Vec2::new(2.0, 2.0), col, tl(tex), false, 9.0);
            x += tile;
        }
        z += tile;
    }
    // Buildings: emit only exposed chunk faces.
    for b in &city.buildings {
        let tex = tl(b.kind.tex());
        let col = b.color;
        for i in 0..b.alive.len() {
            if !b.alive[i] {
                continue;
            }
            let (x, y, zz) = b.coords(i);
            let dead = |dx: i32, dy: i32, dz: i32| {
                let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, zz as i32 + dz);
                if nx < 0 || ny < 0 || nz < 0 || nx >= b.nx as i32 || ny >= b.ny as i32 || nz >= b.nz as i32 {
                    return true;
                }
                !b.alive[b.idx(nx as u8, ny as u8, nz as u8)]
            };
            let faces = [dead(1, 0, 0), dead(-1, 0, 0), dead(0, 1, 0), false, dead(0, 0, 1), dead(0, 0, -1)];
            let a = b.chunk_aabb(i);
            let wall_tex = if matches!(b.kind, BuildingKind::House | BuildingKind::Tower) && y > 0 { tl("window") } else { tex };
            meshgen::add_box(&mut m, a.min, a.max, col, wall_tex, 4.0, faces, false);
        }
        // Landmark decoration on intact structures.
        if b.standing() {
            let top = b.aabb.max.y;
            let c = b.aabb.center();
            match b.kind {
                BuildingKind::Tower if city.landmark == isl_data::Landmark::Cathedral => {
                    let p = isl_data::Prim {
                        shape: isl_data::Shape::Cone,
                        at: [c.x, top + 7.0, c.z],
                        size: [8.0, 14.0, 8.0],
                        rot: [0.0; 3],
                        tex: "stone".into(),
                        col: [120, 110, 110],
                        glow: false,
                    };
                    m.append(&meshgen::prim_mesh(&p), glam::Mat4::IDENTITY);
                }
                BuildingKind::Lighthouse => {
                    meshgen::add_box(
                        &mut m,
                        Vec3::new(c.x - 4.0, top, c.z - 4.0),
                        Vec3::new(c.x + 4.0, top + 4.0, c.z + 4.0),
                        [255, 240, 180],
                        tl("glow"),
                        0.0,
                        [true; 6],
                        true,
                    );
                }
                BuildingKind::Stack => {
                    meshgen::add_box(
                        &mut m,
                        Vec3::new(c.x - 3.0, top, c.z - 3.0),
                        Vec3::new(c.x + 3.0, top + 1.0, c.z + 3.0),
                        [255, 120, 40],
                        tl("glow"),
                        0.0,
                        [true; 6],
                        true,
                    );
                }
                BuildingKind::Church => {
                    let p = isl_data::Prim {
                        shape: isl_data::Shape::Wedge,
                        at: [c.x, top + 4.0, c.z],
                        size: [b.aabb.max.x - b.aabb.min.x, 8.0, b.aabb.max.z - b.aabb.min.z],
                        rot: [-90.0, 0.0, 0.0],
                        tex: "stone".into(),
                        col: [100, 80, 80],
                        glow: false,
                    };
                    m.append(&meshgen::prim_mesh(&p), glam::Mat4::IDENTITY);
                }
                _ => {}
            }
        }
    }
    // Checkpoints.
    for cp in &city.checkpoints {
        let p = cp.pos;
        match cp.kind {
            CheckpointKind::HangarBay => {
                meshgen::add_box(
                    &mut m,
                    p + Vec3::new(-9.0, 0.0, -1.0),
                    p + Vec3::new(9.0, 12.0, 8.0),
                    [110, 110, 120],
                    tl("corrugated"),
                    4.0,
                    [true, true, true, false, true, false],
                    false,
                );
                meshgen::add_box(
                    &mut m,
                    p + Vec3::new(-9.0, 0.0, -1.5),
                    p + Vec3::new(9.0, 1.0, -1.0),
                    [255, 255, 255],
                    tl("hazard"),
                    2.0,
                    [true; 6],
                    false,
                );
                meshgen::add_box(
                    &mut m,
                    p + Vec3::new(-3.0, 12.5, 3.0),
                    p + Vec3::new(3.0, 14.0, 3.5),
                    [120, 200, 255],
                    tl("glow"),
                    0.0,
                    [true; 6],
                    true,
                );
            }
            CheckpointKind::DeepNest => {
                for k in 0..7 {
                    let a = k as f32 / 7.0 * std::f32::consts::TAU;
                    let pr = isl_data::Prim {
                        shape: isl_data::Shape::Spike,
                        at: [p.x + a.sin() * 7.0, p.y + 4.0, p.z + a.cos() * 7.0],
                        size: [1.4, 9.0, 1.4],
                        rot: [a.cos() * -25.0, 0.0, a.sin() * 25.0],
                        tex: "bone".into(),
                        col: [200, 190, 170],
                        glow: false,
                    };
                    m.append(&meshgen::prim_mesh(&pr), glam::Mat4::IDENTITY);
                }
                let g = isl_data::Prim {
                    shape: isl_data::Shape::Sphere,
                    at: [p.x, p.y + 1.0, p.z],
                    size: [5.0, 2.5, 5.0],
                    rot: [0.0; 3],
                    tex: "hide".into(),
                    col: [120, 255, 220],
                    glow: true,
                };
                m.append(&meshgen::prim_mesh(&g), glam::Mat4::IDENTITY);
            }
        }
    }
    isl_render::bake(&m, light)
}

/// Animated water surface over the sea and any flooded land.
pub fn water(out: &mut Vec<Vertex>, city: &City, level: f32, t: f32, cam: Vec3, tint: [u8; 3], flash: bool) {
    let half = city.half();
    let cell = 12.0;
    let r = 150.0;
    let z0 = (cam.z - r).max(-half - 80.0);
    let z1 = (cam.z + r).min(half);
    let x0 = (cam.x - r).max(-half - 20.0);
    let x1 = (cam.x + r).min(half + 20.0);
    let layer = textures::layer("water");
    let h = |x: f32, z: f32| level + (x * 0.08 + t * 1.1).sin() * 0.25 + (z * 0.11 - t * 0.9).cos() * 0.25;
    let a = if flash { 220 } else { 170 };
    let col = [tint[0], tint[1], tint[2], a];
    let mut z = (z0 / cell).floor() * cell;
    while z < z1 {
        let mut x = (x0 / cell).floor() * cell;
        while x < x1 {
            // Skip cells where the ground is above the water everywhere.
            let dry = city.ground(x, z) > level + 0.3 && city.ground(x, z + cell) > level + 0.3;
            if !dry {
                let p = |px: f32, pz: f32| Vertex { pos: [px, h(px, pz), pz], uv: [px / 16.0 + t * 0.02, pz / 16.0], color: col, layer };
                let (a0, b0, c0, d0) = (p(x, z), p(x + cell, z), p(x + cell, z + cell), p(x, z + cell));
                out.extend_from_slice(&[a0, b0, c0, a0, c0, d0]);
            }
            x += cell;
        }
        z += cell;
    }
}
