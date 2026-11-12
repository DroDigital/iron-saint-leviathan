//! The persistent war over Harrowmouth: district control, the Tide Clock, wrecks, inventories,
//! events, contracts and endings.

use crate::build::Loadout;
use crate::parts::{Condition, Origin, PartInstance};
use crate::rng::SimRng;
use crate::tide::TideClock;
use crate::world::{MissionStats, Wreck};
use isl_data::*;
use serde::{Deserialize, Serialize};

/// Control ranges from -3 (Kaiju) to +3 (Saint).
pub const CONTROL_MAX: i8 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DistrictWar {
    pub id: String,
    pub control: i8,
    /// Persistent building damage mask per building.
    pub damage: Vec<Vec<bool>>,
    /// Destruction fraction 0..1 (>= 0.6 => Ruined).
    pub ruin: f32,
    pub contract: Option<String>,
    pub contract_progress: u32,
}

impl DistrictWar {
    /// District state derives from control and destruction:
    /// Fortified (control >= 2), Flooded (control <= -2), Contested otherwise; heavy destruction
    /// overrides everything with Ruined, which neither side holds.
    pub fn state(&self) -> DistrictState {
        if self.ruin >= 0.6 {
            DistrictState::Ruined
        } else if self.control >= 2 {
            DistrictState::Fortified
        } else if self.control <= -2 {
            DistrictState::Flooded
        } else {
            DistrictState::Contested
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Inventory {
    pub parts: Vec<PartInstance>,
    pub currency: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingEvent {
    pub id: String,
    pub district: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    pub war: u32,
    pub seed: u64,
    pub rng: SimRng,
    pub districts: Vec<DistrictWar>,
    pub tide: TideClock,
    /// Indexed by side: [Saint, Kaiju].
    pub inventory: [Inventory; 2],
    pub active: [Loadout; 2],
    pub wrecks: Vec<Wreck>,
    pub missions: [u32; 2],
    pub events: Vec<PendingEvent>,
    pub condition: Option<String>,
    pub disc_rot: u8,
    pub omens: Vec<String>,
    pub finished: Option<String>,
    pub last_side: Side,
    pub next_uid: u64,
    /// True once the Breakwater has opened.
    pub breakwater_open: bool,
    pub log: Vec<String>,
}

pub fn side_ix(s: Side) -> usize {
    match s {
        Side::Saint => 0,
        Side::Kaiju => 1,
    }
}

impl Campaign {
    pub fn new(content: &Content, war: u32, seed: u64, condition: Option<&ConditionDef>, saint: Loadout, kaiju: Loadout) -> Campaign {
        let mut districts: Vec<DistrictWar> = content
            .districts
            .iter()
            .map(|d| DistrictWar {
                id: d.id.clone(),
                control: match d.start {
                    DistrictState::Fortified => 2,
                    DistrictState::Flooded => -2,
                    _ => 0,
                },
                damage: vec![],
                ruin: 0.0,
                contract: None,
                contract_progress: 0,
            })
            .collect();
        let mut tide = TideClock::default();
        if let Some(c) = condition {
            match c.kind {
                ConditionKind::HarborFlooded => {
                    if let Some(h) = districts.iter_mut().find(|d| d.id == "harbor") {
                        h.control = -2;
                    }
                }
                ConditionKind::AllContested => {
                    for d in districts.iter_mut() {
                        d.control = 0;
                    }
                }
                ConditionKind::Blackout => tide.permanent_blackout = true,
                ConditionKind::EternalNight => tide.eternal_night = true,
                ConditionKind::OneSaintPerSide => {}
            }
        }
        let mut c = Campaign {
            war,
            seed,
            rng: SimRng::new(seed),
            districts,
            tide,
            inventory: [Inventory::default(), Inventory::default()],
            active: [saint, kaiju],
            wrecks: vec![],
            missions: [0, 0],
            events: vec![],
            condition: condition.map(|c| c.id.clone()),
            disc_rot: 0,
            omens: vec![],
            finished: None,
            last_side: Side::Saint,
            next_uid: 1 << 32 | (war as u64) << 48,
            breakwater_open: false,
            log: vec![format!("WAR {war} BEGINS.")],
        };
        c.roll_contracts(content);
        c
    }

    pub fn uid(&mut self) -> u64 {
        self.next_uid += 16;
        self.next_uid
    }

    pub fn district(&self, id: &str) -> Option<&DistrictWar> {
        self.districts.iter().find(|d| d.id == id)
    }
    pub fn district_mut(&mut self, id: &str) -> Option<&mut DistrictWar> {
        self.districts.iter_mut().find(|d| d.id == id)
    }

    /// The six war districts (excluding the Breakwater finale).
    pub fn war_districts<'a>(&'a self, content: &'a Content) -> impl Iterator<Item = &'a DistrictWar> {
        self.districts.iter().filter(move |d| content.district(&d.id).is_some_and(|x| !x.finale))
    }

    /// Map balance: positive favours Saints.
    pub fn balance(&self, content: &Content) -> i32 {
        self.war_districts(content).map(|d| if d.state() == DistrictState::Ruined { 0 } else { d.control as i32 }).sum()
    }

    pub fn count_state(&self, content: &Content, s: DistrictState) -> usize {
        self.war_districts(content).filter(|d| d.state() == s).count()
    }

    /// Apply a mission result. Saint wins push toward Fortified, Kaiju wins toward Flooded.
    pub fn apply_result(
        &mut self,
        content: &Content,
        district: &str,
        side: Side,
        won: bool,
        stats: &MissionStats,
        ruin: f32,
        damage: Vec<Vec<bool>>,
    ) {
        let delta: i8 = match (side, won) {
            (Side::Saint, true) => 1,
            (Side::Kaiju, true) => -1,
            _ => 0,
        };
        let name = content.district(district).map_or(district.to_string(), |d| d.name.clone());
        if let Some(d) = self.district_mut(district) {
            d.control = (d.control + delta).clamp(-CONTROL_MAX, CONTROL_MAX);
            d.damage = damage;
            d.ruin = ruin;
            // Saint victories rebuild a little; kaiju victories reclaim rubble for the sea.
            if won {
                d.ruin = (d.ruin - 0.12).max(0.0);
                if d.ruin < 0.6 {
                    // Rebuilding clears part of the persistent damage mask.
                    for b in d.damage.iter_mut() {
                        for (i, x) in b.iter_mut().enumerate() {
                            if *x && i % 3 == 0 {
                                *x = false;
                            }
                        }
                    }
                }
            }
        }
        if won {
            self.missions[side_ix(side)] += 1;
            self.log.push(format!("{} took {} (tide {}).", side.name(), name, self.tide.tide_count));
        } else {
            self.log.push(format!("{} retreated from {}.", side.name(), name));
        }
        let _ = stats;
        let total = self.missions[0] + self.missions[1];
        if total >= 8 && !self.breakwater_open {
            self.breakwater_open = true;
            self.log.push("THE BREAKWATER IS OPEN.".into());
        }
    }

    /// Store harvested parts into the inventory of the side that will *use* them: every part goes
    /// to the inventory for its own anatomy (Saint parts -> Saint inventory). Since Saint parts only
    /// drop while playing Kaiju, this is what makes the two sides depend on each other.
    pub fn store_parts(&mut self, parts: Vec<PartInstance>, content: &Content) {
        for p in parts {
            let side = content.part(&p.def).map_or(Side::Saint, |d| d.side);
            self.inventory[side_ix(side)].parts.push(p);
        }
    }

    /// Roll a random war event after a mission.
    pub fn roll_event(&mut self, content: &Content) -> Option<EventDef> {
        for ev in &content.events {
            if self.rng.chance(ev.chance) {
                let ids: Vec<String> = self.war_districts(content).map(|d| d.id.clone()).collect();
                let district = match ev.kind {
                    EventKind::CathedralBell => "cathedral_row".to_string(),
                    _ => self.rng.pick(&ids).cloned().unwrap_or_else(|| "harbor".into()),
                };
                if ev.kind == EventKind::Blackout {
                    self.tide.weather = crate::tide::Weather::Blackout;
                }
                self.events.push(PendingEvent { id: ev.id.clone(), district });
                self.log.push(format!("EVENT: {}", ev.name));
                return Some(ev.clone());
            }
        }
        None
    }

    pub fn take_event(&mut self, district: &str) -> Vec<String> {
        let (now, keep): (Vec<_>, Vec<_>) = self.events.drain(..).partition(|e| e.district == district);
        self.events = keep;
        now.into_iter().map(|e| e.id).collect()
    }

    pub fn roll_contracts(&mut self, content: &Content) {
        let ids: Vec<String> = content.contracts.iter().map(|c| c.id.clone()).collect();
        for i in 0..self.districts.len() {
            let pick = self.rng.pick(&ids).cloned();
            self.districts[i].contract = pick;
            self.districts[i].contract_progress = 0;
        }
    }

    /// Evaluate a district contract against mission stats. Returns the reward quirk on success.
    pub fn check_contract(
        &mut self,
        content: &Content,
        district: &str,
        side: Side,
        stats: &MissionStats,
        won: bool,
    ) -> Option<(ContractDef, PartInstance)> {
        let cid = self.district(district)?.contract.clone()?;
        let c = content.contracts.iter().find(|c| c.id == cid)?.clone();
        if c.side.is_some_and(|s| s != side) {
            return None;
        }
        let done = match c.goal {
            ContractGoal::SeverSlot(slot, n) => stats.severs.iter().filter(|(_, s, _)| *s == slot).count() >= n as usize,
            ContractGoal::NoHeal => won && stats.heals == 0,
            ContractGoal::NoDamage => won && stats.damage_taken <= 0.0,
            ContractGoal::KillCount(n) => stats.kills >= n as u32,
            ContractGoal::Parries(n) => stats.parries >= n as u32,
            ContractGoal::UnderTime(t) => won && stats.ticks <= t as u64,
            ContractGoal::EatBuildings(n) => stats.eaten >= n as u32,
        };
        if !done {
            return None;
        }
        // Reward: a random opposite-anatomy part carrying the rare quirk (it is harvested loot).
        let pool: Vec<&PartDef> = content.parts.iter().filter(|p| p.side != side && matches!(p.source, Source::Chassis(_))).collect();
        let def = (*self.rng.pick(&pool)?).clone();
        let uid = self.uid();
        let mut inst = PartInstance::harvest(&def, Condition::Scarred, Origin::Contract(c.id.clone()), uid, &mut self.rng, content);
        if !inst.quirks.contains(&c.reward_quirk) {
            inst.quirks.push(c.reward_quirk.clone());
        }
        self.inventory[side_ix(side)].currency += c.reward_currency;
        if let Some(d) = self.district_mut(district) {
            d.contract = None;
        }
        Some((c, inst))
    }

    /// Which finales are reachable at the Breakwater right now, given the map and the build.
    pub fn available_endings(&self, content: &Content, side: Side, heretic_frac: f32) -> Vec<&'static str> {
        let mut v = vec![];
        if !self.breakwater_open {
            return v;
        }
        let fort = self.count_state(content, DistrictState::Fortified);
        let flood = self.count_state(content, DistrictState::Flooded);
        if side == Side::Kaiju && flood >= 4 {
            v.push("drown_the_city");
        }
        if side == Side::Saint && fort >= 4 {
            v.push("the_city_stands");
        }
        if side == Side::Kaiju && heretic_frac > 0.5 {
            v.push("become_the_saint");
        }
        if self.true_balance(content) {
            v.push("tide_between");
        }
        v
    }

    /// The Tide Between: the map balanced at exactly 50/50 - equal Fortified and Flooded districts
    /// (at least two each) and total control summing to zero - after both sides have won at least
    /// four missions. The design said only "exactly 50/50", which the starting map already
    /// satisfies; the mission requirement makes it something you achieve.
    pub fn true_balance(&self, content: &Content) -> bool {
        let fort = self.count_state(content, DistrictState::Fortified);
        let flood = self.count_state(content, DistrictState::Flooded);
        fort == flood && fort >= 2 && self.balance(content) == 0 && self.missions[0] >= 4 && self.missions[1] >= 4
    }

    /// Resting at a checkpoint advances the tide clock by one hour.
    pub fn rest(&mut self) -> bool {
        let mut r = self.rng.fork(self.tide.hours as u64);
        self.tide.advance(1, &mut r)
    }

    /// Wrecks lying in a district (both sides).
    pub fn wrecks_in(&self, district: &str) -> Vec<Wreck> {
        self.wrecks.iter().filter(|w| w.district == district).cloned().collect()
    }

    pub fn merge_wrecks(&mut self, district: &str, wrecks: Vec<Wreck>) {
        self.wrecks.retain(|w| w.district != district);
        // Keep the 6 most recent per district; harvested & empty wrecks fade after a while.
        let mut ws: Vec<Wreck> = wrecks.into_iter().filter(|w| !(w.harvested && w.currency == 0)).collect();
        if ws.len() > 6 {
            ws.drain(0..ws.len() - 6);
        }
        self.wrecks.extend(ws);
    }

    /// Ending consequences on persistent collections.
    pub fn apply_ending_rot(ending: &str, parts: &mut [PartInstance], content: &Content) {
        let (side, q) = match ending {
            "drown_the_city" => (Side::Saint, "rusted"),
            "the_city_stands" => (Side::Kaiju, "crumbling"),
            _ => return,
        };
        for p in parts.iter_mut() {
            if content.part(&p.def).is_some_and(|d| d.side == side) && !p.quirks.iter().any(|x| x == q) {
                p.quirks.push(q.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn district_state_rules() {
        let mut d = DistrictWar { id: "x".into(), control: 0, damage: vec![], ruin: 0.0, contract: None, contract_progress: 0 };
        assert_eq!(d.state(), DistrictState::Contested);
        d.control = 2;
        assert_eq!(d.state(), DistrictState::Fortified);
        d.control = -3;
        assert_eq!(d.state(), DistrictState::Flooded);
        d.ruin = 0.7;
        assert_eq!(d.state(), DistrictState::Ruined);
    }
}
