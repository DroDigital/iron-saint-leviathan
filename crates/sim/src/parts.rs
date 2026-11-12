//! Part instances (what you actually own), conditions, quirk rolling and effect aggregation.

use crate::rng::SimRng;
use isl_data::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Condition {
    Pristine,
    Scarred,
    Mangled,
}

impl Condition {
    /// Multiplier applied to the part's HP, armour, power and impact.
    pub fn stat_mul(self) -> f32 {
        match self {
            Condition::Pristine => 1.0,
            Condition::Scarred => 0.9,
            Condition::Mangled => 0.78,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Condition::Pristine => "PRISTINE",
            Condition::Scarred => "SCARRED",
            Condition::Mangled => "MANGLED",
        }
    }
    pub fn worse(self) -> Condition {
        match self {
            Condition::Pristine => Condition::Scarred,
            _ => Condition::Mangled,
        }
    }
}

/// Who a part came from. Drives Old Grudge, nemesis drops and the Codex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Premade(String),
    Boss(String),
    Enemy(String),
    /// From a nemesis built out of your own saved build.
    Nemesis(String),
    /// Stripped from your own wreck/carcass.
    OwnRemains {
        war: u32,
        tide: u32,
    },
    Import(String),
    Fabricated,
    Contract(String),
    Corrupted {
        war: u32,
    },
    Event(String),
    TideRun(u64),
}

