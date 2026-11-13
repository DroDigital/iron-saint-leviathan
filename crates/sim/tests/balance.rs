//! Balance smoke test: a scripted "competent player" bot must be able to beat district bosses
//! with a premade chassis, and must not trivially steamroll them either.

use glam::Vec2;
use isl_data::*;
use isl_sim::build::Loadout;
use isl_sim::fighter::*;
use isl_sim::tide::TideClock;
use isl_sim::world::*;
use std::path::Path;

fn fight(content: &Content, chassis: &str, boss: &str, seed: u64) -> (bool, u64, u32) {
    let ch = content.chassis_def(chassis).unwrap();
    let lo = Loadout::premade(ch, content, 1);
    let cfg = WorldConfig {
        district: "breakwater".into(),
        state: DistrictState::Contested,
        side: ch.side,
        tide: TideClock { hours: 6, ..Default::default() },
        seed,
        mode: Mode::Run,
        player_fx: vec![],
        enemy_fx: vec![],
        assists: Assists::default(),
        persistent: vec![],
        wrecks: vec![],
        spawns: Some(vec![SpawnSpec::Boss(boss.into())]),
        density: 1.0,
        war: 1,
        bell: false,
        static_night: vec![],
        demo_disc: None,
    };
    let mut w = World::new(content, cfg, vec![lo]);
    let b = 1usize;
    w.fighters[b].ai.as_mut().unwrap().awake = true;
    let mut deaths = 0;
    for t in 0..(60 * 60 * 6) {
        let p = &w.fighters[0];
        let bf = &w.fighters[b];
        let to = bf.pos - p.pos;
        let dist = Vec2::new(to.x, to.z).length() - p.radius() - bf.radius();
        let mut c = Command { lock: p.lock.is_none() && t % 30 == 0, ..Default::default() };
        let dir = Vec2::new(to.x, to.z).normalize_or_zero();
        // Read the boss's telegraph like a player would (animation state, not inputs).
        let threat = match &bf.act {
            Act::Attack { mv, skip, .. } => content.mv(&mv.id).map(|m| {
                let w = (m.windup + m.feint).saturating_sub(*skip) as i64;
                let until = w - bf.act_t as i64;
                (until, m.reach * bf.size() + m.radius * bf.size())
            }),
            _ => None,
        };
        let busy = !matches!(p.act, Act::Idle | Act::Guard);
        if let Some((until, reach)) = threat {
            if (0..=4).contains(&until) && dist < reach + 4.0 && !busy {
                c.dodge = true;
                c.move_dir = Vec2::new(-dir.y, dir.x);
            } else if until > 4 && dist < reach + 6.0 {
                c.move_dir = -dir * 0.6;
            } else if until < 0 {
                // Recovery: punish.
                if dist > 3.0 * p.size() {
                    c.move_dir = dir;
                } else if !busy && p.stamina.frac() > 0.25 {
                    c.light = t % 12 == 0;
                    c.heavy = t % 60 == 0;
                }
            }
        } else if matches!(bf.act, Act::PostureBroken) && dist < 8.0 {
            c.heavy = true;
            c.move_dir = dir;
        } else {
            let reach = 3.0 * p.size();
            if dist > reach {
                c.move_dir = dir;
            } else if p.stamina.frac() > 0.3 && !busy {
                c.light = t % 20 == 0;
                c.heavy = t % 97 == 0;
            }
        }
        if p.hp_frac() < 0.35 && p.heal_charges > 0 && dist > 10.0 && !busy {
            c.heal = true;
        }
        w.step(content, &[c]);
        let evs: Vec<SimEvent> = w.events.drain(..).collect();
        if std::env::var("BAL_DEBUG").is_ok() {
            for e in &evs {
                if let SimEvent::Hit { attacker, target: 0, dmg, blocked, .. } = e {
                    let k = format!("{:?}", w.fighters[*attacker as usize].kind);
                    let mv = match &w.fighters[*attacker as usize].act {
                        Act::Attack { mv, .. } => mv.id.clone(),
                        _ => "-".into(),
                    };
                    eprintln!("dmg_by {k} {mv} {dmg:.0} blocked={blocked}");
                }
            }
        }
        for e in evs {
            if let SimEvent::PlayerDied { .. } = e {
                deaths += 1;
                w.respawn_player(content, 0);
                w.fighters[b].ai.as_mut().unwrap().awake = true;
            }
        }
        if std::env::var("BAL_DEBUG").is_ok() && t % 600 == 0 {
            let (p, bf) = (&w.fighters[0], &w.fighters[b]);
            let verses = w.fighters.iter().filter(|f| f.alive && !matches!(f.act, Act::Dead) && f.kind == FighterKind::Verse).count();
            eprintln!(
                "verses={verses} t={} p={:?} {} hp={:.0} st={:.2} | boss={:?} {} hp={:.0}/{:.0} post={:.0} awake={}",
                t / 60,
                p.pos,
                p.act.name(),
                p.hp,
                p.stamina.frac(),
                bf.pos,
                bf.act.name(),
                bf.hp,
                bf.build.hp_max,
                bf.posture,
                bf.ai.as_ref().unwrap().awake
            );
        }
        if !w.fighters[b].alive || matches!(w.fighters[b].act, Act::Dead) {
            return (true, t, deaths);
        }
    }
    (false, 0, deaths)
}

#[test]
fn district_bosses_are_beatable_but_not_trivial() {
    let c = Content::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
    let only = std::env::var("BAL_ONLY").ok();
    let matchups = [
        ("iron_saint", "boss_leviathan"),
        ("st_anchor", "boss_mawgrave"),
        ("st_wren", "boss_pale_choir"),
        ("leviathan", "boss_iron_saint"),
        ("mawgrave", "boss_st_anchor"),
        ("goliath_moth", "boss_st_cinder"),
    ];
    let mut report = vec![];
    for (ch, boss) in matchups {
        if only.as_deref().is_some_and(|o| o != boss) {
            continue;
        }
        let (won, ticks, deaths) = fight(&c, ch, boss, 7);
        report.push(format!("{ch:>12} vs {boss:<18} won={won} time={:>4}s deaths={deaths}", ticks / 60));
        assert!(won, "{ch} could not beat {boss} in 6 minutes:\n{}", report.join("\n"));
        assert!(ticks > 60 * 20, "{ch} beat {boss} in under 20 s - too easy");
    }
    println!("{}", report.join("\n"));
}
