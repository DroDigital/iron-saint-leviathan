//! "Your history is the enemy."
//!
//! The ghost module records *habit statistics*, not replays: dodge direction bias, favourite
//! attacks and combos, heal timing, preferred distance, parry rate and how you react to feints.
//! A [`HabitProfile`] then drives a nemesis through ordinary [`AiParams`], so a nemesis can only
//! do what a player could do - it just does it the way *you* do.

use isl_data::*;
use isl_sim::ai::AiParams;
use isl_sim::fighter::{Act, SimEvent};
use isl_sim::world::World;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HabitProfile {
    pub dodges: [u32; 4],
    pub attacks: BTreeMap<String, u32>,
    pub combos: BTreeMap<String, u32>,
    pub kinds: [u32; 4],
    pub heal_hp: Vec<f32>,
    pub distance_sum: f64,
    pub distance_n: u32,
    pub enemy_attacks: u32,
    pub parry_attempts: u32,
    pub parries: u32,
    pub feints_seen: u32,
    pub feints_bitten: u32,
    pub ticks: u64,
    pub deaths: u32,
    /// Missions folded into this profile.
    pub missions: u32,
}

const DIRS: [DodgeDir; 4] = [DodgeDir::Left, DodgeDir::Right, DodgeDir::Back, DodgeDir::Forward];

impl HabitProfile {
    pub fn dodge_total(&self) -> u32 {
        self.dodges.iter().sum()
    }
    pub fn dodge_frac(&self, d: DodgeDir) -> f32 {
        let t = self.dodge_total();
        if t == 0 {
            return 0.25;
        }
        let i = DIRS.iter().position(|x| *x == d).unwrap_or(0);
        self.dodges[i] as f32 / t as f32
    }
    /// P(left | lateral dodge).
    pub fn left_bias(&self) -> f32 {
        let l = self.dodges[0] as f32;
        let r = self.dodges[1] as f32;
        if l + r == 0.0 { 0.5 } else { l / (l + r) }
    }
    pub fn heal_below(&self) -> f32 {
        if self.heal_hp.is_empty() { 0.3 } else { self.heal_hp.iter().sum::<f32>() / self.heal_hp.len() as f32 }
    }
    pub fn mean_distance(&self) -> f32 {
        if self.distance_n == 0 { 12.0 } else { (self.distance_sum / self.distance_n as f64) as f32 }
    }
    pub fn parry_rate(&self) -> f32 {
        (self.parry_attempts as f32 / self.enemy_attacks.max(1) as f32).min(1.0)
    }
    pub fn dodge_rate(&self) -> f32 {
        (self.dodge_total() as f32 / self.enemy_attacks.max(1) as f32).min(1.0)
    }
    pub fn feint_bite(&self) -> f32 {
        if self.feints_seen == 0 { 0.5 } else { self.feints_bitten as f32 / self.feints_seen as f32 }
    }
    /// Attacks per minute.
    pub fn aggression(&self) -> f32 {
        let total: u32 = self.kinds.iter().sum();
        total as f32 / (self.ticks.max(1) as f32 / 3600.0)
    }
    pub fn favourite(&self) -> Option<&str> {
        self.attacks.iter().max_by_key(|(_, n)| **n).map(|(k, _)| k.as_str())
    }
    pub fn favourite_combo(&self) -> Option<&str> {
        self.combos.iter().max_by_key(|(_, n)| **n).map(|(k, _)| k.as_str())
    }