impl Origin {
    /// Identifier of the former owner, used by Old Grudge.
    pub fn owner_tag(&self) -> Option<&str> {
        match self {
            Origin::Premade(s) | Origin::Boss(s) | Origin::Enemy(s) | Origin::Nemesis(s) => Some(s),
            _ => None,
        }
    }
    pub fn describe(&self) -> String {
        match self {
            Origin::Premade(s) => format!("Premade ({s})"),
            Origin::Boss(s) => format!("Severed from {s}"),
            Origin::Enemy(s) => format!("Harvested from {s}"),
            Origin::Nemesis(s) => format!("Your own: {s}"),
            Origin::OwnRemains { war, tide } => format!("Your remains, War {war} Tide {tide}"),
            Origin::Import(c) => format!("Imported ({c})"),
            Origin::Fabricated => "Fabricated from the Codex".into(),
            Origin::Contract(c) => format!("Contract reward ({c})"),
            Origin::Corrupted { war } => format!("Corrupted block, War {war}"),
            Origin::Event(e) => format!("Event: {e}"),
            Origin::TideRun(s) => format!("Tide Run {s:08X}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartInstance {
    pub uid: u64,
    pub def: String,
    pub condition: Condition,
    pub quirks: Vec<String>,
    pub origin: Origin,
    #[serde(default)]
    pub memory: PartMemory,
}

impl PartInstance {
    pub fn premade(def: &PartDef, uid: u64, chassis: &str) -> Self {
        PartInstance {
            uid,
            def: def.id.clone(),
            condition: Condition::Pristine,
            quirks: def.quirks.clone(),
            origin: Origin::Premade(chassis.to_string()),
            memory: def.memory.clone(),
        }
    }

    /// Create a harvested part with quirks rolled from its condition.
    pub fn harvest(def: &PartDef, condition: Condition, origin: Origin, uid: u64, rng: &mut SimRng, content: &Content) -> Self {
        let imported = matches!(origin, Origin::Import(_));
        let mut quirks = def.quirks.clone();
        for q in roll_quirks(rng, content, def.side, condition, imported) {
            if !quirks.contains(&q) {
                quirks.push(q);
            }
        }
        let mut memory = def.memory.clone();
        // Memory drifts a little per instance so no two harvested parts behave identically.
        memory.dodge_bias = (memory.dodge_bias + rng.range(-0.15, 0.15)).clamp(-1.0, 1.0);
        memory.twitch = (memory.twitch + rng.range(0.0, 0.1)).clamp(0.0, 1.0);
        PartInstance { uid, def: def.id.clone(), condition, quirks, origin, memory }
    }

    /// Repair one step (Mangled -> Scarred), removing the worst quirk. Returns false if nothing to do.
    pub fn repair(&mut self, content: &Content) -> bool {
        if self.condition != Condition::Mangled {
            return false;
        }
        self.condition = Condition::Scarred;
        if let Some(i) = self
            .quirks
            .iter()
            .enumerate()
            .filter_map(|(i, q)| content.quirk(q).map(|d| (i, d.polarity)))
            .filter(|(_, p)| *p == Polarity::Negative)
            .map(|(i, _)| i)
            .next()
        {
            self.quirks.remove(i);
        }
        true
    }

    pub fn repair_cost(&self) -> u32 {
        match self.condition {
            Condition::Mangled => 300,
            _ => 0,
        }
    }

    /// Re-roll all non-innate quirks (advanced Foundry/Nest feature).
    pub fn reroll(&mut self, rng: &mut SimRng, content: &Content) {
        let Some(def) = content.part(&self.def) else { return };
        let keep: Vec<String> =
            self.quirks.iter().filter(|q| def.quirks.contains(q) || content.quirk(q).is_some_and(|d| d.special)).cloned().collect();
        self.quirks = keep;
        for q in roll_quirks(rng, content, def.side, self.condition.max(Condition::Scarred), false) {
            if !self.quirks.contains(&q) {
                self.quirks.push(q);
            }
        }
    }
}

/// Roll quirks for a condition. Pristine: none. Scarred: one positive-or-mixed. Mangled: one or two
/// mixed-or-negative. Side-restricted quirks only roll on matching anatomy; import-only quirks only
/// roll on imported parts.
pub fn roll_quirks(rng: &mut SimRng, content: &Content, side: Side, condition: Condition, imported: bool) -> Vec<String> {
    let pool = |pols: &[Polarity]| -> Vec<&QuirkDef> {
        content
            .quirks
            .iter()
            .filter(|q| !q.special)
            .filter(|q| q.side.is_none_or(|s| s == side))
            .filter(|q| pols.contains(&q.polarity))
            .filter(|q| !q.import_only || imported)
            .collect()
    };
    let mut out = Vec::new();
    match condition {
        Condition::Pristine => {}
        Condition::Scarred => {
            let p = pool(&[Polarity::Positive, Polarity::Mixed]);
            if let Some(q) = rng.pick(&p) {
                out.push(q.id.clone());
            }
        }
        Condition::Mangled => {
            let p = pool(&[Polarity::Mixed, Polarity::Negative]);
            let n = 1 + rng.below(2);
            for _ in 0..n {
                if let Some(q) = rng.pick(&p) {
                    if !out.contains(&q.id) {
                        out.push(q.id.clone());
                    }
                }
            }
        }
    }
    if imported {
        let p: Vec<&QuirkDef> = content.quirks.iter().filter(|q| q.import_only).collect();
        if rng.chance(0.5) {
            if let Some(q) = rng.pick(&p) {
                if !out.contains(&q.id) {
                    out.push(q.id.clone());
                }
            }
        }
    }
    out
}

/// Aggregated effect of quirks, omens, traits, conditions and meta bonuses on a fighter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub stat: [f32; 16],
    pub early_fire: f32,
    pub early_skip: f32,
    pub grudges: Vec<(String, f32)>,
    pub idle_drain: f32,
    /// Extra i-frames: [left, right, back, forward, any].
    pub dodge_iframes: [i16; 5],
    pub parry_window: i16,
    pub desperate: Vec<(f32, f32)>,
    pub leech: f32,
    pub ignite: bool,
    pub bleed: f32,
    pub steady: f32,
    pub no_regrow: bool,
    pub hallucinate: f32,
    pub drift: f32,
    pub greed: f32,
    pub harvester: f32,
    pub no_lock: bool,
    pub no_hud: bool,
    pub glass_hull: bool,
    pub famine: f32,
    pub echo: bool,
    pub riptide: bool,
    pub enemy_damage: f32,
    pub enemy_hp: f32,
    pub enemy_speed: f32,
    pub heal_mul: f32,
    pub no_recovery: bool,
    pub markers: Vec<String>,
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            stat: [1.0; 16],
            early_fire: 0.0,
            early_skip: 0.0,
            grudges: vec![],
            idle_drain: 0.0,
            dodge_iframes: [0; 5],
            parry_window: 0,
            desperate: vec![],
            leech: 0.0,
            ignite: false,
            bleed: 0.0,
            steady: 1.0,
            no_regrow: false,
            hallucinate: 0.0,
            drift: 0.0,
            greed: 1.0,
            harvester: 1.0,
            no_lock: false,
            no_hud: false,
            glass_hull: false,
            famine: 1.0,
            echo: false,
            riptide: false,
            enemy_damage: 1.0,
            enemy_hp: 1.0,
            enemy_speed: 1.0,
            heal_mul: 1.0,
            no_recovery: false,
            markers: vec![],
        }
    }
}

