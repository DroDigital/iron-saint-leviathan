//! Static content rules. Each returned error is a human-readable sentence naming the offending id.

use crate::*;
use std::collections::HashSet;

pub type ValidationError = String;

impl Content {
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut e = Vec::new();
        let t = match &self.tuning {
            Some(t) => t,
            None => {
                e.push("tuning.ron missing".into());
                return e;
            }
        };

        // Unique ids.
        fn uniq<'a>(e: &mut Vec<String>, what: &str, ids: impl Iterator<Item = &'a str>) {
            let mut seen = HashSet::new();
            for id in ids {
                if !seen.insert(id) {
                    e.push(format!("duplicate {what} id `{id}`"));
                }
            }
        }
        uniq(&mut e, "part", self.parts.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "move", self.moves.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "quirk", self.quirks.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "chassis", self.chassis.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "enemy", self.enemies.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "boss", self.bosses.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "district", self.districts.iter().map(|x| x.id.as_str()));
        uniq(&mut e, "omen", self.omens.iter().map(|x| x.id.as_str()));

        if self.quirks.iter().filter(|q| !q.special).count() < 30 {
            e.push(format!("design requires at least 30 rollable quirks, found {}", self.quirks.iter().filter(|q| !q.special).count()));
        }

        let min_iframes = *t.dodge_iframes.iter().min().unwrap_or(&8);
        for m in &self.moves {
            if m.windup == 0 && !matches!(m.kind, MoveKind::Eat) {
                e.push(format!("move `{}` has zero windup", m.id));
            }
            if m.active == 0 {
                e.push(format!("move `{}` has zero active frames", m.id));
            }
            if matches!(m.kind, MoveKind::Projectile) && m.projectile.is_none() {
                e.push(format!("projectile move `{}` lacks a projectile block", m.id));
            }
            // No unavoidable damage: a move that can only be avoided with i-frames must have an
            // active window no longer than the shortest dodge's i-frames.
            if m.damage > 0.0 && m.avoid == Avoid::Dodge && m.active > min_iframes {
                e.push(format!("move `{}` is dodge-only but active ({}) > min i-frames ({min_iframes})", m.id, m.active));
            }
            if m.damage > 0.0 && !m.blockable && m.avoid == Avoid::Distance && m.reach + m.radius > 16.0 {
                e.push(format!("unblockable move `{}` reaches too far to outrun", m.id));
            }
        }

        for p in &self.parts {
            for mid in &p.moves {
                if self.mv(mid).is_none() {
                    e.push(format!("part `{}` references unknown move `{mid}`", p.id));
                }
            }
            for q in &p.quirks {
                if self.quirk(q).is_none() {
                    e.push(format!("part `{}` references unknown quirk `{q}`", p.id));
                }
            }
            if p.weight <= 0.0 {
                e.push(format!("part `{}` has non-positive weight", p.id));
            }
            match p.slot {
                Slot::Frame => {
                    if p.heal.is_none() {
                        e.push(format!("frame `{}` lacks a heal kind", p.id));
                    }
                    if p.rig.is_none() {
                        e.push(format!("frame `{}` lacks a rig", p.id));
                    }
                    if p.stats.capacity <= 0.0 || p.stats.hp <= 0.0 {
                        e.push(format!("frame `{}` needs capacity and hp", p.id));
                    }
                }
                Slot::Legs => {
                    if p.locomotion.is_none() {
                        e.push(format!("legs `{}` lack locomotion", p.id));
                    }
                }
                Slot::Core if p.stamina.is_none() => {
                    e.push(format!("core `{}` lacks a stamina kind", p.id));
                }
                _ => {}
            }
            if p.slot != Slot::Legs && p.locomotion.is_some() {
                e.push(format!("part `{}` has locomotion outside the legs slot", p.id));
            }
            if p.slot != Slot::Core && p.stamina.is_some() {
                e.push(format!("part `{}` has a stamina kind outside the core slot", p.id));
            }
            if p.visual.prims.is_empty() {
                e.push(format!("part `{}` has no visual primitives", p.id));
            }
        }

        let check_loadout = |e: &mut Vec<String>, owner: &str, side: Side, parts: &[String; 7], strict_side: bool| {
            for (i, pid) in parts.iter().enumerate() {
                match self.part(pid) {
                    None => e.push(format!("{owner} references unknown part `{pid}`")),
                    Some(p) => {
                        if p.slot != Slot::ALL[i] {
                            e.push(format!("{owner}: part `{pid}` is a {:?} but sits in slot {:?}", p.slot, Slot::ALL[i]));
                        }
                        // Cross-Harvest rule: an enemy's drops are its own parts, so its parts must
                        // be of its own anatomy. That guarantees Saint parts only drop while
                        // playing Kaiju and vice-versa.
                        if strict_side && p.side != side {
                            e.push(format!("{owner} ({side:?}) uses {:?} part `{pid}`, breaking the Cross-Harvest rule", p.side));
                        }
                    }
                }
            }
            // Coarse move set check (the full composer lives in isl_sim::build).
            let mut has_light = false;
            let mut has_heavy = false;
            let mut posture = false;
            for pid in parts.iter() {
                if let Some(p) = self.part(pid) {
                    posture |= p.parry;
                    for m in p.moves.iter().filter_map(|m| self.mv(m)) {
                        has_light |= m.kind == MoveKind::Light;
                        has_heavy |= matches!(m.kind, MoveKind::Heavy);
                        posture |= m.posture > 0.0;
                    }
                }
            }
            if !(has_light && has_heavy && posture) {
                e.push(format!("{owner}: loadout lacks light/heavy/posture moves"));
            }
        };

        for c in &self.chassis {
            check_loadout(&mut e, &format!("chassis `{}`", c.id), c.side, &c.parts, true);
            for alt in &c.alt_parts {
                check_loadout(&mut e, &format!("chassis `{}` alt", c.id), c.side, alt, true);
            }
            match self.chassis_def(&c.rival) {
                Some(r) if r.side == c.side.other() => {}
                _ => e.push(format!("chassis `{}` rival `{}` must be an opposite-side chassis", c.id, c.rival)),
            }
            match &c.unlock {
                Unlock::Always => {}
                Unlock::SeverHeads { of, while_playing } | Unlock::HarvestCores { of, while_playing } => {
                    for id in of {
                        match self.chassis_def(id) {
                            Some(x) if x.side != *while_playing => {}
                            _ => e.push(format!("chassis `{}` unlock references `{id}`, which must be an opposite-side chassis", c.id)),
                        }
                    }
                }
            }
            let mut last = 0;
            for r in &c.ranks {
                if r.rank <= last || r.rank > 10 {
                    e.push(format!("chassis `{}` ranks must ascend within 1..=10", c.id));
                }
                last = r.rank;
            }
        }
        for en in &self.enemies {
            check_loadout(&mut e, &format!("enemy `{}`", en.id), en.side, &en.parts, true);
            if en.reaction < 6 && en.archetype != Archetype::Dummy {
                e.push(format!("enemy `{}` reaction under 100 ms reads as input-reading", en.id));
            }
        }
        for b in &self.bosses {
            check_loadout(&mut e, &format!("boss `{}`", b.id), b.side, &b.parts, true);
            if b.phases.is_empty() || (b.phases[0].at_hp - 1.0).abs() > 1e-3 {
                e.push(format!("boss `{}` first phase must start at 1.0 hp", b.id));
            }
            for w in b.phases.windows(2) {
                if w[1].at_hp >= w[0].at_hp {
                    e.push(format!("boss `{}` phases must descend in hp", b.id));
                }
            }
            for ph in &b.phases {
                for (mid, _) in &ph.moves {
                    match self.mv(mid) {
                        None => e.push(format!("boss `{}` phase uses unknown move `{mid}`", b.id)),
                        Some(m) => {
                            if m.windup < t.min_boss_windup && m.damage > 0.0 {
                                e.push(format!(
                                    "boss `{}` move `{mid}` windup {} < {} ticks (300 ms telegraph rule)",
                                    b.id, m.windup, t.min_boss_windup
                                ));
                            }
                        }
                    }
                }
            }
            if let Some(ch) = &b.chassis
                && self.chassis_def(ch).is_none()
            {
                e.push(format!("boss `{}` chassis `{ch}` unknown", b.id));
            }
        }
        let normal = self.districts.iter().filter(|d| !d.finale).count();
        if normal != 6 {
            e.push(format!("expected 6 war districts, found {normal}"));
        }
        for d in &self.districts {
            for (si, st) in d.states.iter().enumerate() {
                for id in &st.saint_enemies {
                    match self.enemy(id) {
                        Some(en) if en.side == Side::Kaiju => {}
                        _ => e.push(format!("district `{}` state {si}: saint-mission enemy `{id}` must be kaiju-side", d.id)),
                    }
                }
                for id in &st.kaiju_enemies {
                    match self.enemy(id) {
                        Some(en) if en.side == Side::Saint => {}
                        _ => e.push(format!("district `{}` state {si}: kaiju-mission enemy `{id}` must be saint-side", d.id)),
                    }
                }
                if st.fog_near >= st.fog_far {
                    e.push(format!("district `{}` fog near >= far", d.id));
                }
            }
            match self.boss(&d.saint_boss) {
                Some(b) if b.side == Side::Kaiju => {}
                _ => e.push(format!("district `{}` saint_boss must be a kaiju boss", d.id)),
            }
            match self.boss(&d.kaiju_boss) {
                Some(b) if b.side == Side::Saint => {}
                _ => e.push(format!("district `{}` kaiju_boss must be a saint boss", d.id)),
            }
        }
        for c in &self.contracts {
            if self.quirk(&c.reward_quirk).is_none() {
                e.push(format!("contract `{}` reward quirk unknown", c.id));
            }
        }
        for ev in &self.events {
            if let Some((a, b)) = &ev.spawn {
                for (s, want) in [(a, Side::Kaiju), (b, Side::Saint)] {
                    let side = self.enemy(s).map(|x| x.side).or(self.boss(s).map(|x| x.side));
                    if side != Some(want) {
                        e.push(format!("event `{}` spawn `{s}` must be an existing {want:?}-side foe", ev.id));
                    }
                }
            }
            if let Some(q) = &ev.quirk
                && self.quirk(q).is_none()
            {
                e.push(format!("event `{}` quirk unknown", ev.id));
            }
        }
        if self.disc_rot.len() != 10 {
            e.push(format!("Disc Rot needs 10 tiers, found {}", self.disc_rot.len()));
        }
        e
    }
}
