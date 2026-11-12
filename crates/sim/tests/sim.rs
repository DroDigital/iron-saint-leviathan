//! Integration tests for the simulation rules the design calls out explicitly:
//! damage and posture math, sever rules, weight and balance, move-set validation, quirk effects,
//! RNG determinism and the Cross-Harvest rule.

use glam::{Vec2, Vec3};
use isl_data::*;
use isl_sim::build::{self, BuildIssue, Loadout, WeightClass};
use isl_sim::fighter::*;
use isl_sim::parts::{Condition, Origin, PartInstance};
use isl_sim::rng::SimRng;
use isl_sim::tide::TideClock;
use isl_sim::world::*;
use std::path::Path;
use std::sync::OnceLock;

fn content() -> &'static Content {
    static C: OnceLock<Content> = OnceLock::new();
    C.get_or_init(|| Content::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).expect("content"))
}

fn premade(id: &str) -> Loadout {
    let c = content();
    Loadout::premade(c.chassis_def(id).unwrap(), c, 1)
}

fn arena(side: Side, player: &str, spawns: Vec<SpawnSpec>, seed: u64) -> World {
    let cfg = WorldConfig {
        district: "harbor".into(),
        state: DistrictState::Contested,
        side,
        tide: TideClock::default(),
        seed,
        mode: Mode::Run,
        player_fx: vec![],
        enemy_fx: vec![],
        assists: Assists::default(),
        persistent: vec![],
        wrecks: vec![],
        spawns: Some(spawns),
        density: 1.0,
        war: 1,
        bell: false,
        static_night: vec![],
        demo_disc: None,
    };
    World::new(content(), cfg, vec![premade(player)])
}

fn run(w: &mut World, cmds: impl Fn(u64, &World) -> Command, ticks: u64) -> Vec<SimEvent> {
    let mut ev = vec![];
    for _ in 0..ticks {
        let c = cmds(w.tick, w);
        w.step(content(), &[c]);
        ev.extend(w.events.drain(..));
    }
    ev
}

fn face_and_close(w: &mut World, target: u32) {
    let t = w.fighters[target as usize].pos;
    let p = w.player_id(0).unwrap() as usize;
    let dir = (w.fighters[p].pos - t).normalize_or_zero();
    let gap = w.fighters[p].radius() + w.fighters[target as usize].radius() + 1.5;
    w.fighters[p].pos = t + dir * gap;
    w.fighters[p].yaw = isl_sim::geom::dir_yaw(t - w.fighters[p].pos);
    w.fighters[target as usize].yaw = isl_sim::geom::dir_yaw(w.fighters[p].pos - t);
}

#[test]
fn damage_and_armor_math() {
    let mut w = arena(Side::Saint, "iron_saint", vec![SpawnSpec::Enemy("juvenile".into())], 1);
    let e = 1u32;
    face_and_close(&mut w, e);
    w.fighters[e as usize].ai = None;
    let hp0 = w.fighters[e as usize].hp;
    let ev = run(&mut w, |t, _| Command { light: t == 1, ..Default::default() }, 40);
    let hit = ev.iter().find_map(|e| match e {
        SimEvent::Hit { dmg, slot, blocked: false, .. } => Some((*dmg, *slot)),
        _ => None,
    });
    let (dmg, slot) = hit.expect("light attack connects");
    // 22 base * 1.6 scale * 1.1 blade power, reduced by armour 100/(100+armour).
    let f = &w.fighters[e as usize];
    let armor = if slot == Slot::Frame { f.build.armor } else { f.build.parts[slot.index()].as_ref().unwrap().armor };
    let bonus = TideClock::default().side_bonus(Side::Saint); // dawn + grid power
    let expected = 22.0 * DAMAGE_SCALE * 1.1 * bonus * 100.0 / (100.0 + armor);
    assert!((dmg - expected).abs() < 0.5, "dmg {dmg} expected {expected}");
    assert!(f.hp < hp0);
    assert!(f.posture > 0.0, "hits deal posture damage");
}

#[test]
fn parry_staggers_and_breaks_posture() {
    let mut w = arena(Side::Saint, "iron_saint", vec![SpawnSpec::Enemy("deacon".into())], 2);
    let e = 1usize;
    face_and_close(&mut w, e as u32);
    w.fighters[e].ai = None;
    let p = w.player_id(0).unwrap() as usize;
    // Enemy swings; player parries just before the active frame.
    w.start_attack(content(), e, w.fighters[e].build.moves.light.clone().unwrap(), false);
    let windup = content().mv("deacon_mace").unwrap().windup as u64;
    let start = w.tick;
    let ev = run(&mut w, |t, _| Command { parry: t == start + windup - 3, ..Default::default() }, 40);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::Parried { .. })), "parry lands");
    assert!(w.fighters[e].posture > 0.0);
    assert!(w.fighters[p].hp == w.fighters[p].build.hp_max, "parried hit deals no damage");
}

