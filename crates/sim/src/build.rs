//! Loadouts and build resolution: weight and balance, move set composition and validation,
//! heretic grafts (Sync / Machine-blood), block cost.

use crate::parts::{Effects, PartInstance};
use glam::Vec2;
use isl_data::*;
use serde::{Deserialize, Serialize};

/// A Saint or Kaiju as assembled in the Foundry / Nest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub name: String,
    /// The side this build is *played* as. Parts of the other anatomy are heretic grafts.
    pub side: Side,
    /// Premade chassis id, if this loadout is an untouched premade.
    #[serde(default)]
    pub chassis: Option<String>,
    pub slots: [Option<PartInstance>; 7],
    pub paint: [[u8; 3]; 2],
    /// Decal / scar pattern index.
    #[serde(default)]
    pub decal: u8,
    /// Stencilled name (Saints) or bioluminescent marking name (Kaiju).
    #[serde(default)]
    pub stencil: String,
}

impl Loadout {
    pub fn premade(c: &ChassisDef, content: &Content, uid_base: u64) -> Loadout {
        Self::premade_with(c, &c.parts, content, uid_base)
    }
    pub fn premade_with(c: &ChassisDef, parts: &[String; 7], content: &Content, uid_base: u64) -> Loadout {
        let slots = std::array::from_fn(|i| content.part(&parts[i]).map(|d| PartInstance::premade(d, uid_base + i as u64, &c.id)));
        Loadout {
            name: c.name.clone(),
            side: c.side,
            chassis: Some(c.id.clone()),
            slots,
            paint: c.palette,
            decal: 0,
            stencil: String::new(),
        }
    }
    pub fn get(&self, s: Slot) -> Option<&PartInstance> {
        self.slots[s.index()].as_ref()
    }
    /// Stable identity hash of the parts (used for build usage stats / Champion detection).
    pub fn hash(&self) -> u64 {
        let mut s = String::new();
        for p in self.slots.iter().flatten() {
            s.push_str(&p.def);
            s.push('|');
        }
        crate::rng::hash_str(&s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeightClass {
    Light,
    Medium,
    Heavy,
}

impl WeightClass {
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn dodge_name(self) -> &'static str {
        match self {
            WeightClass::Light => "DASH",
            WeightClass::Medium => "HOP",
            WeightClass::Heavy => "SHOULDER-SLIDE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRef {
    pub id: String,
    pub slot: Slot,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MoveSet {
    pub light: Option<MoveRef>,
    pub heavy: Option<MoveRef>,
    pub bash: Option<MoveRef>,
    pub special: Option<MoveRef>,
    pub eat: Option<MoveRef>,
    pub heretic: Option<MoveRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPart {
    pub def: String,
    pub hp: f32,
    pub armor: f32,
    pub power: f32,
    pub impact: f32,
    pub heretic: bool,
    pub owner_tag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedBuild {
    pub side: Side,
    pub rig: RigKind,
    pub size: f32,
    pub parts: [Option<ResolvedPart>; 7],
    pub hp_max: f32,
    pub armor: f32,
    pub posture_max: f32,
    pub capacity: f32,
    pub weight: f32,
    pub load: f32,
    pub class: WeightClass,
    pub overloaded: bool,
    /// Centre of mass offset (x = right, y = forward), metres.
    pub com: Vec2,
    /// Visual lean in radians (positive leans right).
    pub lean: f32,
    pub loco: LocoKind,
    pub speed: f32,
    pub swim: f32,
    pub deep_water: bool,
    pub climb: bool,
    pub flying: bool,
    pub turn_speed: f32,
    pub stamina_kind: StaminaKind,
    pub stamina_max: f32,
    pub stamina_regen: f32,
    pub heal: HealKind,
    pub lock_range: f32,
    pub reveal: bool,
    pub fog_vision: f32,
    pub guard: f32,
    pub can_parry: bool,
    pub moves: MoveSet,
    pub heretic_count: u8,
    pub filled: u8,
    /// Saint stability, 1.0 = perfect sync.
    pub sync: f32,
    /// Kaiju machine-blood fraction.
    pub machine_blood: f32,
    pub heretic_form: bool,
    pub fx: Effects,
    pub block_cost: u8,
    pub radius: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildIssue {
    MissingSlot(Slot),
    UnknownPart(Slot),
    NoLight,
    NoHeavy,
    NoDodge,
    NoHeal,
    NoPostureDamage,
}

impl BuildIssue {
    pub fn text(self) -> String {
        match self {
            BuildIssue::MissingSlot(s) => format!("Empty slot: {s:?}"),
            BuildIssue::UnknownPart(s) => format!("Unknown part in {s:?}"),
            BuildIssue::NoLight => "No light attack".into(),
            BuildIssue::NoHeavy => "No heavy attack".into(),
            BuildIssue::NoDodge => "No dodge (locomotion too slow)".into(),
            BuildIssue::NoHeal => "No way to heal".into(),
            BuildIssue::NoPostureDamage => "No posture damage or parry".into(),
        }
    }
}

/// Body collision dimensions per rig at size 1.0 (radius, height).
pub fn rig_dims(rig: RigKind) -> (f32, f32) {
    match rig {
        RigKind::Humanoid => (2.2, 10.0),
        RigKind::Serpent => (2.8, 4.0),
        RigKind::Moth => (2.8, 9.0),
        RigKind::Swarm => (3.2, 3.6),
        RigKind::Burrower => (3.4, 4.2),
        RigKind::Quadruped => (2.4, 5.0),
        RigKind::Vehicle => (2.6, 2.8),
        RigKind::Drone => (1.6, 1.6),
        RigKind::Turret => (2.4, 4.2),
    }
}

fn first_move<'a>(content: &'a Content, slots: &[Option<(&'a PartDef, bool)>; 7], order: &[Slot], kinds: &[MoveKind]) -> Option<MoveRef> {
    for s in order {
        if let Some((d, _)) = slots[s.index()] {
            for m in &d.moves {
                if let Some(md) = content.mv(m)
                    && kinds.contains(&md.kind)
                {
                    return Some(MoveRef { id: m.clone(), slot: *s });
                }
            }
        }
    }
    None
}

/// Resolve a loadout into concrete numbers. `destroyed` marks parts lost mid-mission (their moves
/// and stats vanish until repaired/regrown). `extra` are omen/trait/meta effects applied on top.
pub fn resolve(lo: &Loadout, content: &Content, extra: &[Effect], destroyed: [bool; 7]) -> ResolvedBuild {
    let mut fx = Effects::default();
    for e in extra {
        fx.add(e, None);
    }
    // Gather defs.
    let mut defs: [Option<(&PartDef, bool)>; 7] = [None; 7];
    let mut heretic_count = 0u8;
    let mut filled = 0u8;
    let mut complexity = 0u32;
    let mut quirk_count = 0u32;
    for (i, inst) in lo.slots.iter().enumerate() {
        if let Some(inst) = inst
            && let Some(d) = content.part(&inst.def)
        {
            filled += 1;
            complexity += d.complexity as u32;
            let heretic = d.side != lo.side;
            if heretic {
                heretic_count += 1;
            }
            for q in &inst.quirks {
                if let Some(qd) = content.quirk(q) {
                    quirk_count += 1;
                    for e in &qd.effects {
                        fx.add(e, inst.origin.owner_tag());
                    }
                }
            }
            if !destroyed[i] {
                defs[i] = Some((d, heretic));
            }
        }
    }
    let heretic_form = heretic_count as f32 > filled.max(1) as f32 * 0.5;
    if heretic_form && let Some(q) = content.quirk("heretic_instability") {
        for e in &q.effects {
            fx.add(e, None);
        }
    }
    let sync = (1.0 - heretic_count as f32 * 0.14).clamp(0.0, 1.0);
    let machine_blood = if lo.side == Side::Kaiju { heretic_count as f32 / 7.0 } else { 0.0 };
    if lo.side == Side::Saint {
        // Low sync: HUD glitches, control drift, hallucinated enemies.
        let loss = 1.0 - sync;
        fx.drift += loss * 0.25;
        fx.hallucinate += loss * 0.3;
    } else if machine_blood > 0.0 {
        // Machine-blooded parts cannot regrow.
        fx.no_regrow |= machine_blood >= 0.5;
    }

    let cond = |i: usize| lo.slots[i].as_ref().map_or(1.0, |p| p.condition.stat_mul());
    let parts: [Option<ResolvedPart>; 7] = std::array::from_fn(|i| {
        let inst = lo.slots[i].as_ref()?;
        let (d, heretic) = defs[i]?;
        let c = cond(i);
        Some(ResolvedPart {
            def: d.id.clone(),
            hp: d.stats.hp.max(40.0) * c,
            armor: d.stats.armor * c * fx.get(Stat::Armor),
            power: if d.stats.power > 0.0 { d.stats.power * c } else { c },
            impact: if d.stats.impact > 0.0 { d.stats.impact * c } else { c },
            heretic,
            owner_tag: inst.origin.owner_tag().map(|s| s.to_string()),
        })
    });

    let frame = defs[0].map(|(d, _)| d);
    let rig = frame.and_then(|f| f.rig).unwrap_or(RigKind::Humanoid);
    let size = frame.map_or(1.0, |f| if f.stats.size > 0.0 { f.stats.size } else { 1.0 });
    let capacity = frame.map_or(60.0, |f| f.stats.capacity.max(1.0));

    // Weight and centre of mass.
    let mut weight = 0.0;
    let mut moment = Vec2::ZERO;
    let mut carried = 0.0;
    for (i, d) in defs.iter().enumerate() {
        if let Some((d, _)) = d {
            let w = d.weight * fx.get(Stat::Weight);
            weight += w;
            if i != 0 {
                carried += w;
            }
            let (ox, oz) = Slot::ALL[i].mass_offset();
            moment += Vec2::new(ox, oz) * w;
        }
    }
    let com = if weight > 0.0 { moment / weight } else { Vec2::ZERO };
    let load = carried / capacity;
    let class = if load < 0.55 {
        WeightClass::Light
    } else if load < 0.82 {
        WeightClass::Medium
    } else {
        WeightClass::Heavy
    };
    let overloaded = load > 1.0;
    let lean = (com.x * 0.35).clamp(-0.35, 0.35);

    let legs = defs[Slot::Legs.index()].and_then(|(d, _)| d.locomotion.clone());
    let (loco, base_speed, swim, deep_water, climb) = match &legs {
        Some(l) => (l.kind, l.speed, l.swim, l.deep_water, l.climb),
        None => (LocoKind::Biped, 2.5, 0.4, false, false),
    };
    let load_pen = if overloaded { 0.65 } else { 1.0 - (load - 0.5).max(0.0) * 0.35 };
    let speed = base_speed * load_pen * fx.get(Stat::Speed) * fx.get(Stat::LandSpeed);
    let turn_speed = (std::f32::consts::PI * 2.6 * (1.25 - load * 0.5).clamp(0.45, 1.25)) * fx.get(Stat::TurnSpeed);

    let core = defs[Slot::Core.index()].map(|(d, _)| d);
    let stamina_kind = core.and_then(|c| c.stamina).unwrap_or(match lo.side {
        Side::Saint => StaminaKind::Battery,
        Side::Kaiju => StaminaKind::Hunger,
    });
    let stamina_max = core.map_or(70.0, |c| c.stats.stamina.max(40.0)) * fx.get(Stat::StaminaMax);
    let stamina_regen = core.map_or(20.0, |c| c.stats.regen.max(10.0)) * fx.get(Stat::StaminaRegen);

    let heal = frame.and_then(|f| f.heal).unwrap_or(HealKind::RepairCharges(0));

    let head = defs[Slot::Head.index()].map(|(d, _)| d);
    let lock_range = head.map_or(20.0, |h| h.stats.lock_range.max(20.0)) * fx.get(Stat::LockRange);
    let reveal = head.is_some_and(|h| h.stats.reveal);
    let fog_vision = head.map_or(0.0, |h| h.stats.fog_vision);

    let guard = defs.iter().flatten().map(|(d, _)| d.stats.guard).fold(0.25f32, f32::max).min(0.95) * fx.get(Stat::Guard);
    let can_parry = defs.iter().flatten().any(|(d, _)| d.parry);

    let hp_max = frame.map_or(200.0, |f| f.stats.hp) * cond(0) * fx.get(Stat::Hp);
    let armor = frame.map_or(0.0, |f| f.stats.armor) * cond(0) * fx.get(Stat::Armor);
    let posture_max = (frame.map_or(50.0, |f| f.stats.poise.max(30.0))
        + defs.iter().skip(1).flatten().map(|(d, _)| d.stats.poise).sum::<f32>()
        + weight * 0.25)
        * fx.get(Stat::Posture);

    // Move set composition: arm moves + leg movement + reactor stamina + backpack special.
    use MoveKind as K;
    use Slot as S;
    let mut moves = MoveSet {
        light: first_move(content, &defs, &[S::RightArm, S::LeftArm, S::Head, S::Back, S::Frame], &[K::Light]),
        heavy: first_move(content, &defs, &[S::RightArm, S::Head, S::LeftArm, S::Back, S::Frame], &[K::Heavy]),
        bash: first_move(content, &defs, &[S::LeftArm, S::RightArm], &[K::Bash]),
        special: first_move(content, &defs, &[S::Back], &[K::Special, K::Burst, K::Summon, K::Grab, K::Projectile])
            .or_else(|| first_move(content, &defs, &[S::Head, S::LeftArm, S::RightArm], &[K::Special, K::Summon, K::Grab])),
        eat: first_move(content, &defs, &[S::Frame, S::Head], &[K::Eat]),
        heretic: None,
    };
    if heretic_form && content.mv("heretic_form").is_some() {
        moves.heretic = Some(MoveRef { id: "heretic_form".into(), slot: Slot::Frame });
    }
    let (r, h) = rig_dims(rig);
    let block_cost = (1 + (complexity + heretic_count as u32 * 2 + quirk_count) / 9).clamp(1, 3) as u8;
    ResolvedBuild {
        side: lo.side,
        rig,
        size,
        parts,
        hp_max,
        armor,
        posture_max,
        capacity,
        weight,
        load,
        class,
        overloaded,
        com,
        lean,
        loco,
        speed,
        swim: swim * fx.get(Stat::SwimSpeed),
        deep_water,
        climb,
        flying: matches!(loco, LocoKind::Wings | LocoKind::Hover),
        turn_speed,
        stamina_kind,
        stamina_max,
        stamina_regen,
        heal,
        lock_range,
        reveal,
        fog_vision,
        guard,
        can_parry,
        moves,
        heretic_count,
        filled,
        sync,
        machine_blood,
        heretic_form,
        fx,
        block_cost,
        radius: r * size,
        height: h * size,
    }
}

/// Validation: every *player* build must have a light attack, a heavy attack, a dodge, a heal,
/// and at least one way to deal posture damage.
pub fn validate(lo: &Loadout, b: &ResolvedBuild, content: &Content) -> Vec<BuildIssue> {
    let mut out = vec![];
    for s in Slot::ALL {
        match lo.get(s) {
            None => out.push(BuildIssue::MissingSlot(s)),
            Some(p) if content.part(&p.def).is_none() => out.push(BuildIssue::UnknownPart(s)),
            _ => {}
        }
    }
    if b.moves.light.is_none() {
        out.push(BuildIssue::NoLight);
    }
    if b.moves.heavy.is_none() {
        out.push(BuildIssue::NoHeavy);
    }
    if b.speed < 3.0 {
        out.push(BuildIssue::NoDodge);
    }
    if let HealKind::RepairCharges(0) = b.heal {
        out.push(BuildIssue::NoHeal)
    }
    let posture = b.can_parry
        || [&b.moves.light, &b.moves.heavy, &b.moves.bash, &b.moves.special]
            .iter()
            .filter_map(|m| m.as_ref())
            .filter_map(|m| content.mv(&m.id))
            .any(|m| m.posture > 0.0);
    if !posture {
        out.push(BuildIssue::NoPostureDamage);
    }
    out
}