    /// Fold another profile in, decaying the old one so habits can change.
    pub fn merge(&mut self, o: &HabitProfile) {
        let decay = |x: u32| (x as f32 * 0.8).round() as u32;
        for i in 0..4 {
            self.dodges[i] = decay(self.dodges[i]) + o.dodges[i];
            self.kinds[i] = decay(self.kinds[i]) + o.kinds[i];
        }
        for (k, v) in &o.attacks {
            *self.attacks.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.combos {
            *self.combos.entry(k.clone()).or_default() += v;
        }
        self.heal_hp.extend(o.heal_hp.iter().copied());
        if self.heal_hp.len() > 64 {
            let n = self.heal_hp.len() - 64;
            self.heal_hp.drain(0..n);
        }
        self.distance_sum = self.distance_sum * 0.8 + o.distance_sum;
        self.distance_n = decay(self.distance_n) + o.distance_n;
        self.enemy_attacks = decay(self.enemy_attacks) + o.enemy_attacks;
        self.parry_attempts = decay(self.parry_attempts) + o.parry_attempts;
        self.parries = decay(self.parries) + o.parries;
        self.feints_seen = decay(self.feints_seen) + o.feints_seen;
        self.feints_bitten = decay(self.feints_bitten) + o.feints_bitten;
        self.ticks = (self.ticks as f64 * 0.8) as u64 + o.ticks;
        self.deaths += o.deaths;
        self.missions += 1;
    }

    /// Map habits to nemesis AI parameters.
    pub fn to_params(&self) -> AiParams {
        let mut p = AiParams::for_archetype(Archetype::Boss);
        p.dodge_left = self.left_bias();
        p.dodge_back = self.dodge_frac(DodgeDir::Back);
        p.dodge_rate = (self.dodge_rate() * 0.8).clamp(0.05, 0.75);
        p.parry_rate = (self.parry_rate() * 0.8).clamp(0.0, 0.5);
        p.guard_rate = 0.1;
        p.heal_below = self.heal_below().clamp(0.1, 0.8);
        p.feint_patience = 1.0 - self.feint_bite();
        let d = self.mean_distance();
        p.engage = (d / 8.0).clamp(0.6, 4.0);
        let apm = self.aggression();
        p.aggression = (apm / 40.0).clamp(0.25, 0.95);
        p.cadence = (60.0 - apm.min(50.0)).max(18.0) as u16;
        let k = &self.kinds;
        let tot = k.iter().sum::<u32>().max(1) as f32;
        p.w_light = 0.2 + k[0] as f32 / tot * 4.0;
        p.w_heavy = 0.2 + k[1] as f32 / tot * 4.0;
        p.w_special = 0.1 + k[2] as f32 / tot * 4.0;
        p.w_bash = 0.1 + k[3] as f32 / tot * 4.0;
        p.combo = if self.favourite_combo().is_some() { 3 } else { 2 };
        p
    }

    /// Nemesis intro card, e.g. "Pilot: YOU (War 3, Tide 14). Habits: dodges left 71%; heals below 30%."
    pub fn intro_card(&self, war: u32, tide: u32, content: &Content) -> String {
        let mut habits = vec![];
        let lb = self.left_bias();
        if self.dodges[0] + self.dodges[1] > 0 {
            if lb >= 0.5 {
                habits.push(format!("dodges left {}%", (lb * 100.0).round()));
            } else {
                habits.push(format!("dodges right {}%", ((1.0 - lb) * 100.0).round()));
            }
        }
        if !self.heal_hp.is_empty() {
            habits.push(format!("heals below {}%", (self.heal_below() * 100.0).round()));
        }
        if let Some(f) = self.favourite() {
            let name = content.mv(f).map_or(f.to_string(), |m| m.name.clone());
            habits.push(format!("favours {name}"));
        }
        if self.feints_seen > 0 {
            habits.push(format!("bites feints {}%", (self.feint_bite() * 100.0).round()));
        }
        if self.parry_attempts > 0 {
            habits.push(format!("parries {}%", (self.parry_rate() * 100.0).round()));
        }
        if habits.is_empty() {
            habits.push("unknown".into());
        }
        format!("Pilot: YOU (War {war}, Tide {tide}). Habits: {}.", habits.join("; "))
    }
}

/// Watches the player during a mission and builds a [`HabitProfile`].
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    pub profile: HabitProfile,
    last_attack: Option<(String, u64)>,
    /// Enemy feint attacks in flight: (attacker id, start tick, nominal windup).
    feints: Vec<(u32, u64, u16)>,
    player: u32,
}

impl Recorder {
    pub fn new(player: u32) -> Recorder {
        Recorder { player, ..Default::default() }
    }

