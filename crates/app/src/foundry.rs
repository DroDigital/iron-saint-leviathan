//! The Foundry (Saint hangar) and the Nest (Kaiju trench): assemble custom builds from harvested
//! parts, with a turntable preview, stat comparison, centre-of-mass gizmo, validation, paint,
//! repairs and the advanced features unlocked by cross-side harvesting.

use isl_data::*;
use isl_sim::build::{self, BuildIssue, Loadout, ResolvedBuild};
use isl_sim::parts::{Condition, PartInstance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AMode {
    Slots,
    Pick,
    Premade,
    Fabricate,
    Paint,
}

pub const EXTRAS: [&str; 11] = [
    "PAINT",
    "LOAD PREMADE",
    "REPAIR PART",
    "RE-ROLL QUIRKS",
    "TRANSPLANT QUIRK",
    "FABRICATE",
    "SAVE TO CARD",
    "EXPORT CODE",
    "TEST ARENA",
    "RENAME",
    "DONE",
];

pub struct Assembly {
    pub side: Side,
    pub lo: Loadout,
    pub sel: usize,
    pub mode: AMode,
    pub list_sel: usize,
    pub rot: f32,
    pub msg: String,
    pub paint_sel: usize,
}

/// Candidate entry in the pick list: (inventory side, index) or None for "empty".
#[derive(Clone, Copy, Debug)]
pub struct Cand {
    pub inv: usize,
    pub idx: usize,
}

pub const PALETTE: [[u8; 3]; 16] = [
    [150, 150, 160],
    [200, 200, 200],
    [90, 92, 100],
    [40, 40, 48],
    [180, 60, 40],
    [120, 40, 30],
    [220, 170, 60],
    [140, 110, 50],
    [60, 110, 140],
    [30, 60, 90],
    [70, 120, 70],
    [40, 70, 50],
    [180, 160, 200],
    [110, 80, 140],
    [230, 230, 220],
    [20, 20, 20],
];

impl Assembly {
    pub fn new(side: Side, lo: Loadout) -> Assembly {
        Assembly { side, lo, sel: 0, mode: AMode::Slots, list_sel: 0, rot: 0.0, msg: String::new(), paint_sel: 0 }
    }
    pub fn slot(&self) -> Option<Slot> {
        Slot::ALL.get(self.sel).copied()
    }
    pub fn rows(&self) -> usize {
        7 + EXTRAS.len()
    }
    pub fn resolved(&self, content: &Content) -> ResolvedBuild {
        build::resolve(&self.lo, content, &[], [false; 7])
    }
    pub fn issues(&self, content: &Content) -> Vec<BuildIssue> {
        build::validate(&self.lo, &self.resolved(content), content)
    }
}

/// Parts that can go into `slot` from both inventories (other anatomy = heretic graft).
pub fn candidates(inv: &[Vec<PartInstance>; 2], content: &Content, slot: Slot) -> Vec<Cand> {
    let mut v = vec![];
    for (s, list) in inv.iter().enumerate() {
        for (i, p) in list.iter().enumerate() {
            if content.part(&p.def).is_some_and(|d| d.slot == slot) {
                v.push(Cand { inv: s, idx: i });
            }
        }
    }
    v
}

pub fn part_label(p: &PartInstance, content: &Content, side: Side) -> String {
    let d = content.part(&p.def);
    let heretic = d.is_some_and(|d| d.side != side);
    let c = match p.condition {
        Condition::Pristine => "P",
        Condition::Scarred => "S",
        Condition::Mangled => "M",
    };
    format!(
        "{}{} [{}]{}",
        if heretic { "*" } else { "" },
        d.map_or(p.def.as_str(), |d| d.name.as_str()),
        c,
        if p.quirks.is_empty() { String::new() } else { format!(" +{}", p.quirks.len()) }
    )
}