#[test]
fn dodge_iframes_evade() {
    let mut w = arena(Side::Saint, "st_wren", vec![SpawnSpec::Enemy("deacon".into())], 3);
    let e = 1usize;
    face_and_close(&mut w, e as u32);
    w.fighters[e].ai = None;
    w.start_attack(content(), e, w.fighters[e].build.moves.light.clone().unwrap(), false);
    let windup = content().mv("deacon_mace").unwrap().windup as u64;
    let start = w.tick;
    let ev = run(&mut w, |t, _| Command { dodge: t == start + windup - 2, move_dir: Vec2::new(1.0, 0.0), ..Default::default() }, 40);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::DodgeEvaded { .. })), "i-frames evade the hit");
}

#[test]
fn sever_drops_pristine_opposite_side_part() {
    let mut w = arena(Side::Saint, "iron_saint", vec![SpawnSpec::Enemy("shellback".into())], 4);
    let e = 1usize;
    face_and_close(&mut w, e as u32);
    w.fighters[e].ai = None;
    let p = w.player_id(0).unwrap() as usize;
    w.fighters[p].lock = Some(LockOn { target: e as u32, slot: Slot::LeftArm });
    // Force a posture break.
    w.fighters[e].posture = w.fighters[e].build.posture_max - 0.1;
    let ev = run(&mut w, |t, _| Command { light: t == 1, ..Default::default() }, 30);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::PostureBroken { .. })));
    let ev = run(&mut w, |t, w| Command { heavy: t == w.tick && t % 5 == 0, ..Default::default() }, 80);
    let sev = ev.iter().find_map(|e| match e {
        SimEvent::Severed { slot, .. } => Some(*slot),
        _ => None,
    });
    assert_eq!(sev, Some(Slot::LeftArm));
    let loot = ev.iter().find_map(|e| match e {
        SimEvent::Loot { def, condition, .. } => Some((def.clone(), *condition)),
        _ => None,
    });
    let (def, cond) = loot.expect("sever drops loot");
    assert_eq!(cond, Condition::Pristine);
    assert_eq!(content().part(&def).unwrap().side, Side::Kaiju, "Saints only harvest Kaiju parts");
    assert!(w.fighters[e].destroyed[Slot::LeftArm.index()]);
    assert!(w.fighters[e].build.moves.bash.is_none(), "no left pincer means no left-pincer move");
}

#[test]
fn cross_harvest_rule_holds_for_every_drop() {
    for (side, chassis, foes) in [
        (Side::Saint, "st_anchor", vec!["hull_leech", "shellback", "juvenile", "brine_spitter"]),
        (Side::Kaiju, "mawgrave", vec!["deacon", "tank", "lancer", "drone"]),
    ] {
        let spawns = foes.iter().map(|f| SpawnSpec::Enemy(f.to_string())).collect();
        let mut w = arena(side, chassis, spawns, 5);
        // Kill everything, destroying parts along the way.
        for _ in 0..4000 {
            let tgt = w.fighters.iter().find(|f| f.team == Team::Enemy && f.alive && !matches!(f.act, Act::Dead)).map(|f| f.id);
            let Some(t) = tgt else { break };
            face_and_close(&mut w, t);
            let k = w.tick;
            w.step(content(), &[Command { light: k % 2 == 0, heavy: k % 7 == 0, ..Default::default() }]);
            let p = w.player_id(0).unwrap() as usize;
            w.fighters[p].hp = w.fighters[p].build.hp_max;
            w.events.clear();
        }
        let mut parts: Vec<&PartInstance> = w.collected.iter().collect();
        for pk in &w.pickups {
            if let PickupItem::Part(p) = &pk.item {
                parts.push(p);
            }
        }
        assert!(!parts.is_empty(), "something dropped");
        for p in parts {
            assert_eq!(content().part(&p.def).unwrap().side, side.other(), "{} dropped for {side:?}", p.def);
        }
    }
}

#[test]
fn weight_and_balance() {
    let c = content();
    let wren = build::resolve(&premade("st_wren"), c, &[], [false; 7]);
    let anchor = build::resolve(&premade("st_anchor"), c, &[], [false; 7]);
    let iron = build::resolve(&premade("iron_saint"), c, &[], [false; 7]);
    assert_eq!(wren.class, WeightClass::Light);
    assert_eq!(iron.class, WeightClass::Medium);
    assert_eq!(anchor.class, WeightClass::Heavy);
    assert!(wren.turn_speed > anchor.turn_speed);
    assert!(wren.speed > anchor.speed);
    // Lopsided: put a heavy anchor fluke on the right arm of a Wren -> leans right.
    let mut lo = premade("st_wren");
    lo.slots[Slot::RightArm.index()] = Some(PartInstance::premade(c.part("st_anchor_fluke").unwrap(), 99, "st_anchor"));
    let b = build::resolve(&lo, c, &[], [false; 7]);
    assert!(b.com.x > 0.1 && b.lean > 0.0, "com {:?}", b.com);
    assert!(b.load > wren.load);
}

