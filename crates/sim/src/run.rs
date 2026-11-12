//! Tide Runs: seeded 30-60 minute roguelite runs.
//!
//! Improvement over the original design: a run *alternates* discs. You carry one Saint and one
//! Kaiju; each fight is played as the other side from the last, and the draft after a fight offers
//! parts torn from the enemies you just beat - which fit the build you will play next. A Tide Run
//! is a compressed Cross-Harvest loop rather than a single-side gauntlet.

use crate::build::Loadout;
use crate::parts::{Condition, Origin, PartInstance};
use crate::rng::SimRng;
use crate::war::side_ix;
use crate::world::SpawnSpec;
use isl_data::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunFight {
    pub district: String,
    pub state: DistrictState,
    pub side: Side,
    pub spawns: Vec<SpawnSpec>,
    pub boss: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TideRun {
    pub seed: u64,
    pub daily: Option<String>,
    pub rng: SimRng,
    pub fights: Vec<RunFight>,
    pub index: usize,
    pub builds: [Loadout; 2],
    pub omens: Vec<String>,
    pub score: u32,
    pub offer: Vec<PartInstance>,
    pub harvested: Vec<PartInstance>,
    pub over: bool,
    pub won: bool,
    next_uid: u64,
}

impl TideRun {
    pub fn new(content: &Content, seed: u64, omens: Vec<String>, daily: Option<String>) -> TideRun {
        let mut rng = SimRng::new(seed);
        let saints: Vec<&ChassisDef> = content.starting_chassis(Side::Saint).collect();
        let kaiju: Vec<&ChassisDef> = content.starting_chassis(Side::Kaiju).collect();
        let s = *rng.pick(&saints).expect("saints");
        let k = *rng.pick(&kaiju).expect("kaiju");
        let builds = [Loadout::premade(s, content, 1 << 50), Loadout::premade(k, content, 1 << 51)];
        let first = if rng.chance(0.5) { Side::Saint } else { Side::Kaiju };
        let districts: Vec<&DistrictDef> = content.districts.iter().filter(|d| !d.finale).collect();
        let mut fights = vec![];
        let n = 6;
        for i in 0..n {
            let side = if i % 2 == 0 { first } else { first.other() };
            let d = *rng.pick(&districts).unwrap();
            let state = DistrictState::ALL[rng.below(4) as usize];
            let sv = &d.states[state.index()];
            let pool = match side {
                Side::Saint => &sv.saint_enemies,
                Side::Kaiju => &sv.kaiju_enemies,
            };
            let boss = i == n - 1 || i == n - 2;
            let mut spawns = vec![];
            if boss {
                let b = match side {
                    Side::Saint => d.saint_boss.clone(),
                    Side::Kaiju => d.kaiju_boss.clone(),
                };
                spawns.push(SpawnSpec::Boss(b));
            } else {
                let count = 3 + i as u32 + rng.below(2);
                for _ in 0..count {
                    if let Some(e) = rng.pick(pool) {
                        spawns.push(SpawnSpec::Enemy(e.clone()));
                    }
                }
            }
            fights.push(RunFight { district: d.id.clone(), state, side, spawns, boss });
        }
        TideRun { seed, daily, rng, fights, index: 0, builds, omens, score: 0, offer: vec![], harvested: vec![], over: false, won: false, next_uid: 1 << 52 }
    }

    pub fn current(&self) -> Option<&RunFight> {
        self.fights.get(self.index)
    }

    pub fn reward_mul(&self, content: &Content) -> f32 {
        self.omens.iter().filter_map(|o| content.omen(o)).map(|o| o.reward).product()
    }

    pub fn omen_effects(&self, content: &Content) -> (Vec<Effect>, Vec<Effect>) {
        let mut player = vec![];
        let mut enemy = vec![];
        for o in self.omens.iter().filter_map(|o| content.omen(o)) {
            for e in &o.effects {
                match e {
                    Effect::EnemyDamage(_) | Effect::EnemyHp(_) | Effect::EnemySpeed(_) | Effect::Echo => enemy.push(e.clone()),
                    _ => player.push(e.clone()),
                }
            }
        }
        (player, enemy)
    }

    /// Fight won: offer three parts torn from that fight's enemies.
    pub fn win_fight(&mut self, content: &Content, collected: Vec<PartInstance>) {
        let Some(f) = self.current().cloned() else { return };
        self.score += ((if f.boss { 1000 } else { 300 }) as f32 * self.reward_mul(content)) as u32;
        self.harvested.extend(collected.iter().cloned());
        let mut pool: Vec<String> = vec![];
        for s in &f.spawns {
            let parts = match s {
                SpawnSpec::Enemy(id) => content.enemy(id).map(|e| e.parts.to_vec()),
                SpawnSpec::Boss(id) => content.boss(id).map(|b| b.parts.to_vec()),
                _ => None,
            };
            if let Some(p) = parts {
                pool.extend(p);
            }
        }
        pool.extend(collected.iter().map(|c| c.def.clone()));
        pool.sort();
        pool.dedup();
        self.offer.clear();
        for _ in 0..3 {
            if pool.is_empty() {
                break;
            }
            let k = self.rng.below(pool.len() as u32) as usize;
            let id = pool.remove(k);
            if let Some(d) = content.part(&id) {
                self.next_uid += 16;
                let cond = if f.boss { Condition::Pristine } else { Condition::Scarred };
                self.offer.push(PartInstance::harvest(d, cond, Origin::TideRun(self.seed), self.next_uid, &mut self.rng, content));
            }
        }
        self.index += 1;
        if self.index >= self.fights.len() {
            self.over = true;
            self.won = true;
        }
    }

    /// Draft a part from the offer into the build of its own anatomy.
    pub fn draft(&mut self, content: &Content, k: usize) -> Option<String> {
        let p = self.offer.get(k)?.clone();
        let d = content.part(&p.def)?;
        let b = &mut self.builds[side_ix(d.side)];
        b.slots[d.slot.index()] = Some(p);
        b.chassis = None;
        b.name = format!("RUN {:08X}", self.seed as u32);
        self.offer.clear();
        Some(d.name.clone())
    }

    pub fn lose(&mut self) {
        self.over = true;
        self.won = false;
    }
}
