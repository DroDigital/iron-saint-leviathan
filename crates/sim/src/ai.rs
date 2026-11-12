//! Enemy, boss, ally and nemesis brains.
//!
//! Fairness rules (design section 5): AI perceives a *delayed* snapshot of the world (its
//! `reaction` delay, never under 100 ms) and acts only through [`Command`]s, exactly like the
//! player. It cannot read inputs, only animations that have already started.

use crate::fighter::Command;
use crate::rng::SimRng;
use glam::{Vec2, Vec3};
use isl_data::*;
use serde::{Deserialize, Serialize};

/// Behaviour knobs. Archetypes have defaults; nemeses derive these from recorded habits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiParams {
    /// Preferred distance as a multiple of the attacker's reach.
    pub engage: f32,
    /// Probability per decision of committing to an attack.
    pub aggression: f32,
    pub dodge_rate: f32,
    /// P(left) given a lateral dodge.
    pub dodge_left: f32,
    pub dodge_back: f32,
    pub parry_rate: f32,
    pub guard_rate: f32,
    /// Heal when HP fraction drops below this.
    pub heal_below: f32,
    pub w_light: f32,
    pub w_heavy: f32,
    pub w_special: f32,
    pub w_bash: f32,
    /// Probability of waiting out a feint instead of reacting to the first twitch.
    pub feint_patience: f32,
    /// Attacks per string.
    pub combo: u8,
    /// Ticks between attack decisions.
    pub cadence: u16,
}