#[test]
fn move_set_validation() {
    let c = content();
    let lo = premade("iron_saint");
    let b = build::resolve(&lo, c, &[], [false; 7]);
    assert!(build::validate(&lo, &b, c).is_empty(), "premade chassis are valid");
    // Losing the right arm removes light/heavy (the shield only bashes).
    let mut d = [false; 7];
    d[Slot::RightArm.index()] = true;
    let b2 = build::resolve(&lo, c, &[], d);
    assert!(b2.moves.light.is_none() && b2.moves.heavy.is_none());
    assert!(b2.moves.bash.is_some());
    // AA plinth legs cannot dodge.
    let mut lo3 = lo.clone();
    lo3.slots[Slot::Legs.index()] = Some(PartInstance::premade(c.part("aa_base").unwrap(), 5, "x"));
    let b3 = build::resolve(&lo3, c, &[], [false; 7]);
    assert!(build::validate(&lo3, &b3, c).contains(&BuildIssue::NoDodge));
    // An empty slot is reported.
    let mut lo4 = lo.clone();
    lo4.slots[Slot::Head.index()] = None;
    let b4 = build::resolve(&lo4, c, &[], [false; 7]);
    assert!(build::validate(&lo4, &b4, c).contains(&BuildIssue::MissingSlot(Slot::Head)));
    // Every premade chassis is a valid player build.
    for ch in &c.chassis {
        let lo = Loadout::premade(ch, c, 1);
        let b = build::resolve(&lo, c, &[], [false; 7]);
        assert!(build::validate(&lo, &b, c).is_empty(), "{} invalid: {:?}", ch.id, build::validate(&lo, &b, c));
    }
}

#[test]
fn heretic_grafts() {
    let c = content();
    let mut lo = premade("iron_saint");
    let lev = premade("leviathan");
    for s in [Slot::LeftArm, Slot::RightArm, Slot::Head, Slot::Back] {
        lo.slots[s.index()] = lev.slots[s.index()].clone();
    }
    let b = build::resolve(&lo, c, &[], [false; 7]);
    assert_eq!(b.heretic_count, 4);
    assert!(b.heretic_form, "4/7 heretic parts > 50% unlocks Heretic Form");
    assert!(b.moves.heretic.is_some());
    assert!(b.sync < 0.5 && b.fx.hallucinate > 0.0, "low sync hallucinates");
    // Kaiju heresy raises machine-blood instead.
    let mut k = premade("leviathan");
    let iron = premade("iron_saint");
    k.slots[Slot::LeftArm.index()] = iron.slots[Slot::LeftArm.index()].clone();
    let bk = build::resolve(&k, c, &[], [false; 7]);
    assert!(bk.machine_blood > 0.1 && bk.sync == 1.0 - 0.14);
}

#[test]
fn quirk_effects() {
    let c = content();
    let base = premade("iron_saint");
    let b0 = build::resolve(&base, c, &[], [false; 7]);
    let mut lo = base.clone();
    lo.slots[Slot::RightArm.index()].as_mut().unwrap().quirks = vec!["hollow".into(), "dodges_left".into(), "old_grudge".into()];
    lo.slots[Slot::RightArm.index()].as_mut().unwrap().origin = Origin::Boss("leviathan".into());
    let b = build::resolve(&lo, c, &[], [false; 7]);
    assert!((b.weight - b0.weight * 0.75).abs() < 1e-3, "Hollow: -25% weight");
    assert!((b.hp_max - b0.hp_max * 0.85).abs() < 1e-2, "Hollow: -15% HP");
    assert_eq!(b.fx.dodge_bonus(DodgeDir::Left), 4);
    assert_eq!(b.fx.dodge_bonus(DodgeDir::Right), 0);
    assert!((b.fx.grudge_vs("leviathan") - 1.35).abs() < 1e-6);
    assert_eq!(b.fx.grudge_vs("mawgrave"), 1.0);
    // Meta cap: +10% total.
    let capped = isl_sim::parts::cap_meta_bonuses(&[Effect::StatMul(Stat::Damage, 1.08), Effect::StatMul(Stat::Hp, 1.08)], 0.10);
    let total: f32 = capped
        .iter()
        .map(|e| match e {
            Effect::StatMul(_, m) => m - 1.0,
            _ => 0.0,
        })
        .sum();
    assert!((total - 0.10).abs() < 1e-6);
}

