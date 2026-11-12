//! The six stamina models. Saint reactors: Heat, Battery, Fuel. Kaiju hearts: Hunger, Blood, Breath.
//!
//! All models expose the same contract: `value` is usable stamina in `0..=max`. An action may
//! start whenever `value > 0` (soulslike leniency); Heat instead lets you push past the limit and
//! then forces a vent stagger.

use isl_data::StaminaKind;
use serde::{Deserialize, Serialize};

pub const DT: f32 = 1.0 / 60.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamina {
    pub kind: StaminaKind,
    pub value: f32,
    pub max: f32,
    pub regen: f32,
    /// Seconds since last spend.
    pub since_spend: f32,
    /// Fuel: finite reserve for the sortie.
    pub reserve: f32,
    pub reserve_max: f32,
    /// Hunger: food meter 0..1. Starving slows regen.
    pub food: f32,
    /// Breath: phase 0..1 (0..0.5 inhale, 0.5..1 exhale).
    pub breath: f32,
    /// Set when Heat overheats; consumer turns it into a vent stagger.
    pub overheated: bool,
}

pub struct StaminaCtx {
    /// Multiplier from grid/blackout/tide/omens.
    pub regen_mul: f32,
    pub idle: bool,
    pub idle_drain: f32,
    pub famine: f32,
}

impl Default for StaminaCtx {
    fn default() -> Self {
        StaminaCtx { regen_mul: 1.0, idle: false, idle_drain: 0.0, famine: 1.0 }
    }
}

impl Stamina {
    pub fn new(kind: StaminaKind, max: f32, regen: f32) -> Self {
        Stamina {
            kind,
            value: max,
            max,
            regen,
            since_spend: 10.0,
            reserve: max * 12.0,
            reserve_max: max * 12.0,
            food: 1.0,
            breath: 0.0,
            overheated: false,
        }
    }

    pub fn can_act(&self) -> bool {
        match self.kind {
            StaminaKind::Heat => !self.overheated,
            _ => self.value > 0.0,
        }
    }

    pub fn exhaling(&self) -> bool {
        self.kind == StaminaKind::Breath && self.breath >= 0.5
    }

    /// Effective cost of an action right now.
    pub fn cost(&self, base: f32) -> f32 {
        match self.kind {
            // Attacks timed to the exhale cost half.
            StaminaKind::Breath if self.exhaling() => base * 0.5,
            StaminaKind::Breath => base * 1.1,
            _ => base,
        }
    }

    /// Spend stamina. Returns the amount actually charged.
    pub fn spend(&mut self, base: f32) -> f32 {
        let c = self.cost(base);
        self.value -= c;
        self.since_spend = 0.0;
        if self.kind == StaminaKind::Heat && self.value < 0.0 {
            self.overheated = true;
        }
        self.value = self.value.max(match self.kind {
            StaminaKind::Heat => -self.max * 0.25,
            _ => 0.0,
        });
        c
    }

    /// Vent finished: Heat resets to 40% headroom.
    pub fn vent(&mut self) {
        self.overheated = false;
        self.value = self.max * 0.4;
    }

    pub fn update(&mut self, ctx: &StaminaCtx) {
        self.since_spend += DT;
        let r = self.regen * ctx.regen_mul;
        match self.kind {
            StaminaKind::Heat => {
                // Heat dissipates constantly; faster when not acting.
                if !self.overheated {
                    let k = if self.since_spend > 0.5 { 1.0 } else { 0.55 };
                    self.value += r * k * DT;
                }
            }
            StaminaKind::Battery => {
                // Recharges only after a pause, then quickly.
                if self.since_spend > 0.9 {
                    self.value += r * 1.6 * DT;
                }
            }
            StaminaKind::Fuel => {
                // Working tank refills from the finite reserve; sputters when the reserve runs dry
                // (never zero, so a long fight is never a soft-lock).
                if self.since_spend > 0.35 {
                    let want = (self.max - self.value).max(0.0).min(r * DT);
                    let from_reserve = want.min(self.reserve);
                    self.reserve -= from_reserve;
                    self.value += from_reserve + (want - from_reserve) * 0.25;
                }
            }
            StaminaKind::Hunger => {
                // Hunger drains slowly with time and activity; starving halves regen.
                let drain = 0.006 * ctx.famine * if self.since_spend < 1.0 { 2.0 } else { 1.0 };
                self.food = (self.food - drain * DT).max(0.0);
                let k = if self.food > 0.0 { 1.0 } else { 0.4 };
                if self.since_spend > 0.5 {
                    self.value += r * k * DT;
                }
            }
            StaminaKind::Blood => {
                // Slow natural regen; damage dealt is the real refill.
                if self.since_spend > 0.6 {
                    self.value += r * 0.45 * DT;
                }
            }
            StaminaKind::Breath => {
                self.breath = (self.breath + DT / 3.0) % 1.0;
                if self.since_spend > 0.4 {
                    let k = if self.exhaling() { 0.6 } else { 1.3 };
                    self.value += r * k * DT;
                }
            }
        }
        if ctx.idle && ctx.idle_drain > 0.0 && self.since_spend > 2.0 {
            self.value -= ctx.idle_drain * DT * 3.0;
        }
        self.value = self.value.min(self.max).max(match self.kind {
            StaminaKind::Heat => -self.max * 0.25,
            _ => 0.0,
        });
    }