impl AiParams {
    pub fn for_archetype(a: Archetype) -> AiParams {
        let base = AiParams {
            engage: 0.9,
            aggression: 0.5,
            dodge_rate: 0.15,
            dodge_left: 0.5,
            dodge_back: 0.3,
            parry_rate: 0.0,
            guard_rate: 0.15,
            heal_below: 0.0,
            w_light: 3.0,
            w_heavy: 1.5,
            w_special: 0.5,
            w_bash: 0.5,
            feint_patience: 0.3,
            combo: 2,
            cadence: 50,
        };
        match a {
            Archetype::Brute => base,
            Archetype::Gunner => {
                AiParams { engage: 7.0, aggression: 0.45, w_light: 2.0, w_heavy: 2.0, dodge_rate: 0.1, combo: 1, cadence: 70, ..base }
            }
            Archetype::Swarmer => {
                AiParams { engage: 0.7, aggression: 0.65, dodge_rate: 0.05, guard_rate: 0.0, combo: 1, cadence: 40, ..base }
            }
            Archetype::Flyer => AiParams { engage: 6.0, aggression: 0.4, dodge_rate: 0.2, combo: 1, cadence: 70, ..base },
            Archetype::Ambusher => AiParams { engage: 0.8, aggression: 0.6, combo: 1, cadence: 60, ..base },
            Archetype::Battery => {
                AiParams { engage: 20.0, aggression: 0.5, dodge_rate: 0.0, guard_rate: 0.0, combo: 1, cadence: 90, w_heavy: 3.0, ..base }
            }
            Archetype::Boss => AiParams { engage: 0.9, aggression: 0.7, dodge_rate: 0.1, guard_rate: 0.1, combo: 1, cadence: 36, ..base },
            Archetype::Dummy => AiParams { aggression: 0.0, dodge_rate: 0.0, guard_rate: 0.0, combo: 0, ..base },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiState {
    Dormant,
    Approach,
    Engage,
    Retreat,
    Ambush,
}

/// What the brain can see of another fighter (delayed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub id: u32,
    pub pos: Vec3,
    pub yaw: f32,
    pub radius: f32,
    pub hp_frac: f32,
    /// Attack in progress: (move id, ticks since start, windup, feint, active, reach).
    pub attack: Option<(String, u32, u16, u16, u16, f32)>,
    pub dodging: bool,
    pub healing: bool,
    pub broken: bool,
    /// Tick the current action began (identifies a specific swing).
    pub act_start: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reaction {
    Dodge,
    Parry,
    Guard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brain {
    pub params: AiParams,
    pub archetype: Archetype,
    pub state: AiState,
    pub target: Option<u32>,
    pub timer: u32,
    pub reaction: u16,
    /// Boss id when scripted.
    pub boss: Option<String>,
    pub strafe: f32,
    pub combo_left: u8,
    pub reacted_to: u64,
    /// Scheduled reaction: (fire at tick, kind, lateral direction sign).
    pub pending: Option<(u64, Reaction, f32)>,
    pub guard_hold: u16,
    pub awake: bool,
    /// Wakes when a hostile comes within this range.
    pub aggro: f32,
}

impl Brain {
    pub fn new(archetype: Archetype, reaction: u16) -> Brain {
        Brain {
            params: AiParams::for_archetype(archetype),
            archetype,
            state: if archetype == Archetype::Ambusher { AiState::Ambush } else { AiState::Dormant },
            target: None,
            timer: 0,
            reaction: reaction.max(6),
            boss: None,
            strafe: 1.0,
            combo_left: 0,
            reacted_to: u64::MAX,
            pending: None,
            guard_hold: 0,
            awake: false,
            aggro: 70.0,
        }
    }
}

/// Inputs for one think step.
pub struct Percept<'a> {
    pub tick: u64,
    pub me_pos: Vec3,
    pub me_yaw: f32,
    pub me_hp: f32,
    pub me_reach: f32,
    pub me_busy: bool,
    pub me_can_act: bool,
    pub me_heals: u8,
    pub me_flying: bool,
    pub target: Option<&'a Seen>,
    /// Number of moves in the boss phase list (scripted bosses).
    pub boss_moves: &'a [(String, u32)],
    pub boss_speed: f32,
    pub has_special: bool,
    pub has_bash: bool,
    pub can_parry: bool,
}

/// Script index sentinel stored in Command::heretic for bosses (we keep Command plain-old-data).
pub struct Think {
    pub cmd: Command,
    /// For scripted bosses: index into the current phase's move list.
    pub script: Option<usize>,
}

pub fn think(b: &mut Brain, p: &Percept, rng: &mut SimRng) -> Think {
    let mut cmd = Command::default();
    let mut script = None;
    b.timer += 1;
    let Some(t) = p.target else {
        b.state = if b.archetype == Archetype::Ambusher { AiState::Ambush } else { AiState::Dormant };
        return Think { cmd, script };
    };
    let to = Vec2::new(t.pos.x - p.me_pos.x, t.pos.z - p.me_pos.z);
    let dist = to.length().max(0.01);
    let dir = to / dist;
    let gap = (dist - t.radius).max(0.0);
    if b.archetype == Archetype::Dummy {
        return Think { cmd, script };
    }
    if !b.awake {
        if dist < b.aggro {
            b.awake = true;
        } else {
            return Think { cmd, script };
        }
    }
    if b.state == AiState::Ambush {
        if gap < 18.0 {
            b.state = AiState::Engage;
            cmd.heavy = true; // erupt
        }
        return Think { cmd, script };
    }

    // Defensive reactions to an observed swing.
    if let Some((_, since, windup, feint, active, reach)) = &t.attack {
        if t.act_start != b.reacted_to && gap < reach + 6.0 {
            b.reacted_to = t.act_start;
            let r = rng.f32();
            let pr = &b.params;
            let reaction = if p.can_parry && r < pr.parry_rate {
                Some(Reaction::Parry)
            } else if r < pr.parry_rate + pr.dodge_rate {
                Some(Reaction::Dodge)
            } else if r < pr.parry_rate + pr.dodge_rate + pr.guard_rate {
                Some(Reaction::Guard)
            } else {
                None
            };
            if let Some(kind) = reaction {
                // When to fire: impatient brains react to the nominal windup and eat feints.
                let patient = rng.chance(b.params.feint_patience);
                let hit_at = *windup as i64 + if patient { *feint as i64 } else { 0 } - *since as i64 - b.reaction as i64;
                let lead = match kind {
                    Reaction::Parry => 4,
                    Reaction::Dodge => 6,
                    Reaction::Guard => 14,
                };
                let at = p.tick + (hit_at - lead).max(0) as u64;
                let lat = if rng.chance(b.params.dodge_left) { -1.0 } else { 1.0 };
                b.pending = Some((at, kind, lat));
                let _ = active;
            }
        }
    }
    if let Some((at, kind, lat)) = b.pending {
        if p.tick >= at {
            b.pending = None;
            match kind {
                Reaction::Parry => cmd.parry = true,
                Reaction::Guard => b.guard_hold = 30,
                Reaction::Dodge => {
                    cmd.dodge = true;
                    let side = Vec2::new(-dir.y, dir.x) * lat;
                    cmd.move_dir = if rng.chance(b.params.dodge_back) { -dir } else { side };
                    return Think { cmd, script };
                }
            }
        }
    }
    if b.guard_hold > 0 {
        b.guard_hold -= 1;
        cmd.guard = true;
    }

    // Heal.
    if p.me_hp < b.params.heal_below && p.me_heals > 0 && gap > 8.0 && !p.me_busy {
        cmd.heal = true;
        return Think { cmd, script };
    }

    let want = (p.me_reach * b.params.engage).max(2.0);
    let speed_k = p.boss_speed.max(0.3);
    // Movement.
    match b.archetype {
        Archetype::Battery => {}
        _ => {
            if gap > want * 1.15 {
                cmd.move_dir = dir * speed_k.min(1.0);
                b.state = AiState::Approach;
            } else if gap < want * 0.5 && b.params.engage > 2.0 {
                cmd.move_dir = -dir;
                b.state = AiState::Retreat;
            } else {
                b.state = AiState::Engage;
                if b.timer % 120 == 0 && rng.chance(0.5) {
                    b.strafe = -b.strafe;
                }
                cmd.move_dir = Vec2::new(-dir.y, dir.x) * b.strafe * 0.35;
            }
        }
    }

    // Offence.
    let in_range = gap < want * 1.25 + 1.0;
    if !p.me_busy && p.me_can_act && in_range && b.timer >= b.params.cadence as u32 {
        if !p.boss_moves.is_empty() {
            if rng.chance(b.params.aggression) {
                let w: Vec<u32> = p.boss_moves.iter().map(|(_, w)| *w).collect();
                script = rng.weighted(&w);
                b.timer = 0;
            }
        } else if b.combo_left > 0 || rng.chance(b.params.aggression) {
            let pr = &b.params;
            let ws = [pr.w_light, pr.w_heavy, if p.has_special { pr.w_special } else { 0.0 }, if p.has_bash { pr.w_bash } else { 0.0 }];
            let wi: Vec<u32> = ws.iter().map(|w| (w * 100.0) as u32).collect();
            match rng.weighted(&wi) {
                Some(0) => cmd.light = true,
                Some(1) => cmd.heavy = true,
                Some(2) => cmd.special = true,
                Some(3) => {
                    cmd.guard = true;
                    cmd.light = true;
                }
                _ => {}
            }
            if b.combo_left == 0 {
                b.combo_left = b.params.combo.saturating_sub(1);
            } else {
                b.combo_left -= 1;
            }
            b.timer = if b.combo_left > 0 { (b.params.cadence as u32).saturating_sub(6) } else { 0 };
        }
    }
    Think { cmd, script }
}