impl Effects {
    pub fn get(&self, s: Stat) -> f32 {
        self.stat[s as usize]
    }
    pub fn add(&mut self, e: &Effect, grudge_target: Option<&str>) {
        match e {
            Effect::StatMul(s, m) => self.stat[*s as usize] *= m,
            Effect::EarlyFire { chance, skip } => {
                self.early_fire = (self.early_fire + chance).min(0.9);
                self.early_skip = self.early_skip.max(*skip);
            }
            Effect::Grudge(m) => {
                if let Some(t) = grudge_target {
                    self.grudges.push((t.to_string(), *m));
                }
            }
            Effect::IdleDrain(x) => self.idle_drain += x,
            Effect::DodgeIframes(d, n) => {
                let i = match d {
                    DodgeDir::Left => 0,
                    DodgeDir::Right => 1,
                    DodgeDir::Back => 2,
                    DodgeDir::Forward => 3,
                    DodgeDir::Any => 4,
                };
                self.dodge_iframes[i] += *n as i16;
            }
            Effect::ParryWindow(n) => self.parry_window += *n as i16,
            Effect::Desperate { below, mul } => self.desperate.push((*below, *mul)),
            Effect::Leech(x) => self.leech += x,
            Effect::Ignite => self.ignite = true,
            Effect::Bleed(x) => self.bleed += x,
            Effect::Steady(x) => self.steady *= x,
            Effect::NoRegrow => self.no_regrow = true,
            Effect::Hallucinate(x) => self.hallucinate += x,
            Effect::Drift(x) => self.drift += x,
            Effect::Greed(x) => self.greed *= x,
            Effect::Harvester(x) => self.harvester *= x,
            Effect::NoLock => self.no_lock = true,
            Effect::NoHud => self.no_hud = true,
            Effect::GlassHull => self.glass_hull = true,
            Effect::Famine(x) => self.famine *= x,
            Effect::Echo => self.echo = true,
            Effect::Riptide => self.riptide = true,
            Effect::EnemyDamage(x) => self.enemy_damage *= x,
            Effect::EnemyHp(x) => self.enemy_hp *= x,
            Effect::EnemySpeed(x) => self.enemy_speed *= x,
            Effect::HealMul(x) => self.heal_mul *= x,
            Effect::NoRecovery => self.no_recovery = true,
            Effect::Marker(m) => {
                if !self.markers.contains(m) {
                    self.markers.push(m.clone())
                }
            }
        }
    }
    pub fn has_marker(&self, m: &str) -> bool {
        self.markers.iter().any(|x| x == m)
    }
    pub fn grudge_vs(&self, tag: &str) -> f32 {
        self.grudges.iter().filter(|(t, _)| t == tag).map(|(_, m)| *m).product()
    }
    pub fn desperate_mul(&self, hp_frac: f32) -> f32 {
        self.desperate.iter().filter(|(b, _)| hp_frac < *b).map(|(_, m)| *m).product()
    }
    pub fn dodge_bonus(&self, dir: DodgeDir) -> i16 {
        let i = match dir {
            DodgeDir::Left => 0,
            DodgeDir::Right => 1,
            DodgeDir::Back => 2,
            DodgeDir::Forward => 3,
            DodgeDir::Any => 4,
        };
        self.dodge_iframes[4] + if i < 4 { self.dodge_iframes[i] } else { 0 }
    }
}

/// Clamp the sum of permanent meta stat bonuses to the design cap (+10% total, so skill matters).
pub fn cap_meta_bonuses(effects: &[Effect], cap: f32) -> Vec<Effect> {
    let mut budget = cap;
    let mut out = Vec::new();
    for e in effects {
        match e {
            Effect::StatMul(s, m) if *m > 1.0 => {
                let want = m - 1.0;
                let take = want.min(budget);
                budget -= take;
                if take > 0.0 {
                    out.push(Effect::StatMul(*s, 1.0 + take));
                }
            }
            other => out.push(other.clone()),
        }
    }
    out
}