    pub fn on_damage_dealt(&mut self, dmg: f32) {
        if self.kind == StaminaKind::Blood {
            self.value = (self.value + dmg * 0.6).min(self.max);
        }
    }

    pub fn on_eat(&mut self, food: f32) {
        self.food = (self.food + food).min(1.0);
        if self.kind == StaminaKind::Hunger {
            self.value = (self.value + self.max * food * 0.8).min(self.max);
        }
    }

    /// Resting at a Hangar/Nest.
    pub fn refill(&mut self) {
        self.value = self.max;
        self.reserve = self.reserve_max;
        self.food = 1.0;
        self.overheated = false;
    }

    /// Bar fraction for the HUD (0..1).
    pub fn frac(&self) -> f32 {
        (self.value / self.max).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tick(s: &mut Stamina, n: usize) {
        for _ in 0..n {
            s.update(&StaminaCtx::default());
        }
    }
    #[test]
    fn heat_overheats_and_vents() {
        let mut s = Stamina::new(StaminaKind::Heat, 100.0, 40.0);
        s.spend(60.0);
        assert!(s.can_act());
        s.spend(60.0);
        assert!(s.overheated && !s.can_act());
        tick(&mut s, 120);
        assert!(s.overheated, "heat never cools on its own once overheated; the vent does it");
        s.vent();
        assert!(s.can_act());
        assert!((s.value - 40.0).abs() < 1e-3);
    }
    #[test]
    fn battery_waits_for_pause() {
        let mut s = Stamina::new(StaminaKind::Battery, 100.0, 50.0);
        s.spend(50.0);
        tick(&mut s, 30);
        assert_eq!(s.value, 50.0);
        tick(&mut s, 60);
        assert!(s.value > 50.0);
    }
    #[test]
    fn fuel_reserve_is_finite_but_never_softlocks() {
        let mut s = Stamina::new(StaminaKind::Fuel, 100.0, 60.0);
        s.reserve = 10.0;
        s.spend(100.0);
        tick(&mut s, 600);
        assert_eq!(s.reserve, 0.0);
        assert!(s.value > 10.0, "sputtering still regenerates");
        s.refill();
        assert_eq!(s.reserve, s.reserve_max);
    }
    #[test]
    fn breath_exhale_discount() {
        let mut s = Stamina::new(StaminaKind::Breath, 100.0, 40.0);
        s.breath = 0.75;
        assert_eq!(s.cost(20.0), 10.0);
        s.breath = 0.1;
        assert!(s.cost(20.0) > 20.0);
    }
    #[test]
    fn blood_refills_on_damage_and_hunger_on_eat() {
        let mut b = Stamina::new(StaminaKind::Blood, 100.0, 30.0);
        b.spend(80.0);
        b.on_damage_dealt(50.0);
        assert!((b.value - 50.0).abs() < 1e-3);
        let mut h = Stamina::new(StaminaKind::Hunger, 100.0, 30.0);
        h.food = 0.0;
        h.spend(100.0);
        h.on_eat(0.5);
        assert!(h.value >= 40.0 && h.food == 0.5);
    }
}
