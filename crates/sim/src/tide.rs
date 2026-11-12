//! The Tide Clock and weather. Hours advance when you rest. High tide and night favour Kaiju;
//! dawn and full grid power favour Saints.

use crate::rng::SimRng;
use isl_data::Side;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Weather {
    Clear,
    FogBank,
    /// Lightning briefly reveals hidden kaiju.
    Storm,
    /// City grid is down: ST. ANCHOR weakened, GOLIATH MOTH loses its lure.
    Blackout,
}

impl Weather {
    pub fn name(self) -> &'static str {
        match self {
            Weather::Clear => "CLEAR",
            Weather::FogBank => "FOG BANK",
            Weather::Storm => "STORM",
            Weather::Blackout => "BLACKOUT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TideClock {
    /// Total hours elapsed this war. Hour of day = (6 + hours) % 24 (wars begin at dawn).
    pub hours: u32,
    pub weather: Weather,
    /// Number of tides elapsed (a tide turns every 6 hours).
    pub tide_count: u32,
    /// Condition overrides.
    #[serde(default)]
    pub eternal_night: bool,
    #[serde(default)]
    pub permanent_blackout: bool,
}

impl Default for TideClock {
    fn default() -> Self {
        TideClock { hours: 0, weather: Weather::Clear, tide_count: 0, eternal_night: false, permanent_blackout: false }
    }
}

impl TideClock {
    pub fn hour_of_day(&self) -> u32 {
        if self.eternal_night {
            return 0;
        }
        (6 + self.hours) % 24
    }
    /// Tide height in -1..1, semi-diurnal (period 12 h), high tide at war start + 3 h.
    pub fn tide(&self) -> f32 {
        let t = (self.hours as f32 - 3.0) / 12.0 * std::f32::consts::TAU;
        t.cos()
    }
    pub fn is_night(&self) -> bool {
        let h = self.hour_of_day();
        !(6..20).contains(&h)
    }
    pub fn is_dawn(&self) -> bool {
        (5..9).contains(&self.hour_of_day()) && !self.eternal_night
    }
    pub fn blackout(&self) -> bool {
        self.permanent_blackout || self.weather == Weather::Blackout
    }
    /// Daylight factor 0..1 for lighting.
    pub fn daylight(&self) -> f32 {
        if self.eternal_night {
            return 0.08;
        }
        // Sunrise ~05:00, sunset ~20:00; dawn is already grey-lit.
        let h = self.hour_of_day() as f32;
        let d = ((h - 4.5) / 16.0 * std::f32::consts::PI).sin();
        (d * 1.1).clamp(0.12, 1.0)
    }

    /// Advance by `n` hours (resting = 1). Rolls weather whenever a tide turns.
    pub fn advance(&mut self, n: u32, rng: &mut SimRng) -> bool {
        let before = self.hours / 6;
        self.hours += n;
        let after = self.hours / 6;
        if after != before {
            self.tide_count += after - before;
            self.weather = match rng.weighted(&[50, 20, 18, 12]).unwrap_or(0) {
                0 => Weather::Clear,
                1 => Weather::FogBank,
                2 => Weather::Storm,
                _ => Weather::Blackout,
            };
            if self.permanent_blackout {
                self.weather = Weather::Blackout;
            }
            return true;
        }
        false
    }

    /// Side advantage multiplier on damage and stamina regen (±10% at extremes).
    pub fn side_bonus(&self, side: Side) -> f32 {
        let tide = self.tide();
        let mut k = 0.0;
        match side {
            Side::Kaiju => {
                if tide > 0.5 {
                    k += 0.05;
                }
                if self.is_night() {
                    k += 0.05;
                }
            }
            Side::Saint => {
                if self.is_dawn() {
                    k += 0.05;
                }
                if !self.blackout() {
                    k += 0.05;
                }
            }
        }
        1.0 + k
    }

    /// Extra water level in metres from the tide.
    pub fn water_offset(&self) -> f32 {
        self.tide() * 1.2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn night_and_tide_favour_kaiju() {
        let mut t = TideClock::default();
        t.hours = 18; // midnight
        assert!(t.is_night());
        assert!(t.side_bonus(Side::Kaiju) > 1.0);
        t.hours = 0; // dawn
        assert!(t.is_dawn());
        assert!(t.side_bonus(Side::Saint) >= 1.1 - 1e-6);
    }
    #[test]
    fn weather_rolls_on_tide_turn_only() {
        let mut t = TideClock::default();
        let mut r = SimRng::new(1);
        assert!(!t.advance(1, &mut r));
        assert!(!t.advance(4, &mut r));
        assert!(t.advance(1, &mut r));
        assert_eq!(t.tide_count, 1);
    }
}