    pub fn observe(&mut self, world: &World, content: &Content, events: &[SimEvent]) {
        let tick = world.tick;
        let me = self.player;
        self.profile.ticks += 1;
        for e in events {
            match e {
                SimEvent::Dodge { id, dir } if *id == me => {
                    if let Some(i) = DIRS.iter().position(|d| d == dir) {
                        self.profile.dodges[i] += 1;
                    }
                    self.react(tick);
                }
                SimEvent::ParryAttempt { id } if *id == me => {
                    self.profile.parry_attempts += 1;
                    self.react(tick);
                }
                SimEvent::Parried { by, .. } if *by == me => self.profile.parries += 1,
                SimEvent::Heal { id, hp_frac } if *id == me => self.profile.heal_hp.push(*hp_frac),
                SimEvent::PlayerDied { id, .. } if *id == me => self.profile.deaths += 1,
                SimEvent::AttackStart { id, mv, feint, .. } => {
                    if *id == me {
                        *self.profile.attacks.entry(mv.clone()).or_default() += 1;
                        if let Some(m) = content.mv(mv) {
                            let k = match m.kind {
                                MoveKind::Light => 0,
                                MoveKind::Heavy => 1,
                                MoveKind::Bash => 3,
                                _ => 2,
                            };
                            self.profile.kinds[k] += 1;
                        }
                        if let Some((prev, t)) = &self.last_attack
                            && tick - t < 70
                        {
                            *self.profile.combos.entry(format!("{prev}>{mv}")).or_default() += 1;
                        }
                        self.last_attack = Some((mv.clone(), tick));
                    } else if world.fighters.get(*id as usize).is_some_and(|f| f.team.hostile(world.fighters[me as usize].team)) {
                        let near = world.fighters[*id as usize].pos.distance(world.fighters[me as usize].pos) < 30.0;
                        if near {
                            self.profile.enemy_attacks += 1;
                            if *feint > 0 {
                                let w = content.mv(mv).map_or(20, |m| m.windup);
                                self.feints.push((*id, tick, w));
                                self.profile.feints_seen += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.feints.retain(|(_, t, w)| tick < t + *w as u64 + 40);
        if tick.is_multiple_of(30) {
            let f = &world.fighters[me as usize];
            if !matches!(f.act, Act::Dead) {
                let d = world
                    .fighters
                    .iter()
                    .filter(|o| o.alive && f.team.hostile(o.team) && !matches!(o.act, Act::Dead) && !o.hallucination)
                    .map(|o| o.pos.distance(f.pos) - o.radius() - f.radius())
                    .fold(f32::MAX, f32::min);
                if d < 40.0 {
                    self.profile.distance_sum += d.max(0.0) as f64;
                    self.profile.distance_n += 1;
                }
            }
        }
    }

    /// A defensive press: did it come during a feint's extra hold (bit the feint)?
    fn react(&mut self, tick: u64) {
        if let Some(k) = self.feints.iter().position(|(_, t, w)| tick >= *t && tick < t + *w as u64) {
            self.profile.feints_bitten += 1;
            self.feints.remove(k);
        } else if let Some(k) = self.feints.iter().position(|(_, t, _)| tick >= *t) {
            self.feints.remove(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_extraction() {
        let mut p = HabitProfile::default();
        p.dodges = [71, 29, 0, 0];
        p.heal_hp = vec![0.25, 0.35];
        p.enemy_attacks = 100;
        p.parry_attempts = 20;
        p.feints_seen = 10;
        p.feints_bitten = 8;
        p.kinds = [60, 20, 10, 10];
        p.ticks = 3600 * 2;
        p.distance_sum = 80.0;
        p.distance_n = 10;
        assert!((p.left_bias() - 0.71).abs() < 1e-6);
        assert!((p.heal_below() - 0.30).abs() < 1e-6);
        let a = p.to_params();
        assert!((a.dodge_left - 0.71).abs() < 1e-6);
        assert!((a.heal_below - 0.30).abs() < 1e-6);
        assert!(a.w_light > a.w_heavy, "favours lights");
        assert!(a.feint_patience < 0.3, "bites feints -> impatient nemesis");
        assert!((a.engage - 1.0).abs() < 1e-6);
        let mut c = Content::default();
        c.reindex();
        let card = p.intro_card(3, 14, &c);
        assert_eq!(card, "Pilot: YOU (War 3, Tide 14). Habits: dodges left 71%; heals below 30%; bites feints 80%; parries 20%.");
    }

    #[test]
    fn merge_decays_old_habits() {
        let mut a = HabitProfile { dodges: [100, 0, 0, 0], ..Default::default() };
        let b = HabitProfile { dodges: [0, 100, 0, 0], ..Default::default() };
        a.merge(&b);
        assert!(a.left_bias() < 0.5, "recent right dodges outweigh decayed left ones");
        assert_eq!(a.missions, 1);
    }
}
