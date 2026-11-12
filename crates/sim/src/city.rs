//! Destructible Harrowmouth: procedural district layouts, pre-fractured building chunks,
//! checkpoints, landmarks, water and persistent damage.

use crate::geom::Aabb;
use crate::rng::SimRng;
use glam::{Vec2, Vec3};
use isl_data::*;
use serde::{Deserialize, Serialize};

pub const CHUNK_W: f32 = 8.0;
pub const CHUNK_H: f32 = 6.0;
pub const SEA_FLOOR: f32 = -9.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingKind {
    House,
    Warehouse,
    Tower,
    Church,
    Stack,
    Stall,
    Wall,
    Crane,
    Lighthouse,
    Barricade,
    Railcar,
    Pylon,
    Pier,
}

impl BuildingKind {
    pub fn tex(self) -> &'static str {
        match self {
            BuildingKind::House => "brick",
            BuildingKind::Warehouse => "corrugated",
            BuildingKind::Tower => "concrete",
            BuildingKind::Church => "stone",
            BuildingKind::Stack => "brick",
            BuildingKind::Stall => "wood",
            BuildingKind::Wall => "concrete",
            BuildingKind::Crane => "rust",
            BuildingKind::Lighthouse => "stone",
            BuildingKind::Barricade => "hazard",
            BuildingKind::Railcar => "rust",
            BuildingKind::Pylon => "grate",
            BuildingKind::Pier => "wood",
        }
    }
    /// Hit points per chunk.
    pub fn chunk_hp(self) -> f32 {
        match self {
            BuildingKind::Stall | BuildingKind::Pier => 20.0,
            BuildingKind::House | BuildingKind::Railcar => 45.0,
            BuildingKind::Warehouse | BuildingKind::Barricade | BuildingKind::Crane | BuildingKind::Pylon => 70.0,
            BuildingKind::Tower | BuildingKind::Stack => 90.0,
            BuildingKind::Church | BuildingKind::Lighthouse | BuildingKind::Wall => 140.0,
        }
    }
    /// Is this structure lit at night (lure for Goliath Moth)?
    pub fn lit(self) -> bool {
        matches!(self, BuildingKind::Lighthouse | BuildingKind::Stack | BuildingKind::Church | BuildingKind::Tower)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Building {
    pub kind: BuildingKind,
    pub aabb: Aabb,
    pub nx: u8,
    pub ny: u8,
    pub nz: u8,
    pub alive: Vec<bool>,
    pub chunk_hp: Vec<f32>,
    pub color: [u8; 3],
}

impl Building {
    pub fn new(kind: BuildingKind, aabb: Aabb, color: [u8; 3]) -> Self {
        let s = aabb.max - aabb.min;
        let nx = ((s.x / CHUNK_W).ceil() as u8).clamp(1, 6);
        let ny = ((s.y / CHUNK_H).ceil() as u8).clamp(1, 10);
        let nz = ((s.z / CHUNK_W).ceil() as u8).clamp(1, 6);
        let n = nx as usize * ny as usize * nz as usize;
        Building { kind, aabb, nx, ny, nz, alive: vec![true; n], chunk_hp: vec![kind.chunk_hp(); n], color }
    }
    pub fn idx(&self, x: u8, y: u8, z: u8) -> usize {
        (y as usize * self.nz as usize + z as usize) * self.nx as usize + x as usize
    }
    pub fn coords(&self, i: usize) -> (u8, u8, u8) {
        let nx = self.nx as usize;
        let nz = self.nz as usize;
        let x = i % nx;
        let z = (i / nx) % nz;
        let y = i / (nx * nz);
        (x as u8, y as u8, z as u8)
    }
    pub fn chunk_aabb(&self, i: usize) -> Aabb {
        let (x, y, z) = self.coords(i);
        let s = self.aabb.max - self.aabb.min;
        let d = Vec3::new(s.x / self.nx as f32, s.y / self.ny as f32, s.z / self.nz as f32);
        let mn = self.aabb.min + Vec3::new(x as f32 * d.x, y as f32 * d.y, z as f32 * d.z);
        Aabb::new(mn, mn + d)
    }
    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|a| **a).count()
    }
    pub fn standing(&self) -> bool {
        self.alive.iter().any(|a| *a)
    }
    /// Destroy a chunk and anything it was holding up. Returns destroyed indices.
    pub fn destroy(&mut self, i: usize) -> Vec<usize> {
        let mut out = vec![];
        if !self.alive[i] {
            return out;
        }
        self.alive[i] = false;
        out.push(i);
        // Unsupported chunks above fall.
        let (x, y, z) = self.coords(i);
        for yy in (y + 1)..self.ny {
            let j = self.idx(x, yy, z);
            if self.alive[j] {
                // Supported if any neighbour at same level is attached to a column that is alive below.
                let below = self.idx(x, yy - 1, z);
                if !self.alive[below] {
                    self.alive[j] = false;
                    out.push(j);
                }
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointKind {
    /// Saint checkpoint.
    HangarBay,
    /// Kaiju checkpoint.
    DeepNest,
}

impl CheckpointKind {
    pub fn side(self) -> Side {
        match self {
            CheckpointKind::HangarBay => Side::Saint,
            CheckpointKind::DeepNest => Side::Kaiju,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub kind: CheckpointKind,
    pub pos: Vec3,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct City {
    pub size: f32,
    /// z below this is sea.
    pub shore_z: f32,
    pub buildings: Vec<Building>,
    pub checkpoints: Vec<Checkpoint>,
    /// Boss arena centre and radius.
    pub arena: (Vec3, f32),
    /// Power pylons (St. Anchor's grid).
    pub pylons: Vec<Vec3>,
    /// Fires / furnaces (Goliath Moth lure).
    pub fires: Vec<Vec3>,
    /// Street lamps (render point lights, lure at night).
    pub lamps: Vec<Vec3>,
    /// Enemy spawn points.
    pub spawns: Vec<Vec3>,
    pub landmark: Landmark,
}

impl City {
    /// Ground height at an XZ position (land = 0, sea slopes to SEA_FLOOR).
    pub fn ground(&self, x: f32, z: f32) -> f32 {
        let _ = x;
        if z >= self.shore_z { 0.0 } else { (-(self.shore_z - z) * 0.45).max(SEA_FLOOR) }
    }
    pub fn half(&self) -> f32 {
        self.size * 0.5
    }
    pub fn in_bounds(&self, p: Vec3) -> Vec3 {
        let h = self.half() - 4.0;
        Vec3::new(p.x.clamp(-h, h), p.y, p.z.clamp(-h - 30.0, h))
    }

    /// Generate a district layout. `destroyed` is persistent per-building chunk damage.
    pub fn generate(d: &DistrictDef, state: DistrictState, persistent: &[Vec<bool>]) -> City {
        let sv = &d.states[state.index()];
        let mut rng = SimRng::new(d.seed ^ (state as u64) << 32);
        let size = d.size;
        let half = size * 0.5;
        let shore_z = -half + d.sea * size;
        let block = d.block;
        let mut buildings = vec![];
        let pal = |rng: &mut SimRng| -> [u8; 3] {
            let c = d.palette[rng.below(d.palette.len() as u32) as usize];
            let k = rng.range(0.85, 1.1);
            [(c[0] as f32 * k).min(255.0) as u8, (c[1] as f32 * k).min(255.0) as u8, (c[2] as f32 * k).min(255.0) as u8]
        };
        // Landmark & arena near the centre-north.
        let arena_c = match d.landmark {
            Landmark::Docks | Landmark::Seawall | Landmark::Lighthouse => Vec3::new(0.0, 0.0, shore_z + 28.0),
            _ => Vec3::new(0.0, 0.0, (shore_z + half) * 0.5 - 6.0),
        };
        let arena_r = 34.0;
        let mut fires = vec![];
        let mut lamps = vec![];
        let mut pylons = vec![];
        let add = |b: &mut Vec<Building>, k: BuildingKind, c: Vec3, s: Vec3, col: [u8; 3]| {
            b.push(Building::new(
                k,
                Aabb::new(Vec3::new(c.x - s.x / 2.0, c.y, c.z - s.z / 2.0), Vec3::new(c.x + s.x / 2.0, c.y + s.y, c.z + s.z / 2.0)),
                col,
            ));
        };
        // Landmark structures surround the arena.
        let lc = arena_c;
        match d.landmark {
            Landmark::Docks => {
                for i in 0..3 {
                    let x = -40.0 + i as f32 * 40.0;
                    add(&mut buildings, BuildingKind::Crane, Vec3::new(x, 0.0, shore_z + 4.0), Vec3::new(4.0, 34.0, 4.0), [170, 120, 50]);
                    add(
                        &mut buildings,
                        BuildingKind::Pier,
                        Vec3::new(x + 10.0, -0.6, shore_z - 18.0),
                        Vec3::new(8.0, 1.2, 34.0),
                        [110, 90, 70],
                    );
                }
                add(&mut buildings, BuildingKind::Warehouse, lc + Vec3::new(-44.0, 0.0, 30.0), Vec3::new(30.0, 12.0, 18.0), pal(&mut rng));
                add(&mut buildings, BuildingKind::Warehouse, lc + Vec3::new(44.0, 0.0, 30.0), Vec3::new(30.0, 12.0, 18.0), pal(&mut rng));
            }
            Landmark::Cathedral => {
                add(&mut buildings, BuildingKind::Church, lc + Vec3::new(0.0, 0.0, 50.0), Vec3::new(26.0, 22.0, 34.0), [150, 140, 130]);
                add(&mut buildings, BuildingKind::Tower, lc + Vec3::new(0.0, 0.0, 30.0), Vec3::new(9.0, 44.0, 9.0), [140, 132, 124]);
                lamps.push(lc + Vec3::new(0.0, 40.0, 30.0));
            }
            Landmark::Smokestacks => {
                for i in 0..4 {
                    let p = lc + Vec3::new(-36.0 + i as f32 * 24.0, 0.0, 46.0);
                    add(&mut buildings, BuildingKind::Stack, p, Vec3::new(7.0, 46.0, 7.0), [120, 70, 56]);
                    fires.push(p + Vec3::new(0.0, 47.0, 0.0));
                }
                add(&mut buildings, BuildingKind::Warehouse, lc + Vec3::new(0.0, 0.0, 62.0), Vec3::new(60.0, 14.0, 16.0), [110, 90, 80]);
                fires.push(lc + Vec3::new(-20.0, 3.0, 40.0));
            }
            Landmark::Railyard => {
                for i in 0..6 {
                    let x = -50.0 + (i % 3) as f32 * 50.0;
                    let z = lc.z - 20.0 + (i / 3) as f32 * 46.0;
                    add(&mut buildings, BuildingKind::Railcar, Vec3::new(x, 0.0, z), Vec3::new(5.0, 5.0, 18.0), [120, 70, 50]);
                }
                add(&mut buildings, BuildingKind::Tower, lc + Vec3::new(40.0, 0.0, 50.0), Vec3::new(10.0, 18.0, 10.0), [110, 110, 104]);
            }
            Landmark::Lighthouse => {
                add(
                    &mut buildings,
                    BuildingKind::Lighthouse,
                    Vec3::new(-50.0, 0.0, shore_z + 10.0),
                    Vec3::new(10.0, 50.0, 10.0),
                    [220, 210, 200],
                );
                lamps.push(Vec3::new(-50.0, 52.0, shore_z + 10.0));
                fires.push(Vec3::new(-50.0, 52.0, shore_z + 10.0));
            }
            Landmark::Market => {
                for i in 0..10 {
                    let p = lc + Vec3::new(-40.0 + (i % 5) as f32 * 20.0, 0.0, -10.0 + (i / 5) as f32 * 64.0);
                    add(&mut buildings, BuildingKind::Stall, p, Vec3::new(8.0, 4.0, 6.0), pal(&mut rng));
                }
            }
            Landmark::Seawall => {
                // The breakwater has sluice gates: the sea always has a way in.
                let gates = [-60.0f32, 0.0, 34.0];
                let gw = 22.0;
                let mut x0 = -size * 0.45;
                for g in gates.iter().chain(std::iter::once(&(size * 0.45 + gw * 0.5))) {
                    let x1 = g - gw * 0.5;
                    if x1 - x0 > 4.0 {
                        let c = Vec3::new((x0 + x1) * 0.5, 0.0, shore_z - 2.0);
                        add(&mut buildings, BuildingKind::Wall, c, Vec3::new(x1 - x0, 9.0, 6.0), [130, 130, 124]);
                    }
                    x0 = g + gw * 0.5;
                }
            }
        }
        // City blocks on a grid.
        let mut z = shore_z + 12.0;
        while z < half - 20.0 {
            let mut x = -half + 16.0;
            while x < half - 16.0 {
                let c = Vec3::new(x + block * 0.5, 0.0, z + block * 0.5);
                let near_arena = Vec2::new(c.x - arena_c.x, c.z - arena_c.z).length() < arena_r + block * 0.6;
                let near_spawn = c.z > half - 50.0 && c.x.abs() < 30.0;
                if !near_arena && !near_spawn && rng.chance(0.82) {
                    let n = 1 + rng.below(2);
                    for k in 0..n {
                        let w = rng.range(10.0, 18.0);
                        let dz = rng.range(10.0, 18.0);
                        let off = Vec3::new(if n == 2 { (k as f32 - 0.5) * 14.0 } else { 0.0 }, 0.0, rng.range(-3.0, 3.0));
                        let tall = rng.chance(0.18);
                        let h = if tall { rng.range(24.0, 36.0) } else { rng.range(7.0, 16.0) };
                        let kind = if tall {
                            BuildingKind::Tower
                        } else if z < shore_z + 40.0 {
                            BuildingKind::Warehouse
                        } else {
                            BuildingKind::House
                        };
                        add(&mut buildings, kind, c + off, Vec3::new(w, h, dz), pal(&mut rng));
                    }
                }
                // Street lamps at block corners.
                if rng.chance(0.35) {
                    lamps.push(Vec3::new(x, 7.0, z));
                }
                x += block;
            }
            z += block;
        }
        // Grid pylons along the main road.
        for i in 0..4 {
            let p = Vec3::new(-60.0 + i as f32 * 40.0, 0.0, half - 60.0 - (i % 2) as f32 * 40.0);
            add(&mut buildings, BuildingKind::Pylon, p, Vec3::new(3.0, 20.0, 3.0), [90, 90, 96]);
            pylons.push(p + Vec3::new(0.0, 20.0, 0.0));
        }
        if sv.fortifications {
            for i in 0..6 {
                let p = Vec3::new(-50.0 + i as f32 * 20.0, 0.0, shore_z + 22.0 + (i % 2) as f32 * 6.0);
                add(&mut buildings, BuildingKind::Barricade, p, Vec3::new(12.0, 3.0, 2.0), [200, 170, 40]);
            }
        }
        // Remove buildings overlapping each other or the arena.
        let mut kept: Vec<Building> = vec![];
        for b in buildings {
            let c = b.aabb.center();
            let in_arena = Vec2::new(c.x - arena_c.x, c.z - arena_c.z).length() < arena_r - 4.0;
            if in_arena && !matches!(b.kind, BuildingKind::Tower | BuildingKind::Church) {
                continue;
            }
            if kept.iter().any(|k| k.aabb.expand(1.0).overlaps(&b.aabb) && k.kind != BuildingKind::Pier) {
                continue;
            }
            kept.push(b);
        }
        let mut buildings = kept;
        // Pre-ruin from district state, then apply persistent damage.
        let mut ruin_rng = SimRng::new(d.seed ^ 0xD00D);
        for b in buildings.iter_mut() {
            if matches!(b.kind, BuildingKind::Pier) {
                continue;
            }
            for i in 0..b.alive.len() {
                if ruin_rng.chance(sv.ruin * 0.6) {
                    b.destroy(i);
                }
            }
        }
        for (bi, mask) in persistent.iter().enumerate() {
            if let Some(b) = buildings.get_mut(bi) {
                for (i, dead) in mask.iter().enumerate() {
                    if *dead && i < b.alive.len() {
                        b.alive[i] = false;
                    }
                }
            }
        }
        let checkpoints = vec![
            Checkpoint { kind: CheckpointKind::HangarBay, pos: Vec3::new(0.0, 0.0, half - 30.0), name: format!("{} HANGAR BAY", d.name) },
            Checkpoint { kind: CheckpointKind::DeepNest, pos: Vec3::new(30.0, 0.0, shore_z - 6.0), name: format!("{} DEEP NEST", d.name) },
            Checkpoint {
                kind: CheckpointKind::HangarBay,
                pos: arena_c + Vec3::new(-arena_r - 14.0, 0.0, 10.0),
                name: "FORWARD HANGAR".into(),
            },
            Checkpoint {
                kind: CheckpointKind::DeepNest,
                pos: arena_c + Vec3::new(arena_r + 14.0, 0.0, -10.0),
                name: "FORWARD NEST".into(),
            },
        ];
        let mut spawns = vec![];
        let mut sr = SimRng::new(d.seed ^ 0x5EED);
        let mut tries = 0;
        while spawns.len() < 14 && tries < 400 {
            tries += 1;
            let p = Vec3::new(sr.range(-half + 20.0, half - 20.0), 0.0, sr.range(shore_z - 10.0, half - 60.0));
            let blocked = buildings.iter().any(|b| b.aabb.expand(4.0).contains(Vec3::new(p.x, 1.0, p.z)));
            let in_arena = Vec2::new(p.x - arena_c.x, p.z - arena_c.z).length() < arena_r + 6.0;
            if !blocked && !in_arena {
                spawns.push(p);
            }
        }
        City { size, shore_z, buildings, checkpoints, arena: (arena_c, arena_r), pylons, fires, lamps, spawns, landmark: d.landmark }
    }

    /// Persistent destruction mask (for the campaign save).
    pub fn damage_mask(&self) -> Vec<Vec<bool>> {
        self.buildings.iter().map(|b| b.alive.iter().map(|a| !a).collect()).collect()
    }

    /// Fraction of chunks destroyed (drives the Ruined state).
    pub fn ruin(&self) -> f32 {
        let total: usize = self.buildings.iter().map(|b| b.alive.len()).sum();
        let dead: usize = self.buildings.iter().map(|b| b.alive.len() - b.alive_count()).sum();
        if total == 0 { 0.0 } else { dead as f32 / total as f32 }
    }

    pub fn nearest_checkpoint(&self, p: Vec3, side: Side) -> Option<&Checkpoint> {
        self.checkpoints
            .iter()
            .filter(|c| c.kind.side() == side)
            .min_by(|a, b| a.pos.distance_squared(p).total_cmp(&b.pos.distance_squared(p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_collapse_when_unsupported() {
        let mut b = Building::new(BuildingKind::Tower, Aabb::new(Vec3::ZERO, Vec3::new(8.0, 18.0, 8.0)), [1, 2, 3]);
        assert_eq!((b.nx, b.ny, b.nz), (1, 3, 1));
        let gone = b.destroy(0);
        assert_eq!(gone.len(), 3, "whole column falls");
        assert!(!b.standing());
    }
}