#[test]
fn quirk_rolls_follow_condition() {
    let c = content();
    let d = c.part("leviathan_maw").unwrap();
    let mut r = SimRng::new(11);
    for _ in 0..200 {
        let p = PartInstance::harvest(d, Condition::Pristine, Origin::Fabricated, 1, &mut r, c);
        assert!(p.quirks.is_empty());
        let s = PartInstance::harvest(d, Condition::Scarred, Origin::Fabricated, 1, &mut r, c);
        assert_eq!(s.quirks.len(), 1);
        let q = c.quirk(&s.quirks[0]).unwrap();
        assert!(q.polarity != Polarity::Negative && !q.special && !q.import_only);
        assert!(q.side.is_none_or(|x| x == Side::Kaiju), "side-restricted quirks respect anatomy");
        let m = PartInstance::harvest(d, Condition::Mangled, Origin::Fabricated, 1, &mut r, c);
        assert!((1..=2).contains(&m.quirks.len()));
        assert!(m.quirks.iter().all(|q| c.quirk(q).unwrap().polarity != Polarity::Positive));
    }
}

fn state_hash(w: &World) -> String {
    let mut s = String::new();
    for f in &w.fighters {
        s += &format!("{:.4},{:.4},{:.4},{:.3},{:.3},{}|", f.pos.x, f.pos.z, f.yaw, f.hp, f.posture, f.act.name());
    }
    s += &format!("{}", w.projectiles.len());
    s
}

#[test]
fn simulation_is_deterministic() {
    let make = || {
        let mut w = arena(Side::Kaiju, "leviathan", vec![SpawnSpec::Boss("boss_iron_saint".into()), SpawnSpec::Enemy("tank".into())], 77);
        let ev = run(
            &mut w,
            |t, _| Command {
                move_dir: Vec2::new(((t as f32) * 0.01).sin(), 1.0),
                light: t % 23 == 0,
                heavy: t % 61 == 0,
                dodge: t % 97 == 0,
                lock: t == 30,
                guard: t % 200 > 180,
                ..Default::default()
            },
            1800,
        );
        (state_hash(&w), ev.len())
    };
    let a = make();
    let b = make();
    assert_eq!(a, b);
}

#[test]
fn bosses_telegraph_and_phase() {
    let mut w = arena(Side::Saint, "iron_saint", vec![SpawnSpec::Boss("boss_leviathan".into())], 9);
    let b = 1usize;
    w.fighters[b].ai.as_mut().unwrap().awake = true;
    let ev = run(&mut w, |_, _| Command::default(), 600);
    let starts: Vec<&String> = ev
        .iter()
        .filter_map(|e| match e {
            SimEvent::AttackStart { id: 1, mv, .. } => Some(mv),
            _ => None,
        })
        .collect();
    assert!(!starts.is_empty(), "boss attacks");
    for m in starts {
        assert!(content().mv(m).unwrap().windup >= 18, "boss move {m} telegraphs >= 300ms");
    }
    w.fighters[b].hp = w.fighters[b].build.hp_max * 0.5;
    let ev = run(&mut w, |_, _| Command::default(), 2);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::BossPhase { phase: 1, .. })));
}

#[test]
fn death_drops_wreck_and_recovery() {
    let mut cfgw = arena(Side::Saint, "iron_saint", vec![SpawnSpec::Enemy("deacon".into())], 10);
    cfgw.cfg.mode = Mode::Mission;
    let p = cfgw.player_id(0).unwrap() as usize;
    cfgw.fighters[p].currency = 500;
    cfgw.fighters[p].hp = 1.0;
    cfgw.fighters[p].status.burn = 200;
    let ev = run(&mut cfgw, |_, _| Command::default(), 30);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::PlayerDied { .. })));
    assert_eq!(cfgw.wrecks.len(), 1);
    assert_eq!(cfgw.wrecks[0].currency, 500);
    assert_eq!(cfgw.fighters[p].currency, 0);
    let pos = cfgw.wrecks[0].pos;
    cfgw.respawn_player(content(), 0);
    cfgw.fighters[p].pos = pos + Vec3::new(2.0, 0.0, 0.0);
    // Clear checkpoint proximity so interact targets the wreck.
    for c in cfgw.city.checkpoints.iter_mut() {
        c.pos += Vec3::new(500.0, 0.0, 0.0);
    }
    let ev = run(&mut cfgw, |t, w| Command { interact: t == w.tick, ..Default::default() }, 2);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::WreckRecovered { amount: 500 })));
}
