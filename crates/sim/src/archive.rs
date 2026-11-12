//! The Archive: permanent knowledge across all wars and modes. Adds options, breadth and
//! knowledge - never more than +10% raw power in total.

use crate::parts::{Condition, Origin, PartInstance};
use crate::rng::SimRng;
use isl_data::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct License {
    pub xp: u32,
    pub rank: u8,
    /// Wins against this chassis while playing the other side.
    pub wins_against: u32,
}

impl License {
    pub fn xp_for(rank: u8) -> u32 {
        200 * rank as u32 * rank as u32
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WarRecord {
    pub wars: u32,
    pub endings: BTreeSet<String>,
    pub nemeses_defeated: u32,
    pub deaths: u32,
    pub missions: u32,
    pub corrupted_chain: u32,
}

/// Named rival nemeses: THE ROOKIE, THE GRUDGE, THE CHAMPION.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rival {
    pub title: String,
    pub loadout: crate::build::Loadout,
    /// Encounters survived; rivals evolve (hp, aggression) each time.
    pub level: u32,
    pub defeated: u32,
    pub war: u32,
    pub tide: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Archive {
    /// Part Codex: severing a part once registers it.
    pub codex: BTreeSet<String>,
    pub licenses: BTreeMap<String, License>,
    pub record: WarRecord,
    pub bestiary: BTreeSet<String>,
    pub unlocked_chassis: BTreeSet<String>,
    pub unlocked_conditions: BTreeSet<String>,
    /// Heads severed (per chassis tag) while playing Kaiju - ST. NULL unlock.
    pub heads_taken: BTreeSet<String>,
    /// Hearts harvested (per chassis tag) while playing Saint - TIDE-WIDOW unlock.
    pub hearts_taken: BTreeSet<String>,
    pub cross_harvests: u32,
    pub rivals: Vec<Rival>,
    /// Build usage in ticks by loadout hash (THE CHAMPION).
    pub usage: BTreeMap<u64, u64>,
    pub first_build: Option<crate::build::Loadout>,
    pub run_best: u32,
    pub daily_best: BTreeMap<String, u32>,
    pub disc_rot_unlocked: u8,
    pub seen_endings_disc3: bool,
}

pub const REROLL_HARVESTS: u32 = 20;
pub const TRANSPLANT_HARVESTS: u32 = 40;

impl Archive {
    pub fn register(&mut self, def: &str) -> bool {
        self.codex.insert(def.to_string())
    }

    /// Fabricate a Scarred copy of a registered part (bad RNG never blocks a build forever).
    pub fn fabricate_cost(def: &PartDef) -> u32 {
        (def.weight * 18.0) as u32 + 150 * def.complexity as u32
    }

    pub fn fabricate(&self, def: &PartDef, uid: u64, rng: &mut SimRng, content: &Content) -> Option<PartInstance> {
        if !self.codex.contains(&def.id) {
            return None;
        }
        Some(PartInstance::harvest(def, Condition::Scarred, Origin::Fabricated, uid, rng, content))
    }

    pub fn can_reroll(&self) -> bool {
        self.cross_harvests >= REROLL_HARVESTS
    }
    pub fn can_transplant(&self) -> bool {
        self.cross_harvests >= TRANSPLANT_HARVESTS
    }

    /// Add licence XP for the chassis used. Rank 5 additionally needs 3 wins against that chassis
    /// from the other side (you cannot master a side without playing the other).
    pub fn add_xp(&mut self, chassis: &str, xp: u32, content: &Content) -> Vec<String> {
        let mut out = vec![];
        let Some(cd) = content.chassis_def(chassis) else { return out };
        let lic = self.licenses.entry(chassis.to_string()).or_default();
        lic.xp += xp;
        loop {
            let next = lic.rank + 1;
            if next > 10 || lic.xp < License::xp_for(next) {
                break;
            }
            if let Some(r) = cd.ranks.iter().find(|r| r.rank == next) {
                if r.wins_against as u32 > lic.wins_against {
                    break;
                }
                out.push(format!("{} rank {}: {}", cd.name, next, r.reward));
            }
            lic.rank = next;
        }
        out
    }

    /// Permanent bonuses from licence ranks, clamped to the meta cap.
    pub fn meta_effects(&self, chassis: Option<&str>, content: &Content) -> Vec<Effect> {
        let mut v = vec![];
        for (id, lic) in &self.licenses {
            if let Some(cd) = content.chassis_def(id) {
                for r in cd.ranks.iter().filter(|r| r.rank <= lic.rank) {
                    if let Some(b) = &r.bonus {
                        v.push(b.clone());
                    }
                }
                // Signature traits only apply to their own chassis.
                if Some(id.as_str()) == chassis && lic.rank >= 5 {
                    if let Some(e) = &cd.signature_effect {
                        v.push(e.clone());
                    }
                }
            }
        }
        crate::parts::cap_meta_bonuses(&v, content.tuning().meta_cap)
    }

    /// Record mission results for unlock tracking.
    pub fn record_mission(
        &mut self,
        content: &Content,
        side: Side,
        stats: &crate::world::MissionStats,
        harvested: &[PartInstance],
    ) -> Vec<String> {
        let mut out = vec![];
        self.record.missions += 1;
        self.record.deaths += stats.deaths;
        self.record.nemeses_defeated += stats.nemeses_killed.len() as u32;
        for p in harvested {
            if self.register(&p.def) {
                if let Some(d) = content.part(&p.def) {
                    out.push(format!("CODEX: {} registered", d.name));
                }
            }
            if content.part(&p.def).is_some_and(|d| d.side != side) {
                self.cross_harvests += 1;
            }
        }
        for (tag, slot, victim_side) in &stats.severs {
            if *slot == Slot::Head && side == Side::Kaiju && *victim_side == Side::Saint {
                self.heads_taken.insert(tag.clone());
            }
        }
        for (tag, slot) in &stats.harvested {
            if *slot == Slot::Core && side == Side::Saint {
                self.hearts_taken.insert(tag.clone());
            }
        }
        for tag in &stats.beaten_tags {
            if content.chassis_def(tag).is_some_and(|c| c.side != side) {
                self.licenses.entry(tag.clone()).or_default().wins_against += 1;
            }
        }
        // Secret unlocks.
        for c in &content.chassis {
            if self.unlocked_chassis.contains(&c.id) {
                continue;
            }
            let ok = match &c.unlock {
                Unlock::Always => false,
                Unlock::SeverHeads { of, .. } => of.iter().all(|x| self.heads_taken.contains(x)),
                Unlock::HarvestCores { of, .. } => of.iter().all(|x| self.hearts_taken.contains(x)),
            };
            if ok {
                self.unlocked_chassis.insert(c.id.clone());
                out.push(format!("UNLOCKED: {}", c.name));
            }
        }
        if self.cross_harvests == REROLL_HARVESTS {
            out.push("FOUNDRY/NEST: quirk re-roll unlocked".into());
        }
        if self.cross_harvests == TRANSPLANT_HARVESTS {
            out.push("FOUNDRY/NEST: move transplant unlocked".into());
        }
        out
    }

    pub fn chassis_available(&self, c: &ChassisDef) -> bool {
        matches!(c.unlock, Unlock::Always) || self.unlocked_chassis.contains(&c.id)
    }

    pub fn finish_war(&mut self, ending: &str, content: &Content) -> Vec<String> {
        self.record.wars += 1;
        self.record.endings.insert(ending.to_string());
        self.disc_rot_unlocked = (self.disc_rot_unlocked + 1).min(10);
        let mut out = vec![];
        for c in &content.conditions {
            if !self.unlocked_conditions.contains(&c.id)
                && self.record.wars >= c.wars
                && c.endings.iter().all(|e| self.record.endings.contains(e))
            {
                self.unlocked_conditions.insert(c.id.clone());
                out.push(format!("CONDITION UNLOCKED: {}", c.name));
            }
        }
        out
    }

    pub fn note_usage(&mut self, hash: u64, ticks: u64) {
        *self.usage.entry(hash).or_default() += ticks;
    }

    /// Ensure/refresh a named rival.
    pub fn set_rival(&mut self, title: &str, loadout: crate::build::Loadout, war: u32, tide: u32) {
        if let Some(r) = self.rivals.iter_mut().find(|r| r.title == title) {
            r.loadout = loadout;
            r.level += 1;
            r.war = war;
            r.tide = tide;
        } else {
            self.rivals.push(Rival { title: title.to_string(), loadout, level: 0, defeated: 0, war, tide });
        }
    }
}

/// Daily Tide seed generated offline from the calendar date (comparable with friends by seed).
pub fn daily_seed(year: i32, month: u32, day: u32) -> u64 {
    crate::rng::hash_str(&format!("HARROWMOUTH-{year:04}-{month:02}-{day:02}"))
}

/// Civil date from days since 1970-01-01 (no chrono dependency).
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((y + if m <= 2 { 1 } else { 0 }) as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_404), (2025, 11, 12));
        assert_ne!(daily_seed(2026, 11, 12), daily_seed(2026, 11, 13));
        assert_eq!(daily_seed(2026, 11, 12), daily_seed(2026, 11, 12));
    }
}
