//! All screens: update (input -> next screen) and draw (UI + optional 3D stage).

use crate::fmv::{Fmv, Scene};
use crate::foundry::{self, AMode, Assembly, EXTRAS, PALETTE};
use crate::game::{Game, Launch, Request, Screen};
use crate::hud;
use crate::mission::MissionKind;
use crate::settings::{Palette, RESOLUTIONS};
use crate::visuals::{Anim, Pose, rig_of, size_of};
use glam::{Mat4, Vec3};
use isl_audio::{Audio, Sfx};
use isl_data::*;
use isl_input::{Action, Input, PlayerInput};
use isl_render::{DrawList, Lighting, PostSettings, View, meshgen, textures};
use isl_save::{BlockData, CorruptItem};
use isl_sim::build::{self, Loadout};
use isl_sim::run::TideRun;
use isl_sim::war::side_ix;
use isl_ui::{Ui, col, icon};

#[derive(PartialEq, Eq)]
enum Nav {
    None,
    Confirm,
    Back,
    Left,
    Right,
}

fn nav(p: &PlayerInput, sel: &mut usize, n: usize, audio: &mut Audio) -> Nav {
    if n > 0 {
        if p.hit(Action::MenuDown) {
            *sel = (*sel + 1) % n;
            audio.play(Sfx::UiMove, None, 0.5, 1.0);
        }
        if p.hit(Action::MenuUp) {
            *sel = (*sel + n - 1) % n;
            audio.play(Sfx::UiMove, None, 0.5, 1.0);
        }
    }
    if p.hit(Action::Confirm) {
        audio.play(Sfx::UiOk, None, 0.6, 1.0);
        return Nav::Confirm;
    }
    if p.hit(Action::Back) {
        audio.play(Sfx::UiBack, None, 0.6, 1.0);
        return Nav::Back;
    }
    if p.hit(Action::MenuLeft) {
        audio.play(Sfx::UiMove, None, 0.4, 1.2);
        return Nav::Left;
    }
    if p.hit(Action::MenuRight) {
        audio.play(Sfx::UiMove, None, 0.4, 1.2);
        return Nav::Right;
    }
    Nav::None
}

fn main_items(g: &Game) -> Vec<(String, bool)> {
    vec![
        ("CONTINUE WAR".into(), g.campaign.is_some()),
        ("NEW WAR".into(), true),
        ("TIDE RUN".into(), true),
        ("DAILY TIDE".into(), true),
        ("NEMESIS GAUNTLET".into(), true),
        ("LINK CABLE VERSUS".into(), true),
        ("TEST ARENA".into(), true),
        ("MEMORY CARD".into(), true),
        ("ARCHIVE".into(), true),
        ("SETTINGS".into(), true),
        ("QUIT".into(), true),
    ]
}

fn war_rows(g: &Game) -> Vec<String> {
    let mut v: Vec<String> = g
        .content
        .districts
        .iter()
        .filter(|d| !d.finale || g.campaign.as_ref().is_some_and(|c| c.breakwater_open))
        .map(|d| d.id.clone())
        .collect();
    for s in ["@FOUNDRY", "@NEST", "@WAIT", "@CARD", "@ARCHIVE", "@MAIN"] {
        v.push(s.into());
    }
    v
}

fn versus_pool(g: &Game, side: Side) -> Vec<Loadout> {
    let mut v = vec![g.active_or_premade(side)];
    for c in g.content.chassis.iter().filter(|c| c.side == side && g.card.system.archive.chassis_available(c)) {
        v.push(Loadout::premade(c, &g.content, 1 << 40));
    }
    for (_, lo, ..) in g.card.builds().filter(|b| b.1.side == side) {
        v.push(lo.clone());
    }
    v
}

fn corrupt_options(g: &Game) -> Vec<(String, Option<CorruptItem>)> {
    let mut v = vec![];
    if let Some(c) = &g.campaign {
        for (s, inv) in c.inventory.iter().enumerate() {
            for p in inv.parts.iter().take(5) {
                let name = g.content.part(&p.def).map_or(p.def.clone(), |d| d.name.clone());
                v.push((format!("{} PART: {}", if s == 0 { "SAINT" } else { "KAIJU" }, name), Some(CorruptItem::Part(p.clone()))));
            }
        }
    }
    for o in g.content.omens.iter().take(3) {
        v.push((format!("TRAIT: {}", o.name), Some(CorruptItem::Trait(o.id.clone()))));
    }
    v.push(("CARRY NOTHING".into(), None));
    v
}

pub fn update(g: &mut Game, screen: Screen, dt: f32, input: &mut Input, audio: &mut Audio) -> Screen {
    let p = input.players[0].clone();
    match screen {
        Screen::Boot(t) => {
            let t = t + dt;
            if t > 2.6 || input.any_pressed { Screen::Title(0.0) } else { Screen::Boot(t) }
        }
        Screen::Title(t) => {
            if t > 0.5 && (p.hit(Action::Confirm) || p.hit(Action::Pause)) {
                audio.play(Sfx::UiOk, None, 0.8, 1.0);
                // Rage-quit flag from last session: quitting right after dying.
                Screen::Main { sel: if g.campaign.is_some() { 0 } else { 1 } }
            } else {
                Screen::Title(t + dt)
            }
        }
        Screen::Main { mut sel } => {
            let items = main_items(g);
            match nav(&p, &mut sel, items.len(), audio) {
                Nav::Confirm if items[sel].1 => match sel {
                    0 => Screen::WarMap { sel: 0, side: g.campaign.as_ref().map_or(Side::Saint, |c| c.last_side) },
                    1 => Screen::NewWar { sel: 0, condition: 0, rot: 0, use_corrupt: g.card.corrupted().is_some() },
                    2 => Screen::RunSetup { sel: 0, omens: vec![false; g.content.omens.len()], daily: false },
                    3 => Screen::RunSetup { sel: 0, omens: vec![false; g.content.omens.len()], daily: true },
                    4 => Screen::Gauntlet { sel: 0, side: Side::Saint },
                    5 => Screen::Versus { sel: 0, a: 0, b: 0 },
                    6 => {
                        let lo = g.active_or_premade(Side::Saint);
                        g.start_launch(Launch::Arena { loadout: Box::new(lo) });
                        std::mem::replace(&mut g.screen, Screen::Mission)
                    }
                    7 => Screen::Card { sel: 0, confirm: false },
                    8 => Screen::Archive { tab: 0, sel: 0 },
                    9 => Screen::Settings { tab: 0, sel: 0 },
                    _ => {
                        g.requests.push(Request::Quit);
                        Screen::Main { sel }
                    }
                },
                Nav::Back => Screen::Title(0.0),
                _ => Screen::Main { sel },
            }
        }
        Screen::NewWar { mut sel, mut condition, mut rot, mut use_corrupt } => {
            let conds: Vec<String> = std::iter::once(String::new())
                .chain(
                    g.content.conditions.iter().filter(|c| g.card.system.archive.unlocked_conditions.contains(&c.id)).map(|c| c.id.clone()),
                )
                .collect();
            let max_rot = g.card.system.archive.disc_rot_unlocked;
            match nav(&p, &mut sel, 5, audio) {
                Nav::Left | Nav::Right => {
                    let d = if p.hit(Action::MenuRight) { 1 } else { -1i32 };
                    match sel {
                        0 => condition = ((condition as i32 + d).rem_euclid(conds.len() as i32)) as usize,
                        1 => rot = (rot as i32 + d).clamp(0, max_rot as i32) as u8,
                        2 => use_corrupt = !use_corrupt && g.card.corrupted().is_some(),
                        _ => {}
                    }
                    Screen::NewWar { sel, condition, rot, use_corrupt }
                }
                Nav::Confirm if sel == 3 => {
                    if let Some(i) = g.card.campaign_index() {
                        g.card.delete(i);
                    }
                    let c = conds.get(condition).filter(|s| !s.is_empty()).cloned();
                    g.new_campaign(c, rot, use_corrupt);
                    std::mem::replace(&mut g.screen, Screen::Mission)
                }
                Nav::Back | Nav::Confirm if sel == 4 || p.hit(Action::Back) => Screen::Main { sel: 1 },
                _ => Screen::NewWar { sel, condition, rot, use_corrupt },
            }
        }
        Screen::WarMap { mut sel, mut side } => {
            let rows = war_rows(g);
            match nav(&p, &mut sel, rows.len(), audio) {
                Nav::Left | Nav::Right => {
                    side = side.other();
                    Screen::WarMap { sel, side }
                }
                Nav::Back => Screen::Main { sel: 0 },
                Nav::Confirm => {
                    let r = rows[sel].clone();
                    match r.as_str() {
                        "@FOUNDRY" | "@NEST" => {
                            let s = if r == "@FOUNDRY" { Side::Saint } else { Side::Kaiju };
                            let lo = g.active_or_premade(s);
                            Screen::Foundry(Box::new(Assembly::new(s, lo)))
                        }
                        "@WAIT" => {
                            if let Some(c) = g.campaign.as_mut()
                                && c.rest()
                            {
                                g.toast = Some((format!("THE TIDE TURNS. WEATHER: {}", c.tide.weather.name()), 3.0));
                            }
                            g.save_card();
                            Screen::WarMap { sel, side }
                        }
                        "@CARD" => Screen::Card { sel: 0, confirm: false },
                        "@ARCHIVE" => Screen::Archive { tab: 0, sel: 0 },
                        "@MAIN" => Screen::Main { sel: 0 },
                        d => Screen::Briefing { district: d.to_string(), side, sel: 0 },
                    }
                }
                _ => Screen::WarMap { sel, side },
            }
        }
        Screen::Briefing { district, side, mut sel } => {
            let finale = g.content.district(&district).is_some_and(|d| d.finale);
            let options = briefing_options(g, &district, side);
            match nav(&p, &mut sel, options.len(), audio) {
                Nav::Back => Screen::WarMap { sel: 0, side },
                Nav::Confirm => {
                    let (label, ending) = options[sel].clone();
                    if label == "BACK" {
                        return Screen::WarMap { sel: 0, side };
                    }
                    let lo = g.active_or_premade(side);
                    let issues = build::validate(&lo, &build::resolve(&lo, &g.content, &[], [false; 7]), &g.content);
                    if !issues.is_empty() {
                        g.toast(format!("BUILD INVALID: {}", issues[0].text()));
                        return Screen::Briefing { district, side, sel };
                    }
                    g.start_launch(Launch::War {
                        district: district.clone(),
                        side,
                        finale: if finale { Some(ending.unwrap_or_default()) } else { None },
                    });
                    std::mem::replace(&mut g.screen, Screen::Mission)
                }
                _ => Screen::Briefing { district, side, sel },
            }
        }
        Screen::DiscSwap { t, to, next } => {
            let t = t + dt;
            if t < 0.05 {
                audio.play(Sfx::DiscSwap, None, 0.8, 1.0);
            }
            if t > 3.0 || (t > 0.6 && (p.hit(Action::Confirm) || p.hit(Action::Pause))) {
                Screen::Loading { t: 0.0, lore: (g.time * 13.0) as usize, next }
            } else {
                let _ = to;
                Screen::DiscSwap { t, to, next }
            }
        }
        Screen::Loading { t, lore, next } => {
            let t = t + dt;
            if t > 1.0 {
                g.build_mission(&next, audio);
                std::mem::replace(&mut g.screen, Screen::Mission)
            } else {
                Screen::Loading { t, lore, next }
            }
        }
        Screen::Mission => Screen::Mission,
        Screen::Results { title, lines, next } => {
            if p.hit(Action::Confirm) || p.hit(Action::Back) {
                audio.play(Sfx::UiOk, None, 0.6, 1.0);
                *next
            } else {
                Screen::Results { title, lines, next }
            }
        }
        Screen::Foundry(a) => update_foundry(g, *a, &p, dt, audio),
        Screen::Card { mut sel, confirm } => {
            let n = g.card.entries.len() + 1;
            let nv = nav(&p, &mut sel, n, audio);
            if sel == g.card.entries.len() {
                if nv == Nav::Confirm {
                    g.typed.clear();
                    return Screen::Codes { text: String::new(), msg: "TYPE OR PASTE A CODE, ENTER TO IMPORT".into() };
                }
            } else if p.hit(Action::Heavy) || (confirm && nv == Nav::Confirm) {
                if confirm {
                    if matches!(g.card.entries[sel].data, BlockData::Campaign(_)) {
                        g.campaign = None;
                    }
                    g.card.delete(sel);
                    g.save_card();
                    return Screen::Card { sel: sel.saturating_sub(1), confirm: false };
                }
                return Screen::Card { sel, confirm: true };
            } else if p.hit(Action::Special) {
                if let BlockData::Build { loadout, .. } = &g.card.entries[sel].data {
                    let code = isl_save::codes::export_build(loadout);
                    let dir = g.paths.exports();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(dir.join(format!("{}.txt", loadout.name.replace(' ', "_"))), &code);
                    g.toast(format!("EXPORTED: {code}"));
                }
            } else if nv == Nav::Confirm
                && let BlockData::Build { loadout, .. } = &g.card.entries[sel].data
            {
                let lo = loadout.clone();
                if let Some(c) = g.campaign.as_mut() {
                    c.active[side_ix(lo.side)] = lo.clone();
                    g.toast(format!("{} IS NOW YOUR ACTIVE {}", lo.name, lo.side.name()));
                    g.save_card();
                }
            }
            match nv {
                Nav::Back if confirm => Screen::Card { sel, confirm: false },
                Nav::Back => {
                    if g.campaign.is_some() {
                        Screen::WarMap { sel: 0, side: g.campaign.as_ref().unwrap().last_side }
                    } else {
                        Screen::Main { sel: 7 }
                    }
                }
                _ => Screen::Card { sel, confirm: confirm && nv == Nav::None },
            }
        }
        Screen::Codes { mut text, mut msg } => {
            text.push_str(&std::mem::take(&mut g.typed));
            if p.hit(Action::Back) && text.is_empty() {
                return Screen::Card { sel: 0, confirm: false };
            }
            if input.players[0].hit(Action::Back) {
                text.pop();
            }
            if p.hit(Action::Confirm) && !text.is_empty() {
                let uid = (g.time * 1e6) as u64;
                match isl_save::codes::import_build(&text, &g.content, uid) {
                    Ok(lo) => {
                        msg = if g.save_build(&lo) { format!("IMPORTED BUILD {}", lo.name) } else { "CARD FULL".into() };
                        text.clear();
                    }
                    Err(_) => {
                        let mut rng = isl_sim::rng::SimRng::new(uid);
                        match isl_save::codes::import_part(&text, &g.content, uid, &mut rng) {
                            Ok(part) => {
                                if let Some(c) = g.campaign.as_mut() {
                                    let name = g.content.part(&part.def).map_or(String::new(), |d| d.name.clone());
                                    c.store_parts(vec![part], &g.content);
                                    msg = format!("IMPORTED PART {name}");
                                    g.save_card();
                                } else {
                                    msg = "START A WAR TO RECEIVE PARTS".into();
                                }
                                text.clear();
                            }
                            Err(e) => msg = format!("INVALID CODE ({e})"),
                        }
                    }
                }
            }
            Screen::Codes { text, msg }
        }
        Screen::Archive { mut tab, mut sel } => {
            if p.hit(Action::TabNext) || p.hit(Action::MenuRight) {
                tab = (tab + 1) % 5;
                sel = 0;
            }
            if p.hit(Action::TabPrev) || p.hit(Action::MenuLeft) {
                tab = (tab + 4) % 5;
                sel = 0;
            }
            let n = archive_rows(g, tab).len();
            match nav(&p, &mut sel, n, audio) {
                Nav::Back => {
                    if g.campaign.is_some() {
                        Screen::WarMap { sel: 0, side: g.campaign.as_ref().unwrap().last_side }
                    } else {
                        Screen::Main { sel: 8 }
                    }
                }
                _ => Screen::Archive { tab, sel },
            }
        }
        Screen::Settings { tab, sel } => update_settings(g, tab, sel, &p, input, audio),
        Screen::Cutscene(mut f) => {
            f.t += dt;
            if f.t < 0.05 {
                audio.play(Sfx::Drone, None, 0.6, 0.5);
            }
            if f.t > f.total() || (f.t > 0.8 && (p.hit(Action::Confirm) || p.hit(Action::Pause))) {
                match f.next.take() {
                    Some(n) => *n,
                    None => Screen::Main { sel: 0 },
                }
            } else {
                Screen::Cutscene(f)
            }
        }
        Screen::RunSetup { mut sel, mut omens, daily } => {
            let n = if daily { 2 } else { omens.len() + 2 };
            match nav(&p, &mut sel, n, audio) {
                Nav::Back => Screen::Main { sel: if daily { 3 } else { 2 } },
                Nav::Confirm => {
                    if (daily && sel == 0) || (!daily && sel == omens.len()) {
                        let (seed, oms, tag) = if daily {
                            let (s, tag) = Game::daily_seed_today();
                            let k = g.content.omens.len();
                            let o1 = g.content.omens[(s % k as u64) as usize].id.clone();
                            (s, vec![o1], Some(tag))
                        } else {
                            let s = (g.time * 7919.0) as u64 ^ 0xA11CE;
                            (s, g.content.omens.iter().zip(&omens).filter(|(_, on)| **on).map(|(o, _)| o.id.clone()).collect(), None)
                        };
                        g.run = Some(TideRun::new(&g.content, seed, oms, tag));
                        g.start_launch(Launch::RunFight);
                        std::mem::replace(&mut g.screen, Screen::Mission)
                    } else if (daily && sel == 1) || (!daily && sel == omens.len() + 1) {
                        Screen::Main { sel: 2 }
                    } else {
                        omens[sel] = !omens[sel];
                        Screen::RunSetup { sel, omens, daily }
                    }
                }
                _ => Screen::RunSetup { sel, omens, daily },
            }
        }
        Screen::RunDraft { mut sel } => {
            let n = g.run.as_ref().map_or(0, |r| r.offer.len()) + 1;
            match nav(&p, &mut sel, n, audio) {
                Nav::Confirm => {
                    if let Some(r) = g.run.as_mut()
                        && sel < r.offer.len()
                        && let Some(name) = r.draft(&g.content, sel)
                    {
                        g.toast = Some((format!("DRAFTED {name}"), 3.0));
                    }
                    g.start_launch(Launch::RunFight);
                    std::mem::replace(&mut g.screen, Screen::Mission)
                }
                _ => Screen::RunDraft { sel },
            }
        }
        Screen::Gauntlet { mut sel, mut side } => {
            let q = g.nemeses_of(side.other());
            match nav(&p, &mut sel, 2, audio) {
                Nav::Left | Nav::Right => {
                    side = side.other();
                    Screen::Gauntlet { sel, side }
                }
                Nav::Confirm if sel == 0 => {
                    if q.is_empty() {
                        g.toast("SAVE BUILDS OF THE OTHER SIDE TO THE MEMORY CARD FIRST");
                        return Screen::Gauntlet { sel, side };
                    }
                    g.gauntlet = Some((side, q.clone(), 0));
                    g.start_launch(Launch::Gauntlet { side, queue: q, index: 0 });
                    std::mem::replace(&mut g.screen, Screen::Mission)
                }
                Nav::Back | Nav::Confirm => Screen::Main { sel: 4 },
                _ => Screen::Gauntlet { sel, side },
            }
        }
        Screen::Versus { mut sel, mut a, mut b } => {
            let pa = versus_pool(g, Side::Saint);
            let pb = versus_pool(g, Side::Kaiju);
            match nav(&p, &mut sel, 4, audio) {
                Nav::Left | Nav::Right => {
                    let d = if p.hit(Action::MenuRight) { 1 } else { pa.len().max(pb.len()) * 4 - 1 };
                    match sel {
                        0 => a = (a + d) % pa.len(),
                        1 => b = (b + d) % pb.len(),
                        _ => {}
                    }
                    Screen::Versus { sel, a, b }
                }
                Nav::Confirm if sel == 2 => {
                    g.start_launch(Launch::Versus { a: Box::new(pa[a].clone()), b: Box::new(pb[b].clone()) });
                    std::mem::replace(&mut g.screen, Screen::Mission)
                }
                Nav::Back | Nav::Confirm if sel == 3 || p.hit(Action::Back) => Screen::Main { sel: 5 },
                _ => Screen::Versus { sel, a, b },
            }
        }
        Screen::Corrupt { mut sel, ending } => {
            let opts = corrupt_options(g);
            match nav(&p, &mut sel, opts.len(), audio) {
                Nav::Confirm => {
                    let item = opts[sel].1.clone();
                    g.conclude_war(&ending, item);
                    std::mem::replace(&mut g.screen, Screen::Main { sel: 0 })
                }
                _ => Screen::Corrupt { sel, ending },
            }
        }
    }
}

fn briefing_options(g: &Game, district: &str, side: Side) -> Vec<(String, Option<String>)> {
    let mut v = vec![];
    let finale = g.content.district(district).is_some_and(|d| d.finale);
    if finale {
        if let Some(c) = &g.campaign {
            let lo = &c.active[side_ix(side)];
            let b = build::resolve(lo, &g.content, &[], [false; 7]);
            let frac = b.heretic_count as f32 / b.filled.max(1) as f32;
            for e in c.available_endings(&g.content, side, frac) {
                let name = g.content.endings.iter().find(|x| x.id == e).map_or(e.to_string(), |x| x.name.clone());
                v.push((format!("FINALE: {name}"), Some(e.to_string())));
            }
        }
        if v.is_empty() {
            v.push(("ASSAULT THE BREAKWATER".into(), Some(String::new())));
        }
    } else {
        v.push(("LAUNCH SORTIE".into(), None));
    }
    v.push(("BACK".into(), None));
    v
}

fn update_foundry(g: &mut Game, mut a: Assembly, p: &PlayerInput, dt: f32, audio: &mut Audio) -> Screen {
    a.rot += dt * 0.6;
    let c = &g.content;
    let empty: [Vec<isl_sim::parts::PartInstance>; 2] = [vec![], vec![]];
    match a.mode {
        AMode::Slots => {
            let rows = a.rows();
            let mut sel = a.sel;
            let nv = nav(p, &mut sel, rows, audio);
            a.sel = sel;
            if nv == Nav::Back {
                return back_from_foundry(g);
            }
            if nv != Nav::Confirm {
                return Screen::Foundry(Box::new(a));
            }
            if a.sel < 7 {
                if g.campaign.is_none() {
                    g.toast("START A WAR TO HARVEST PARTS");
                    return Screen::Foundry(Box::new(a));
                }
                a.mode = AMode::Pick;
                a.list_sel = 0;
                return Screen::Foundry(Box::new(a));
            }
            let extra = EXTRAS[a.sel - 7];
            let slot_part = |a: &Assembly| a.slot().and_then(|s| a.lo.get(s).cloned());
            let _ = slot_part;
            match extra {
                "PAINT" => a.mode = AMode::Paint,
                "LOAD PREMADE" => {
                    a.mode = AMode::Premade;
                    a.list_sel = 0;
                }
                "REPAIR PART" | "RE-ROLL QUIRKS" | "TRANSPLANT QUIRK" => {
                    // Operate on the last slot the cursor pointed at (stored in paint_sel as a slot index).
                    let s = Slot::ALL[a.paint_sel.min(6)];
                    let cost = match extra {
                        "REPAIR PART" => 300,
                        "RE-ROLL QUIRKS" => 400,
                        _ => 500,
                    };
                    let allowed = match extra {
                        "RE-ROLL QUIRKS" => g.card.system.archive.can_reroll(),
                        "TRANSPLANT QUIRK" => g.card.system.archive.can_transplant(),
                        _ => true,
                    };
                    let bank = g.campaign.as_ref().map_or(0, |c| c.inventory[side_ix(a.side)].currency);
                    if !allowed {
                        a.msg = format!(
                            "LOCKED: HARVEST MORE FROM THE OTHER SIDE ({}/{})",
                            g.card.system.archive.cross_harvests,
                            if extra == "RE-ROLL QUIRKS" { 20 } else { 40 }
                        );
                    } else if bank < cost {
                        a.msg = format!("NEED {cost} {}", a.side.currency());
                    } else {
                        let mut rng = isl_sim::rng::SimRng::new((g.time * 1000.0) as u64);
                        let ok = match extra {
                            "REPAIR PART" => a.lo.slots[s.index()].as_mut().is_some_and(|p| p.repair(c)),
                            "RE-ROLL QUIRKS" => a.lo.slots[s.index()].as_mut().map(|p| p.reroll(&mut rng, c)).is_some(),
                            _ => {
                                let next = Slot::ALL[(s.index() + 1) % 7];
                                let q = a.lo.slots[s.index()]
                                    .as_mut()
                                    .and_then(|p| if p.quirks.is_empty() { None } else { Some(p.quirks.remove(0)) });
                                match (q, a.lo.slots[next.index()].as_mut()) {
                                    (Some(q), Some(t)) => {
                                        t.quirks.push(q);
                                        true
                                    }
                                    _ => false,
                                }
                            }
                        };
                        if ok {
                            if let Some(cp) = g.campaign.as_mut() {
                                cp.inventory[side_ix(a.side)].currency -= cost;
                            }
                            a.lo.chassis = None;
                            a.msg = format!("{} DONE ON {}", extra, s.name_for(a.side));
                        } else {
                            a.msg = "NOTHING TO DO ON THAT PART".into();
                        }
                    }
                }
                "FABRICATE" => {
                    a.mode = AMode::Fabricate;
                    a.list_sel = 0;
                }
                "SAVE TO CARD" => {
                    let lo = a.lo.clone();
                    let blocks = a.resolved(c).block_cost;
                    a.msg = if g.save_build(&lo) { format!("SAVED ({blocks} BLOCKS)") } else { "CARD FULL".into() };
                }
                "EXPORT CODE" => {
                    let code = isl_save::codes::export_build(&a.lo);
                    let dir = g.paths.exports();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(dir.join(format!("{}.txt", a.lo.name.replace(' ', "_"))), &code);
                    a.msg = code;
                }
                "TEST ARENA" => {
                    let lo = a.lo.clone();
                    if let Some(cp) = g.campaign.as_mut() {
                        cp.active[side_ix(a.side)] = lo.clone();
                    }
                    g.start_launch(Launch::Arena { loadout: Box::new(lo) });
                    return std::mem::replace(&mut g.screen, Screen::Mission);
                }
                "RENAME" => {
                    let names = [
                        "VIGIL",
                        "ROOK",
                        "MOTHWING",
                        "UNDERTOW",
                        "PENITENT",
                        "SALT-9",
                        "THE LATE SHIFT",
                        "HARROW",
                        "LANTERN",
                        "GRAVE GOODS",
                    ];
                    let k = names.iter().position(|n| *n == a.lo.name).map_or(0, |i| (i + 1) % names.len());
                    a.lo.name = names[k].to_string();
                    a.lo.stencil = a.lo.name.clone();
                }
                _ => {
                    // DONE
                    let issues = a.issues(c);
                    if !issues.is_empty() {
                        a.msg = issues.iter().map(|i| i.text()).collect::<Vec<_>>().join(", ");
                        return Screen::Foundry(Box::new(a));
                    }
                    if let Some(cp) = g.campaign.as_mut() {
                        cp.active[side_ix(a.side)] = a.lo.clone();
                    }
                    g.save_card();
                    return back_from_foundry(g);
                }
            }
            if a.sel < 7 {
                a.paint_sel = a.sel;
            }
            Screen::Foundry(Box::new(a))
        }
        AMode::Pick => {
            let slot = a.slot().unwrap();
            let inv = g.campaign.as_ref().map(|c| [c.inventory[0].parts.clone(), c.inventory[1].parts.clone()]).unwrap_or(empty);
            let cands = foundry::candidates(&inv, c, slot);
            let mut ls = a.list_sel;
            let nv = nav(p, &mut ls, cands.len().max(1), audio);
            a.list_sel = ls;
            a.paint_sel = a.sel;
            match nv {
                Nav::Back => a.mode = AMode::Slots,
                Nav::Confirm if !cands.is_empty() => {
                    let cd = cands[a.list_sel];
                    if let Some(cp) = g.campaign.as_mut() {
                        let new = cp.inventory[cd.inv].parts.remove(cd.idx);
                        if let Some(old) = a.lo.slots[slot.index()].take() {
                            // Premade parts are always available; don't pollute the inventory with copies.
                            if !matches!(old.origin, isl_sim::parts::Origin::Premade(_)) {
                                cp.store_parts(vec![old], c);
                            }
                        }
                        a.lo.slots[slot.index()] = Some(new);
                        a.lo.chassis = None;
                        if a.lo.name.ends_with("SAINT")
                            || a.lo.name.contains("LEVIATHAN")
                            || c.chassis.iter().any(|ch| ch.name == a.lo.name)
                        {
                            a.lo.name = "CUSTOM".into();
                        }
                    }
                    a.mode = AMode::Slots;
                }
                _ => {}
            }
            Screen::Foundry(Box::new(a))
        }
        AMode::Premade => {
            let list: Vec<&ChassisDef> =
                c.chassis.iter().filter(|ch| ch.side == a.side && g.card.system.archive.chassis_available(ch)).collect();
            let mut ls = a.list_sel;
            let nv = nav(p, &mut ls, list.len(), audio);
            a.list_sel = ls;
            match nv {
                Nav::Back => a.mode = AMode::Slots,
                Nav::Confirm => {
                    let ch = list[a.list_sel];
                    if let Some(cp) = g.campaign.as_mut() {
                        let old: Vec<_> =
                            a.lo.slots
                                .iter_mut()
                                .filter_map(|s| s.take())
                                .filter(|p| !matches!(p.origin, isl_sim::parts::Origin::Premade(_)))
                                .collect();
                        cp.store_parts(old, c);
                    }
                    let lic = g.card.system.archive.licenses.get(&ch.id).map_or(0, |l| l.rank);
                    // Licence rank 3 unlocks the alternate starting loadout.
                    let parts = if lic >= 3 && !ch.alt_parts.is_empty() && p.down(Action::Special) {
                        ch.alt_parts[0].clone()
                    } else {
                        ch.parts.clone()
                    };
                    a.lo = Loadout::premade_with(ch, &parts, c, (g.time * 100.0) as u64);
                    a.mode = AMode::Slots;
                }
                _ => {}
            }
            Screen::Foundry(Box::new(a))
        }
        AMode::Fabricate => {
            let list: Vec<&PartDef> = c.parts.iter().filter(|d| g.card.system.archive.codex.contains(&d.id)).collect();
            let mut ls = a.list_sel;
            let nv = nav(p, &mut ls, list.len().max(1), audio);
            a.list_sel = ls;
            match nv {
                Nav::Back => a.mode = AMode::Slots,
                Nav::Confirm if !list.is_empty() => {
                    let d = list[a.list_sel];
                    let cost = isl_sim::archive::Archive::fabricate_cost(d);
                    if let Some(cp) = g.campaign.as_mut() {
                        let bank = &mut cp.inventory[side_ix(a.side)].currency;
                        if *bank >= cost {
                            *bank -= cost;
                            let uid = cp.uid();
                            let mut rng = isl_sim::rng::SimRng::new(uid);
                            if let Some(part) = g.card.system.archive.fabricate(d, uid, &mut rng, c) {
                                cp.store_parts(vec![part], c);
                                a.msg = format!("FABRICATED {} (SCARRED)", d.name);
                            }
                        } else {
                            a.msg = format!("NEED {cost} {}", a.side.currency());
                        }
                    }
                }
                _ => {}
            }
            Screen::Foundry(Box::new(a))
        }
        AMode::Paint => {
            let mut ls = a.list_sel;
            let nv = nav(p, &mut ls, 3, audio);
            a.list_sel = ls;
            match nv {
                Nav::Back | Nav::Confirm => a.mode = AMode::Slots,
                Nav::Left | Nav::Right => {
                    let d: i32 = if p.hit(Action::MenuRight) { 1 } else { -1 };
                    if a.list_sel < 2 {
                        let cur = PALETTE.iter().position(|c| *c == a.lo.paint[a.list_sel]).unwrap_or(0) as i32;
                        a.lo.paint[a.list_sel] = PALETTE[(cur + d).rem_euclid(PALETTE.len() as i32) as usize];
                    } else {
                        a.lo.decal = (a.lo.decal as i32 + d).rem_euclid(8) as u8;
                    }
                    a.lo.chassis = None;
                }
                _ => {}
            }
            Screen::Foundry(Box::new(a))
        }
    }
}

fn back_from_foundry(g: &Game) -> Screen {
    if let Some(c) = &g.campaign { Screen::WarMap { sel: 0, side: c.last_side } } else { Screen::Main { sel: 0 } }
}

fn settings_rows(g: &Game, tab: usize) -> Vec<(String, String)> {
    let s = &g.settings;
    let pct = |x: f32| format!("{}%", (x * 100.0).round());
    let onoff = |b: bool| if b { "ON".to_string() } else { "OFF".to_string() };
    match tab {
        0 => {
            let (w, h) = s.low_res();
            vec![
                ("RESOLUTION".into(), format!("{w}x{h}")),
                ("ASPECT".into(), if s.widescreen { "16:9".into() } else { "4:3".into() }),
                ("INTEGER SCALE".into(), onoff(s.integer_scale)),
                ("VERTEX JITTER".into(), pct(s.vertex_jitter)),
                ("DITHER".into(), pct(s.dither)),
                ("CRT".into(), onoff(s.crt)),
                ("COMPOSITE".into(), onoff(s.composite)),
                ("SCANLINES".into(), pct(s.scanlines)),
                ("VSYNC".into(), onoff(s.vsync)),
                ("FULLSCREEN".into(), onoff(s.fullscreen)),
                ("PAUSE UNFOCUSED".into(), onoff(s.pause_unfocused)),
            ]
        }
        1 => vec![
            ("MASTER".into(), pct(s.master)),
            ("SFX".into(), pct(s.sfx)),
            ("MUSIC".into(), pct(s.music)),
            ("SPU MODE".into(), onoff(s.spu)),
        ],
        2 => vec![
            ("SCREEN SHAKE".into(), pct(s.screen_shake)),
            ("PHOTOSENSITIVE MODE".into(), onoff(s.photosensitive)),
            ("SUBTITLE SIZE".into(), format!("x{}", s.subtitle_size as u32)),
            ("COLOUR PALETTE".into(), format!("{:?}", s.palette).to_uppercase()),
            ("STEADY HANDS (PARRY)".into(), onoff(s.steady_hands)),
            ("OLD BONES (SLOW BOSSES)".into(), onoff(s.old_bones)),
            ("INSURANCE (DEATH DROP)".into(), onoff(s.insurance)),
            ("SHOW I-FRAMES".into(), onoff(s.show_iframes)),
            ("RUMBLE".into(), onoff(s.rumble)),
            ("GUARD".into(), if s.bindings.guard_toggle { "TOGGLE".into() } else { "HOLD".into() }),
            ("INVERT CAMERA Y".into(), onoff(s.bindings.invert_y)),
        ],
        _ => isl_input::Action::REBINDABLE
            .iter()
            .map(|a| {
                let b =
                    s.bindings.map.get(a).map_or(String::new(), |v| v.iter().take(3).map(|b| b.label()).collect::<Vec<_>>().join(" / "));
                (a.label().to_string(), b)
            })
            .collect(),
    }
}

fn update_settings(g: &mut Game, mut tab: usize, mut sel: usize, p: &PlayerInput, input: &mut Input, audio: &mut Audio) -> Screen {
    if input.capture.is_some() {
        return Screen::Settings { tab, sel };
    }
    if p.hit(Action::TabNext) {
        tab = (tab + 1) % 4;
        sel = 0;
    }
    if p.hit(Action::TabPrev) {
        tab = (tab + 3) % 4;
        sel = 0;
    }
    let n = settings_rows(g, tab).len();
    let nv = nav(p, &mut sel, n, audio);
    if nv == Nav::Back {
        g.save_settings();
        input.bindings = g.settings.bindings.clone();
        g.requests.push(Request::ApplyVideo);
        if g.mission.is_some() {
            g.pause = Some(0);
            return Screen::Mission;
        }
        return Screen::Main { sel: 9 };
    }
    let d: f32 = match nv {
        Nav::Left => -1.0,
        Nav::Right | Nav::Confirm => 1.0,
        _ => return Screen::Settings { tab, sel },
    };
    let s = &mut g.settings;
    let step = |x: &mut f32, d: f32| *x = (*x + d * 0.1).clamp(0.0, 1.0);
    match (tab, sel) {
        (0, 0) => s.resolution = (s.resolution as i32 + d as i32).rem_euclid(RESOLUTIONS.len() as i32) as usize,
        (0, 1) => s.widescreen = !s.widescreen,
        (0, 2) => s.integer_scale = !s.integer_scale,
        (0, 3) => step(&mut s.vertex_jitter, d),
        (0, 4) => step(&mut s.dither, d),
        (0, 5) => s.crt = !s.crt,
        (0, 6) => s.composite = !s.composite,
        (0, 7) => step(&mut s.scanlines, d),
        (0, 8) => s.vsync = !s.vsync,
        (0, 9) => s.fullscreen = !s.fullscreen,
        (0, 10) => s.pause_unfocused = !s.pause_unfocused,
        (1, 0) => step(&mut s.master, d),
        (1, 1) => step(&mut s.sfx, d),
        (1, 2) => step(&mut s.music, d),
        (1, 3) => s.spu = !s.spu,
        (2, 0) => step(&mut s.screen_shake, d),
        (2, 1) => s.photosensitive = !s.photosensitive,
        (2, 2) => s.subtitle_size = if s.subtitle_size >= 2.0 { 1.0 } else { 2.0 },
        (2, 3) => {
            s.palette = match s.palette {
                Palette::Standard => Palette::Deuteranopia,
                Palette::Deuteranopia => Palette::Protanopia,
                Palette::Protanopia => Palette::Tritanopia,
                Palette::Tritanopia => Palette::Standard,
            }
        }
        (2, 4) => s.steady_hands = !s.steady_hands,
        (2, 5) => s.old_bones = !s.old_bones,
        (2, 6) => s.insurance = !s.insurance,
        (2, 7) => s.show_iframes = !s.show_iframes,
        (2, 8) => s.rumble = !s.rumble,
        (2, 9) => s.bindings.guard_toggle = !s.bindings.guard_toggle,
        (2, 10) => s.bindings.invert_y = !s.bindings.invert_y,
        (3, i) if nv == Nav::Confirm => {
            input.capture = Some(isl_input::Action::REBINDABLE[i]);
        }
        _ => {}
    }
    audio.settings.master = g.settings.master;
    audio.settings.sfx = g.settings.sfx;
    audio.settings.music = g.settings.music;
    audio.set_spu(g.settings.spu);
    audio.apply_volumes();
    input.rumble_enabled = g.settings.rumble;
    Screen::Settings { tab, sel }
}

fn archive_rows(g: &Game, tab: usize) -> Vec<String> {
    let a = &g.card.system.archive;
    let c = &g.content;
    match tab {
        0 => {
            let mut v = vec![format!("REGISTERED {}/{}  CROSS-HARVESTS {}", a.codex.len(), c.parts.len(), a.cross_harvests)];
            for d in &c.parts {
                if a.codex.contains(&d.id) {
                    v.push(format!("{:<22} {:>5} {}", d.name, isl_sim::archive::Archive::fabricate_cost(d), d.side.currency()));
                }
            }
            v
        }
        1 => c
            .chassis
            .iter()
            .map(|ch| {
                let l = a.licenses.get(&ch.id).cloned().unwrap_or_default();
                let lock = if a.chassis_available(ch) { "" } else { " [LOCKED]" };
                format!("{:<16} RANK {:>2} XP {:>6} WINS VS {:>2}{}", ch.name, l.rank, l.xp, l.wins_against, lock)
            })
            .collect(),
        2 => {
            let r = &a.record;
            let mut v = vec![
                format!("WARS FOUGHT       {}", r.wars),
                format!("MISSIONS          {}", r.missions),
                format!("DEATHS            {}", r.deaths),
                format!("NEMESES DEFEATED  {}", r.nemeses_defeated),
                format!("CORRUPTED CHAIN   {}", r.corrupted_chain),
                format!("TIDE RUN BEST     {}", a.run_best),
                format!("DISC ROT UNLOCKED {}", a.disc_rot_unlocked),
                format!("HEADS TAKEN       {}/4", a.heads_taken.len()),
                format!("HEARTS TAKEN      {}/4", a.hearts_taken.len()),
            ];
            for e in &c.endings {
                v.push(format!("ENDING {:<18} {}", e.name, if r.endings.contains(&e.id) { "SEEN" } else { "---" }));
            }
            for cd in &c.conditions {
                v.push(format!("CONDITION {:<15} {}", cd.name, if a.unlocked_conditions.contains(&cd.id) { "UNLOCKED" } else { "LOCKED" }));
            }
            v
        }
        3 => {
            let mut v: Vec<String> = c.bosses.iter().map(|b| format!("{}: {}", b.name, b.bestiary)).collect();
            v.extend(c.enemies.iter().map(|e| format!("{}: {}", e.name, e.bestiary)));
            v
        }
        _ => {
            if a.rivals.is_empty() {
                vec!["NO RIVALS YET. SAVE A BUILD. DIE ANGRY. WIN OFTEN.".into()]
            } else {
                a.rivals.iter().map(|r| format!("{:<13} {:<12} LVL {} BEATEN {}", r.title, r.loadout.name, r.level, r.defeated)).collect()
            }
        }
    }
}

// ====================================================================== drawing

fn stage_light() -> Lighting {
    Lighting {
        ambient: Vec3::new(0.35, 0.36, 0.42),
        dirs: vec![
            (Vec3::new(-0.5, -0.8, -0.4).normalize(), Vec3::new(1.0, 0.92, 0.8)),
            (Vec3::new(0.7, -0.2, 0.6).normalize(), Vec3::new(0.25, 0.35, 0.55)),
        ],
        points: vec![],
    }
}

fn stage(dl: &mut DrawList, light: &Lighting, t: f32, city: bool) {
    let mut m = meshgen::Mesh::default();
    meshgen::add_box(
        &mut m,
        Vec3::new(-60.0, -1.0, -60.0),
        Vec3::new(60.0, 0.0, 60.0),
        [120, 120, 125],
        textures::layer("concrete") as u16,
        6.0,
        [true; 6],
        false,
    );
    if city {
        let mut s = 7u32;
        for i in 0..14 {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let a = i as f32 / 14.0 * std::f32::consts::TAU;
            let r = 62.0 + (s % 20) as f32;
            let h = 10.0 + (s % 30) as f32;
            let c = Vec3::new(a.sin() * r, 0.0, a.cos() * r);
            meshgen::add_box(
                &mut m,
                c - Vec3::new(5.0, 0.0, 5.0),
                c + Vec3::new(5.0, h, 5.0),
                [100, 96, 104],
                textures::layer("window") as u16,
                4.0,
                [true; 6],
                false,
            );
        }
    }
    isl_render::push_mesh(dl, &m, Mat4::IDENTITY, light, Vec3::ONE, 255);
    let _ = t;
}

fn pose_lo<'a>(lo: &'a Loadout, c: &Content, pos: Vec3, yaw: f32, t: f32, anim: Anim, id: u32) -> Pose<'a> {
    Pose {
        id,
        loadout: lo,
        destroyed: std::array::from_fn(|i| lo.slots[i].is_none()),
        rig: rig_of(lo, c),
        pos,
        yaw,
        size: size_of(lo, c),
        lean: build::resolve(lo, c, &[], [false; 7]).lean,
        altitude: if rig_of(lo, c) == RigKind::Moth { 2.0 } else { 0.0 },
        anim,
        tint: Vec3::ONE,
        alpha: 255,
        bodies: 10,
        burrowed: false,
        moving: 0.0,
        time: t,
    }
}

fn view_for(g: &Game, eye: Vec3, at: Vec3, fog: [f32; 3], far: f32) -> View {
    let (w, h) = g.settings.low_res();
    let aspect = g.settings.aspect();
    View {
        view_proj: isl_render::projection(0.9, aspect, far) * isl_render::look_at(eye, at, Vec3::Y),
        cam: eye,
        fog_color: fog,
        fog_near: far * 0.4,
        fog_far: far,
        flash: 0.0,
        viewport: [0, 0, w, h],
        draw_static: false,
        clear: true,
    }
}

pub fn draw(g: &mut Game, views: &mut Vec<(View, DrawList)>, post: &mut PostSettings) {
    let (w, h) = g.low();
    let t = g.time;
    let screen = std::mem::replace(&mut g.screen, Screen::Boot(0.0));
    match &screen {
        Screen::Boot(bt) => {
            g.ui.rect(0.0, 0.0, w, h, col::BLACK);
            let a = (bt.min(1.0) * 255.0) as u8;
            let cx = w * 0.5;
            // An original boot sting: a rotating diamond and the studio name.
            for i in 0..4 {
                let r = 14.0 + i as f32 * 6.0;
                let ang = bt * 1.5 + i as f32;
                let x = cx + ang.cos() * r;
                let y = h * 0.42 + ang.sin() * r * 0.5;
                g.ui.rect(x - 2.0, y - 2.0, 4.0, 4.0, [200, 180, 120, a]);
            }
            g.ui.text_center(cx, h * 0.62, "HARROWMOUTH ENTERTAINMENT", [220, 220, 220, a]);
            g.ui.text_center(cx, h * 0.62 + 12.0, "LICENSED BY NOBODY", [120, 120, 120, a]);
        }
        Screen::Title(tt) => {
            let light = stage_light();
            let mut dl = DrawList::default();
            stage(&mut dl, &light, t, true);
            let saint = Loadout::premade(g.content.chassis_def("iron_saint").unwrap(), &g.content, 1);
            let kaiju = Loadout::premade(g.content.chassis_def("leviathan").unwrap(), &g.content, 1);
            let p1 = pose_lo(&saint, &g.content, Vec3::new(-9.0, 0.0, 0.0), 0.9, t, Anim::Idle, 1);
            let p2 = pose_lo(&kaiju, &g.content, Vec3::new(10.0, 0.0, -2.0), -1.2 + (t * 0.3).sin() * 0.2, t, Anim::Walk(t * 1.5), 2);
            g.preview.draw(&mut dl, &g.content, &p1, &light);
            g.preview.draw(&mut dl, &g.content, &p2, &light);
            let a = t * 0.08;
            let v = view_for(g, Vec3::new(a.sin() * 30.0, 12.0, a.cos() * 30.0 + 4.0), Vec3::new(0.0, 6.0, 0.0), [0.08, 0.1, 0.16], 130.0);
            views.push((v, dl));
            g.ui.text_center_big(w * 0.5 + 1.0, h * 0.16, "IRON SAINT", col::GOLD, 3.0);
            g.ui.text_center_big(w * 0.5, h * 0.16 + 26.0, "// LEVIATHAN", [140, 220, 230, 255], 2.0);
            if ((tt * 2.0) as u32).is_multiple_of(2) {
                g.ui.text_center(w * 0.5, h * 0.8, "PRESS START", col::WHITE);
            }
            g.ui.text_center(w * 0.5, h - 12.0, "(C) 1998/2026 HARROWMOUTH. NO MEMORY CARD REQUIRED.", col::DARK);
        }
        Screen::Main { sel } => {
            draw_bg(g, views, t);
            let items = main_items(g);
            let labels: Vec<String> = items.iter().map(|i| i.0.clone()).collect();
            let en: Vec<bool> = items.iter().map(|i| i.1).collect();
            g.ui.panel(16.0, 30.0, 150.0, 14.0 + labels.len() as f32 * 11.0 * g.ui.text_scale.max(1.0));
            g.ui.menu(28.0, 38.0, &labels, *sel, &en);
            g.ui.text_big(w - 150.0, 12.0, "IRON SAINT", col::GOLD, 2.0);
            let used = g.card.used();
            g.ui.text_right(w - 8.0, h - 12.0, &format!("MEMORY CARD {used}/15 BLOCKS"), col::GREY);
        }
        Screen::NewWar { sel, condition, rot, use_corrupt } => {
            draw_bg(g, views, t);
            let conds: Vec<String> = std::iter::once("NONE".to_string())
                .chain(
                    g.content
                        .conditions
                        .iter()
                        .filter(|c| g.card.system.archive.unlocked_conditions.contains(&c.id))
                        .map(|c| c.name.clone()),
                )
                .collect();
            let rows = vec![
                format!("CONDITION   < {} >", conds.get(*condition).cloned().unwrap_or_default()),
                format!("DISC ROT    < {} >", rot),
                format!("CORRUPTED   < {} >", if *use_corrupt { "CARRY" } else { "LEAVE" }),
                "BEGIN THE WAR".into(),
                "BACK".into(),
            ];
            g.ui.panel(16.0, 40.0, w - 32.0, 110.0);
            g.ui.text(24.0, 46.0, "NEW WAR", col::GOLD);
            g.ui.menu(32.0, 60.0, &rows, *sel, &[true, true, g.card.corrupted().is_some(), true, true]);
            let desc = match sel {
                0 => g
                    .content
                    .conditions
                    .iter()
                    .find(|c| Some(&c.name) == conds.get(*condition))
                    .map_or("A normal war.".into(), |c| c.desc.clone()),
                1 => g.content.disc_rot.iter().filter(|r| r.tier <= *rot).map(|r| r.rule.clone()).collect::<Vec<_>>().join(" "),
                2 => "Carry the corrupted block from your last war into this one.".into(),
                _ => "Harrowmouth, the seventh war.".into(),
            };
            g.ui.text_wrap(24.0, 160.0, w - 48.0, &desc, col::GREY);
        }
        Screen::WarMap { sel, side } => draw_warmap(g, *sel, *side),
        Screen::Briefing { district, side, sel } => draw_briefing(g, district, *side, *sel),
        Screen::DiscSwap { t: dt, to, .. } => {
            g.ui.rect(0.0, 0.0, w, h, [6, 6, 10, 255]);
            let cx = w * 0.5;
            let cy = h * 0.42;
            let spin = dt * 9.0;
            for r in (6..40).step_by(4) {
                let shade = (120.0 + (spin + r as f32 * 0.3).sin() * 60.0) as u8;
                g.ui.outline(
                    cx - r as f32,
                    cy - r as f32 * 0.98,
                    r as f32 * 2.0,
                    r as f32 * 1.96,
                    [shade, shade, (shade as u16 + 40).min(255) as u8, 255],
                );
            }
            g.ui.rect(cx - 5.0, cy - 5.0, 10.0, 10.0, col::BLACK);
            g.ui.text_center_big(cx, h * 0.72, &format!("PLEASE INSERT DISC {}", to.disc()), col::WHITE, 2.0);
            g.ui.text_center(cx, h * 0.72 + 20.0, &format!("{} SIDE", to.name()), col::GREY);
        }
        Screen::Loading { t: lt, lore, next } => {
            g.ui.rect(0.0, 0.0, w, h, col::BLACK);
            let tips = &g.content.lore.loading;
            let tip = &tips[lore % tips.len().max(1)];
            let name = match &**next {
                Launch::War { district, .. } => g.content.district(district).map_or(String::new(), |d| d.name.clone()),
                Launch::RunFight => "TIDE RUN".into(),
                Launch::Gauntlet { .. } => "NEMESIS GAUNTLET".into(),
                Launch::Arena { .. } => "TEST ARENA".into(),
                Launch::Versus { .. } => "LINK CABLE".into(),
            };
            g.ui.text_center_big(w * 0.5, h * 0.3, &name, col::GOLD, 2.0);
            g.ui.text_wrap(24.0, h * 0.55, w - 48.0, tip, col::GREY);
            let k = (lt * 8.0) as usize % 4;
            g.ui.text_right(w - 10.0, h - 14.0, &format!("NOW LOADING{}", ".".repeat(k)), col::WHITE);
        }
        Screen::Mission => draw_mission(g, views, post),
        Screen::Results { title, lines, .. } => {
            draw_bg(g, views, t);
            g.ui.panel(14.0, 20.0, w - 28.0, h - 40.0);
            g.ui.text_center_big(w * 0.5, 28.0, title, col::GOLD, 2.0);
            for (i, l) in lines.iter().take(18).enumerate() {
                g.ui.text(24.0, 52.0 + i as f32 * 9.0, l, col::WHITE);
            }
            g.ui.text_right(w - 22.0, h - 32.0, "CONFIRM", col::GREY);
        }
        Screen::Foundry(a) => draw_foundry(g, a, views),
        Screen::Card { sel, confirm } => draw_card(g, *sel, *confirm),
        Screen::Codes { text, msg } => {
            g.ui.rect(0.0, 0.0, w, h, [10, 10, 30, 255]);
            g.ui.panel(10.0, 40.0, w - 20.0, 80.0);
            g.ui.text(18.0, 46.0, "MEMORY CARD EXCHANGE", col::GOLD);
            g.ui.text_wrap(
                18.0,
                62.0,
                w - 36.0,
                &format!("{text}{}", if ((t * 2.0) as u32).is_multiple_of(2) { "_" } else { " " }),
                col::WHITE,
            );
            g.ui.text_wrap(18.0, 100.0, w - 36.0, msg, col::GREY);
        }
        Screen::Archive { tab, sel } => {
            g.ui.rect(0.0, 0.0, w, h, [16, 14, 10, 255]);
            // Framed like a scanned 1998 strategy guide.
            g.ui.rect(6.0, 6.0, w - 12.0, h - 12.0, [210, 200, 170, 255]);
            g.ui.rect(10.0, 10.0, w - 20.0, h - 20.0, [235, 228, 205, 255]);
            let tabs = ["CODEX", "LICENSES", "WAR RECORD", "BESTIARY", "RIVALS"];
            let mut x = 14.0;
            for (i, tn) in tabs.iter().enumerate() {
                let c = if i == *tab { [140, 30, 20, 255] } else { [90, 80, 70, 255] };
                x += g.ui.text_raw(x, 14.0, tn, c, 1.0) + 10.0;
            }
            let rows = archive_rows(g, *tab);
            let start = sel.saturating_sub(10);
            let mut y = 30.0;
            for (i, r) in rows.iter().enumerate().skip(start).take(22) {
                let c = if i == *sel { [140, 30, 20, 255] } else { [40, 36, 30, 255] };
                let lines = isl_ui::wrap(r, ((w - 30.0) / 6.0) as usize);
                for l in lines.iter().take(if *tab == 3 { 3 } else { 1 }) {
                    g.ui.text_raw(16.0, y, l, c, 1.0);
                    y += 9.0;
                }
                if y > h - 20.0 {
                    break;
                }
            }
        }
        Screen::Settings { tab, sel } => {
            g.ui.rect(0.0, 0.0, w, h, [8, 10, 24, 255]);
            let tabs = ["VIDEO", "AUDIO", "ACCESSIBILITY", "CONTROLS"];
            let mut x = 12.0;
            for (i, tn) in tabs.iter().enumerate() {
                x += g.ui.text(x, 8.0, tn, if i == *tab { col::GOLD } else { col::GREY }) + 12.0;
            }
            let rows = settings_rows(g, *tab);
            let start = sel.saturating_sub(18);
            for (i, (k, v)) in rows.iter().enumerate().skip(start).take(20) {
                let y = 24.0 + (i - start) as f32 * 10.0;
                let c = if i == *sel { col::GOLD } else { col::WHITE };
                g.ui.text(18.0, y, k, c);
                g.ui.text_right(w - 14.0, y, v, c);
            }
            let hint = if g.mission.is_some() { "TAB SWITCH  BACK RETURNS TO PAUSE" } else { "TAB SWITCH  LEFT/RIGHT CHANGE  BACK SAVES" };
            g.ui.text(12.0, h - 12.0, hint, col::DARK);
        }
        Screen::Cutscene(f) => draw_cutscene(g, f, views, post),
        Screen::RunSetup { sel, omens, daily } => {
            draw_bg(g, views, t);
            g.ui.panel(12.0, 16.0, w - 24.0, h - 32.0);
            if *daily {
                let (seed, tag) = Game::daily_seed_today();
                g.ui.text(22.0, 24.0, &format!("DAILY TIDE {tag}"), col::GOLD);
                g.ui.text(22.0, 36.0, &format!("SEED {:08X}", seed as u32), col::GREY);
                let best = g.card.system.archive.daily_best.get(&tag).copied().unwrap_or(0);
                g.ui.text(22.0, 48.0, &format!("YOUR BEST TODAY {best}"), col::GREY);
                g.ui.menu(30.0, 70.0, &["START".into(), "BACK".into()], *sel, &[]);
            } else {
                g.ui.text(22.0, 24.0, "TIDE RUN - CHOOSE OMENS", col::GOLD);
                let mut rows: Vec<String> = g
                    .content
                    .omens
                    .iter()
                    .zip(omens)
                    .map(|(o, on)| format!("[{}] {} x{:.1}", if *on { "X" } else { " " }, o.name, o.reward))
                    .collect();
                rows.push("START".into());
                rows.push("BACK".into());
                g.ui.menu(30.0, 38.0, &rows, *sel, &[]);
                if let Some(o) = g.content.omens.get(*sel) {
                    g.ui.text_wrap(22.0, h - 40.0, w - 44.0, &o.desc, col::GREY);
                }
            }
        }
        Screen::RunDraft { sel } => {
            draw_bg(g, views, t);
            g.ui.panel(12.0, 16.0, w - 24.0, h - 32.0);
            if let Some(r) = &g.run {
                g.ui.text(22.0, 24.0, &format!("DRAFT - FIGHT {}/{}  SCORE {}", r.index, r.fights.len(), r.score), col::GOLD);
                let next = r.current().map_or("-".into(), |f| {
                    format!("NEXT: {} AS {}", g.content.district(&f.district).map_or("", |d| &d.name), f.side.name())
                });
                g.ui.text(22.0, 36.0, &next, col::GREY);
                let mut rows: Vec<String> = r
                    .offer
                    .iter()
                    .map(|p| {
                        let d = g.content.part(&p.def);
                        format!(
                            "{} {} [{}] -> {}",
                            d.map_or("", |d| d.slot.name_for(d.side)),
                            d.map_or(p.def.as_str(), |d| d.name.as_str()),
                            p.condition.name(),
                            d.map_or("", |d| d.side.name())
                        )
                    })
                    .collect();
                rows.push("SKIP".into());
                g.ui.menu(30.0, 54.0, &rows, *sel, &[]);
                if let Some(p) = r.offer.get(*sel) {
                    let q: Vec<String> =
                        p.quirks.iter().filter_map(|q| g.content.quirk(q)).map(|q| format!("{}: {}", q.name, q.desc)).collect();
                    g.ui.text_wrap(22.0, h - 60.0, w - 44.0, &q.join(" "), col::GREY);
                }
            }
        }
        Screen::Gauntlet { sel, side } => {
            draw_bg(g, views, t);
            let q = g.nemeses_of(side.other());
            g.ui.panel(12.0, 16.0, w - 24.0, h - 32.0);
            g.ui.text(22.0, 24.0, &format!("NEMESIS GAUNTLET - YOU PLAY {} < >", side.name()), col::GOLD);
            g.ui.menu(30.0, 40.0, &["BEGIN".into(), "BACK".into()], *sel, &[!q.is_empty(), true]);
            for (i, n) in q.iter().take(12).enumerate() {
                g.ui.text(30.0, 70.0 + i as f32 * 9.0, &format!("{}. {}", i + 1, n.name), col::WHITE);
            }
            if q.is_empty() {
                g.ui.text_wrap(
                    30.0,
                    70.0,
                    w - 60.0,
                    "No ghosts yet. Save builds of the other side to the memory card; they come back as bosses here.",
                    col::GREY,
                );
            }
        }
        Screen::Versus { sel, a, b } => {
            draw_bg(g, views, t);
            let pa = versus_pool(g, Side::Saint);
            let pb = versus_pool(g, Side::Kaiju);
            g.ui.panel(12.0, 16.0, w - 24.0, 110.0);
            g.ui.text(22.0, 24.0, "LINK CABLE VERSUS", col::GOLD);
            let rows = vec![
                format!("P1 SAINT < {} >", pa[*a % pa.len()].name),
                format!("P2 KAIJU < {} >", pb[*b % pb.len()].name),
                "FIGHT".into(),
                "BACK".into(),
            ];
            g.ui.menu(30.0, 40.0, &rows, *sel, &[]);
            g.ui.text_wrap(22.0, 140.0, w - 44.0, "P1: keyboard/mouse or pad 1. P2: pad 2 (or pad 1 if P1 uses the keyboard).", col::GREY);
        }
        Screen::Corrupt { sel, .. } => {
            g.ui.rect(0.0, 0.0, w, h, [4, 0, 8, 255]);
            g.ui.text_center(w * 0.5, 16.0, "THE MEMORY CARD IS CORRUPTED.", [230, 90, 220, 255]);
            g.ui.text_center(w * 0.5, 28.0, "CHOOSE ONE THING TO CARRY INTO THE NEXT WAR.", col::GREY);
            let opts: Vec<String> = corrupt_options(g).into_iter().map(|o| o.0).collect();
            g.ui.glitch = 0.4;
            g.ui.menu(28.0, 48.0, &opts, *sel, &[]);
            g.ui.glitch = 0.0;
        }
    }
    g.screen = screen;
}

fn draw_bg(g: &mut Game, views: &mut Vec<(View, DrawList)>, t: f32) {
    let light = stage_light();
    let mut dl = DrawList::default();
    stage(&mut dl, &light, t, true);
    let a = t * 0.05;
    let v = view_for(g, Vec3::new(a.sin() * 70.0, 30.0, a.cos() * 70.0), Vec3::new(0.0, 10.0, 0.0), [0.05, 0.06, 0.1], 110.0);
    views.push((v, dl));
}

fn draw_warmap(g: &mut Game, sel: usize, side: Side) {
    let (w, h) = g.low();
    let Some(c) = g.campaign.clone() else { return };
    g.ui.rect(0.0, 0.0, w, h, [10, 18, 30, 255]);
    // Sea and coastline.
    let mx = 8.0;
    let my = 22.0;
    let mw = w * 0.62;
    let mh = h - 60.0;
    g.ui.rect(mx, my, mw, mh, [30, 50, 70, 255]);
    for i in 0..10 {
        let y = my + mh * (0.25 + i as f32 * 0.07);
        let x0 = mx + mw * (0.05 + (i as f32 * 1.3).sin().abs() * 0.08);
        g.ui.rect(x0, y, mw - (x0 - mx) - 6.0, mh * 0.07 + 1.0, [70, 74, 66, 255]);
    }
    let rows = war_rows(g);
    for (i, r) in rows.iter().enumerate() {
        let Some(d) = g.content.district(r) else { continue };
        let Some(dw) = c.district(&d.id) else { continue };
        let st = dw.state();
        let (cc, glyph) = match st {
            DistrictState::Fortified => ([120, 200, 255, 255], icon::BLOCK),
            DistrictState::Contested => ([255, 230, 120, 255], icon::STAR),
            DistrictState::Flooded => ([80, 255, 200, 255], icon::DOWN),
            DistrictState::Ruined => ([160, 120, 110, 255], 'X'),
        };
        let x = mx + d.map_pos[0] * mw;
        let y = my + d.map_pos[1] * mh;
        if i == sel {
            g.ui.outline(x - 6.0, y - 6.0, 13.0, 13.0, col::GOLD);
        }
        g.ui.text(x - 2.0, y - 3.0, &glyph.to_string(), cc);
        let short: String = d.name.trim_start_matches("THE ").chars().take(11).collect();
        let tw = g.ui.text_width(&short);
        let lx = if x + 8.0 + tw > mx + mw - 2.0 { x - 6.0 - tw } else { x + 8.0 };
        g.ui.text(lx, y - 3.0, &short, if i == sel { col::GOLD } else { col::WHITE });
        if c.wrecks.iter().any(|wr| wr.district == d.id) {
            g.ui.text(x - 2.0, y + 6.0, "+", [255, 120, 220, 255]);
        }
    }
    // Side panel.
    let px = mx + mw + 6.0;
    let pw = w - px - 6.0;
    g.ui.panel(px, 6.0, pw, h - 12.0);
    let disc = format!("DISC {} {}", side.disc(), side.name());
    g.ui.text(
        px + 6.0,
        12.0,
        &format!("{} {}{}", icon::LEFT, disc, icon::RIGHT),
        if side == Side::Saint { [120, 200, 255, 255] } else { [80, 255, 200, 255] },
    );
    g.ui.text(px + 6.0, 24.0, &format!("WAR {}  TIDE {}", c.war, c.tide.tide_count), col::WHITE);
    g.ui.text(px + 6.0, 33.0, &format!("{:02}:00 {}", c.tide.hour_of_day(), c.tide.weather.name()), col::GREY);
    g.ui.text(px + 6.0, 42.0, &format!("TIDE {:+.1}", c.tide.tide()), col::GREY);
    g.ui.text(px + 6.0, 54.0, &format!("SALVAGE {}", c.inventory[0].currency), col::WHITE);
    g.ui.text(px + 6.0, 63.0, &format!("BIOMASS {}", c.inventory[1].currency), col::WHITE);
    g.ui.text(px + 6.0, 72.0, &format!("PARTS {}/{}", c.inventory[0].parts.len(), c.inventory[1].parts.len()), col::GREY);
    let bal = c.balance(&g.content);
    g.ui.text(px + 6.0, 84.0, &format!("BALANCE {:+}", bal), col::GREY);
    g.ui.bar(px + 6.0, 94.0, pw - 12.0, 4.0, 0.5 + bal as f32 / 36.0, [120, 200, 255, 255], [80, 255, 200, 255]);
    g.ui.text(px + 6.0, 102.0, &format!("WINS S{} K{}", c.missions[0], c.missions[1]), col::GREY);
    let extras = ["FOUNDRY (SAINT)", "NEST (KAIJU)", "WAIT ONE HOUR", "MEMORY CARD", "ARCHIVE", "MAIN MENU"];
    let nd = rows.len() - extras.len();
    for (i, e) in extras.iter().enumerate() {
        let y = 116.0 + i as f32 * 10.0;
        let s = sel == nd + i;
        if s {
            g.ui.text(px + 2.0, y, &icon::CURSOR.to_string(), col::GOLD);
        }
        g.ui.text(px + 9.0, y, e, if s { col::GOLD } else { col::WHITE });
    }
    if let Some(d) = rows.get(sel).and_then(|r| g.content.district(r))
        && let Some(dw) = c.district(&d.id)
    {
        g.ui.text(
            mx,
            h - 34.0,
            &format!("{} - {}  CONTROL {:+}  RUIN {}%", d.name, dw.state().name(), dw.control, (dw.ruin * 100.0) as u32),
            col::WHITE,
        );
        let lore = &d.lore[(g.time as usize / 5) % d.lore.len()];
        g.ui.text(mx, h - 24.0, lore, col::GREY);
    }
    if let Some(l) = c.log.last() {
        g.ui.text(mx, 8.0, l, col::GREY);
    }
}

fn draw_briefing(g: &mut Game, district: &str, side: Side, sel: usize) {
    let (w, h) = g.low();
    let Some(d) = g.content.district(district).cloned() else { return };
    let Some(c) = g.campaign.clone() else { return };
    let dw = c.district(district).cloned().unwrap();
    let st = dw.state();
    let sv = &d.states[st.index()];
    g.ui.rect(0.0, 0.0, w, h, [12, 12, 20, 255]);
    g.ui.panel(8.0, 8.0, w - 16.0, h - 16.0);
    g.ui.text_big(16.0, 14.0, &d.name, col::GOLD, 2.0);
    g.ui.text(16.0, 34.0, &format!("STATE {}   PLAYING {} (DISC {})", st.name(), side.name(), side.disc()), col::WHITE);
    let foes = match side {
        Side::Saint => &sv.saint_enemies,
        Side::Kaiju => &sv.kaiju_enemies,
    };
    let names: Vec<String> = foes.iter().filter_map(|e| g.content.enemy(e)).map(|e| e.name.clone()).collect();
    g.ui.text_wrap(16.0, 46.0, w - 32.0, &format!("HOSTILES: {}", names.join(", ")), col::GREY);
    let boss = match side {
        Side::Saint => &d.saint_boss,
        Side::Kaiju => &d.kaiju_boss,
    };
    if let Some(b) = g.content.boss(boss) {
        g.ui.text(16.0, 66.0, &format!("BOSS: {} - {}", b.name, b.title), [255, 160, 140, 255]);
        g.ui.text_wrap(16.0, 76.0, w - 32.0, &b.bestiary, col::GREY);
    }
    if let Some(ct) = dw.contract.as_ref().and_then(|id| g.content.contracts.iter().find(|x| &x.id == id)) {
        let q = g.content.quirk(&ct.reward_quirk).map_or("", |q| q.name.as_str());
        g.ui.text(16.0, 100.0, &format!("CONTRACT: {} (REWARD: {}, {})", ct.name, q, ct.reward_currency), col::CYAN);
    }
    let evs: Vec<String> = c
        .events
        .iter()
        .filter(|e| e.district == district)
        .filter_map(|e| g.content.events.iter().find(|x| x.id == e.id))
        .map(|e| e.name.clone())
        .collect();
    if !evs.is_empty() {
        g.ui.text(16.0, 110.0, &format!("EVENT: {}", evs.join(", ")), col::MAGENTA);
    }
    let wrecks = c.wrecks_in(district);
    let mine = wrecks.iter().filter(|w| w.side == side && w.currency > 0).count();
    let theirs = wrecks.iter().filter(|w| w.side != side && !w.harvested).count();
    g.ui.text(16.0, 120.0, &format!("YOUR REMAINS HERE: {mine} TO RECOVER, {theirs} TO HARVEST"), col::GREY);
    let bonus = c.tide.side_bonus(side);
    g.ui.text(16.0, 130.0, &format!("TIDE ADVANTAGE x{bonus:.2}  WEATHER {}", c.tide.weather.name()), col::GREY);
    let lo = &c.active[side_ix(side)];
    let b = build::resolve(lo, &g.content, &[], [false; 7]);
    g.ui.text(
        16.0,
        142.0,
        &format!("BUILD: {}  {} {}  HERESY {}/7", lo.name, b.class.dodge_name(), b.stamina_kind.name(), b.heretic_count),
        col::WHITE,
    );
    let opts: Vec<String> = briefing_options(g, district, side).into_iter().map(|o| o.0).collect();
    g.ui.menu(24.0, h - 20.0 - opts.len() as f32 * 11.0, &opts, sel, &[]);
}

fn draw_foundry(g: &mut Game, a: &Assembly, views: &mut Vec<(View, DrawList)>) {
    let (w, h) = g.low();
    let c = &g.content;
    // Turntable preview with PSX lighting.
    let light = stage_light();
    let mut dl = DrawList::default();
    let mut m = meshgen::Mesh::default();
    meshgen::add_box(
        &mut m,
        Vec3::new(-12.0, -1.0, -12.0),
        Vec3::new(12.0, 0.0, 12.0),
        [90, 90, 96],
        textures::layer(if a.side == Side::Saint { "grate" } else { "hide" }) as u16,
        4.0,
        [false, false, true, false, false, false],
        false,
    );
    isl_render::push_mesh(&mut dl, &m, Mat4::IDENTITY, &light, Vec3::ONE, 255);
    let mut pose = pose_lo(&a.lo, c, Vec3::ZERO, a.rot, g.time, Anim::Idle, 77);
    if a.lo.chassis.is_none() {
        let p = a.lo.paint[0];
        pose.tint = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 200.0 * 0.6 + Vec3::splat(0.4);
    }
    g.preview.draw(&mut dl, c, &pose, &light);
    let size = pose.size.max(0.5);
    let b = a.resolved(c);
    // The turntable lives in the gap between the part list and the stats column.
    let (lw, lh) = g.settings.low_res();
    let vx = 120u32;
    let vwid = lw.saturating_sub(vx + 104).max(64);
    let vh = lh.saturating_sub(80).max(64);
    let eye = Vec3::new(0.0, 7.0 * size, 21.0 * size);
    let aspect = vwid as f32 / vh as f32 * (g.settings.aspect() / (lw as f32 / lh as f32));
    let v = View {
        view_proj: isl_render::projection(0.9, aspect, 90.0) * isl_render::look_at(eye, Vec3::new(0.0, 4.6 * size, 0.0), Vec3::Y),
        cam: eye,
        fog_color: [0.06, 0.07, 0.1],
        fog_near: 40.0,
        fog_far: 90.0,
        flash: 0.0,
        viewport: [vx, 14, vwid, vh],
        draw_static: false,
        clear: true,
    };
    views.push((v, dl));
    // Left: slots and extras.
    let title = if a.side == Side::Saint { "THE FOUNDRY" } else { "THE NEST" };
    g.ui.text(8.0, 6.0, &format!("{title} - {}", a.lo.name), col::GOLD);
    g.ui.panel(2.0, 16.0, 116.0, h - 20.0);
    let mut y = 20.0;
    for (i, s) in Slot::ALL.iter().enumerate() {
        let selected = a.sel == i;
        let label = s.name_for(a.side);
        let part = a.lo.get(*s).map_or("- EMPTY -".to_string(), |p| foundry::part_label(p, c, a.side));
        g.ui.text(6.0, y, label, if selected { col::GOLD } else { col::GREY });
        g.ui.text(10.0, y + 8.0, &part.chars().take(17).collect::<String>(), if selected { col::WHITE } else { [180, 180, 190, 255] });
        y += 17.0;
    }
    for (i, e) in EXTRAS.iter().enumerate() {
        let s = a.sel == 7 + i;
        g.ui.text(6.0, y, &format!("{}{}", if s { icon::CURSOR } else { ' ' }, e), if s { col::GOLD } else { col::WHITE });
        y += 8.0;
    }
    // Right: stats.
    let rx = w - 100.0;
    g.ui.panel(rx - 4.0, 16.0, 102.0, 132.0);
    let stats = [
        format!("HP      {:>6.0}", b.hp_max),
        format!("ARMOR   {:>6.0}", b.armor),
        format!("POSTURE {:>6.0}", b.posture_max),
        format!("WEIGHT {:>3.0}/{:<3.0}", b.weight, b.capacity),
        format!("LOAD    {:>5.0}%{}", b.load * 100.0, if b.overloaded { "!" } else { "" }),
        format!("DODGE   {}", b.class.dodge_name()),
        format!("SPEED   {:>6.1}", b.speed),
        format!("STAMINA {}", b.stamina_kind.name()),
        format!("LOCK    {:>6.0}", b.lock_range),
        format!("HERESY  {}/7{}", b.heretic_count, if b.heretic_form { " FORM" } else { "" }),
        format!("BLOCKS  {}", b.block_cost),
    ];
    for (i, s) in stats.iter().enumerate() {
        g.ui.text(rx, 20.0 + i as f32 * 9.0, s, col::WHITE);
    }
    // Centre-of-mass gizmo.
    let gx = rx + 70.0;
    let gy = 128.0;
    g.ui.outline(gx - 14.0, gy - 10.0, 28.0, 20.0, col::GREY);
    g.ui.rect(gx - 0.5, gy - 10.0, 1.0, 20.0, col::DARK);
    let dx = (b.com.x * 8.0).clamp(-13.0, 13.0);
    let dy = (-b.com.y * 8.0).clamp(-9.0, 9.0);
    g.ui.rect(gx + dx - 1.5, gy + dy - 1.5, 3.0, 3.0, if b.com.x.abs() > 0.4 { col::ORANGE } else { col::GREEN });
    g.ui.text(rx, gy - 9.0, "COM", col::GREY);
    // Moves.
    let mv = |m: &Option<isl_sim::build::MoveRef>| m.as_ref().and_then(|m| c.mv(&m.id)).map_or("-".to_string(), |m| m.name.clone());
    let lines = [
        format!("L {}", mv(&b.moves.light)),
        format!("H {}", mv(&b.moves.heavy)),
        format!("B {}", mv(&b.moves.bash)),
        format!("S {}", mv(&b.moves.special)),
    ];
    for (i, l) in lines.iter().enumerate() {
        g.ui.text(rx - 4.0, 152.0 + i as f32 * 9.0, &l.chars().take(16).collect::<String>(), col::CYAN);
    }
    // Issues / messages.
    let issues = build::validate(&a.lo, &b, c);
    let mut my = h - 12.0;
    if !a.msg.is_empty() {
        g.ui.text_wrap(122.0, my - 18.0, w - 228.0, &a.msg, col::YELLOW);
        my -= 22.0;
    }
    if !issues.is_empty() {
        g.ui.text_wrap(122.0, my - 9.0, w - 228.0, &issues.iter().map(|i| i.text()).collect::<Vec<_>>().join(", "), g.settings.danger());
    }
    let bank = g.campaign.as_ref().map_or(0, |cp| cp.inventory[side_ix(a.side)].currency);
    g.ui.text_right(w - 6.0, 6.0, &format!("{bank} {}", a.side.currency()), col::WHITE);
    // Overlays.
    match a.mode {
        AMode::Pick => {
            let slot = a.slot().unwrap();
            let empty = [vec![], vec![]];
            let inv = g.campaign.as_ref().map(|cp| [cp.inventory[0].parts.clone(), cp.inventory[1].parts.clone()]).unwrap_or(empty);
            let cands = foundry::candidates(&inv, c, slot);
            g.ui.panel(150.0, 24.0, w - 160.0, h - 60.0);
            g.ui.text(156.0, 28.0, &format!("{} - * = HERETIC GRAFT", slot.name_for(a.side)), col::GOLD);
            let start = a.list_sel.saturating_sub(12);
            for (i, cd) in cands.iter().enumerate().skip(start).take(14) {
                let p = &inv[cd.inv][cd.idx];
                let s = i == a.list_sel;
                g.ui.text(
                    158.0,
                    40.0 + (i - start) as f32 * 9.0,
                    &format!("{}{}", if s { icon::CURSOR } else { ' ' }, foundry::part_label(p, c, a.side))
                        .chars()
                        .take(30)
                        .collect::<String>(),
                    if s { col::GOLD } else { col::WHITE },
                );
            }
            if cands.is_empty() {
                let other = if a.side == Side::Saint { "KAIJU" } else { "SAINT" };
                g.ui.text_wrap(
                    158.0,
                    44.0,
                    w - 176.0,
                    &format!("NOTHING HARVESTED FOR THIS SLOT. {} PARTS ONLY DROP WHILE PLAYING THE {} SIDE.", a.side.name(), other),
                    col::GREY,
                );
            } else if let Some(cd) = cands.get(a.list_sel) {
                let p = &inv[cd.inv][cd.idx];
                let q: Vec<String> = p.quirks.iter().filter_map(|q| c.quirk(q)).map(|q| format!("{}: {}", q.name, q.desc)).collect();
                let mut text = format!("{}. {}", p.origin.describe(), q.join(" "));
                // Stat delta preview.
                let mut test = a.lo.clone();
                test.slots[slot.index()] = Some(p.clone());
                let nb = build::resolve(&test, c, &[], [false; 7]);
                text.push_str(&format!(
                    " HP {:+.0} LOAD {:+.0}% SPEED {:+.1}",
                    nb.hp_max - b.hp_max,
                    (nb.load - b.load) * 100.0,
                    nb.speed - b.speed
                ));
                g.ui.text_wrap(158.0, h - 72.0, w - 176.0, &text, col::GREY);
            }
        }
        AMode::Premade => {
            let list: Vec<&ChassisDef> =
                c.chassis.iter().filter(|ch| ch.side == a.side && g.card.system.archive.chassis_available(ch)).collect();
            g.ui.panel(150.0, 24.0, w - 160.0, 100.0);
            let rows: Vec<String> = list.iter().map(|ch| format!("{} - {}", ch.name, ch.playstyle)).collect();
            g.ui.menu(162.0, 32.0, &rows.iter().map(|r| r.chars().take(32).collect()).collect::<Vec<_>>(), a.list_sel, &[]);
            g.ui.text(156.0, 112.0, "HOLD SPECIAL: ALT LOADOUT (RANK 3)", col::GREY);
        }
        AMode::Fabricate => {
            let list: Vec<&PartDef> = c.parts.iter().filter(|d| g.card.system.archive.codex.contains(&d.id)).collect();
            g.ui.panel(150.0, 24.0, w - 160.0, h - 60.0);
            g.ui.text(156.0, 28.0, "CODEX FABRICATION (SCARRED COPY)", col::GOLD);
            let start = a.list_sel.saturating_sub(12);
            for (i, d) in list.iter().enumerate().skip(start).take(14) {
                let s = i == a.list_sel;
                g.ui.text(
                    158.0,
                    40.0 + (i - start) as f32 * 9.0,
                    &format!("{}{} {}", if s { icon::CURSOR } else { ' ' }, d.name, isl_sim::archive::Archive::fabricate_cost(d))
                        .chars()
                        .take(30)
                        .collect::<String>(),
                    if s { col::GOLD } else { col::WHITE },
                );
            }
            if list.is_empty() {
                g.ui.text_wrap(158.0, 44.0, w - 176.0, "SEVER A PART ONCE TO REGISTER IT.", col::GREY);
            }
        }
        AMode::Paint => {
            g.ui.panel(150.0, 24.0, w - 160.0, 50.0);
            let rows = vec![
                format!("PRIMARY   < {:?} >", a.lo.paint[0]),
                format!("SECONDARY < {:?} >", a.lo.paint[1]),
                format!("{} < {} >", if a.side == Side::Saint { "DECAL" } else { "SCARS" }, a.lo.decal),
            ];
            g.ui.menu(162.0, 32.0, &rows, a.list_sel, &[]);
            for (i, pc) in a.lo.paint.iter().enumerate() {
                g.ui.rect(w - 30.0, 32.0 + i as f32 * 11.0, 8.0, 8.0, [pc[0], pc[1], pc[2], 255]);
            }
        }
        AMode::Slots => {}
    }
}

fn draw_card(g: &mut Game, sel: usize, confirm: bool) {
    let (w, h) = g.low();
    g.ui.rect(0.0, 0.0, w, h, [20, 24, 60, 255]);
    g.ui.text_center(w * 0.5, 8.0, "MEMORY CARD (SLOT 1)", col::WHITE);
    // 15 blocks, PS1-style grid.
    let bw = 40.0;
    let bh = 30.0;
    let ox = (w - bw * 5.0 - 16.0) * 0.5;
    let oy = 22.0;
    let mut owner = vec![None; 15];
    let mut k = 0;
    for (i, e) in g.card.entries.iter().enumerate() {
        for _ in 0..e.blocks {
            if k < 15 {
                owner[k] = Some(i);
                k += 1;
            }
        }
    }
    for b in 0..15 {
        let x = ox + (b % 5) as f32 * (bw + 4.0);
        let y = oy + (b / 5) as f32 * (bh + 4.0);
        let (fill, label) = match owner[b] {
            Some(i) => {
                let e = &g.card.entries[i];
                let c = match e.data {
                    BlockData::Campaign(_) => [60, 90, 160, 255],
                    BlockData::Build { .. } => {
                        if e.icon == 1 {
                            [90, 110, 140, 255]
                        } else {
                            [60, 120, 100, 255]
                        }
                    }
                    BlockData::Corrupted { .. } => [120, 40, 120, 255],
                    BlockData::Run(_) => [120, 100, 40, 255],
                };
                (c, e.kind_name())
            }
            None => ([30, 30, 50, 255], "FREE"),
        };
        g.ui.rect(x, y, bw, bh, fill);
        let s = owner[b] == Some(sel);
        g.ui.outline(x, y, bw, bh, if s { col::GOLD } else { col::BORDER });
        g.ui.text(x + 3.0, y + 11.0, label, col::WHITE);
    }
    let ly = oy + 3.0 * (bh + 4.0) + 6.0;
    let mut rows: Vec<String> = g
        .card
        .entries
        .iter()
        .map(|e| format!("{:<7} {:<20} {}BLK", e.kind_name(), e.title.chars().take(20).collect::<String>(), e.blocks))
        .collect();
    rows.push("IMPORT CODE...".into());
    g.ui.menu(24.0, ly, &rows, sel, &[]);
    let hint = if confirm { "CONFIRM AGAIN TO DELETE" } else { "CONFIRM: LOAD BUILD  HEAVY: DELETE  SPECIAL: EXPORT CODE" };
    g.ui.text(8.0, h - 12.0, hint, if confirm { g.settings.danger() } else { col::GREY });
    g.ui.text_right(w - 8.0, 8.0, &format!("{} FREE", g.card.free()), col::GREY);
}

fn draw_cutscene(g: &mut Game, f: &Fmv, views: &mut Vec<(View, DrawList)>, post: &mut PostSettings) {
    let (w, h) = g.low();
    let Some((shot, k)) = f.current() else { return };
    post.fmv = 0.85;
    let light = stage_light();
    let mut dl = DrawList::default();
    let c = &g.content;
    stage(&mut dl, &light, g.time, shot.scene != Scene::Void);
    let saint = Loadout::premade(c.chassis_def("iron_saint").unwrap(), c, 1);
    let kaiju = Loadout::premade(c.chassis_def("leviathan").unwrap(), c, 1);
    let t = g.time;
    let chimera = if shot.scene == Scene::Chimera {
        // The final form: one part from every corner of the codex.
        let mut lo = saint.clone();
        let codex: Vec<&String> = g.card.system.archive.codex.iter().collect();
        for (i, s) in Slot::ALL.iter().enumerate() {
            if let Some(d) = codex.iter().filter_map(|id| c.part(id)).filter(|d| d.slot == *s).nth(i % 3) {
                lo.slots[i] = Some(isl_sim::parts::PartInstance::premade(d, 900 + i as u64, "chimera"));
            }
        }
        Some(lo)
    } else {
        None
    };
    match shot.scene {
        Scene::Saint | Scene::Both => {
            let p = pose_lo(&saint, c, Vec3::new(-6.0, 0.0, 0.0), 0.6, t, Anim::Walk(t), 1);
            g.preview.draw(&mut dl, c, &p, &light);
        }
        _ => {}
    }
    match shot.scene {
        Scene::Kaiju | Scene::Both => {
            let p = pose_lo(&kaiju, c, Vec3::new(8.0, 0.0, -4.0), -0.8 + t * 0.2, t, Anim::Walk(t * 2.0), 2);
            g.preview.draw(&mut dl, c, &p, &light);
        }
        _ => {}
    }
    if let Some(lo) = &chimera {
        let p = pose_lo(lo, c, Vec3::ZERO, t * 0.3, t, Anim::Idle, 3);
        g.preview.draw(&mut dl, c, &p, &light);
    }
    let a = t * 0.15;
    let dist = 34.0 - k * 10.0;
    let v = view_for(g, Vec3::new(a.sin() * dist, 8.0 + k * 6.0, a.cos() * dist), Vec3::new(0.0, 6.0, 0.0), [0.04, 0.05, 0.08], 100.0);
    views.push((v, dl));
    g.ui.rect(0.0, 0.0, w, 18.0, col::BLACK);
    g.ui.rect(0.0, h - 30.0, w, 30.0, col::BLACK);
    for (i, l) in shot.lines.iter().enumerate() {
        g.ui.text_center(w * 0.5, h - 24.0 + i as f32 * 10.0, l, col::WHITE);
    }
    if f.id == "disc3" {
        g.ui.glitch = 0.6;
        g.ui.text_center(w * 0.5, 4.0, "DISC 3", [230, 90, 220, 255]);
        g.ui.glitch = 0.0;
    }
}

fn draw_mission(g: &mut Game, views: &mut Vec<(View, DrawList)>, post: &mut PostSettings) {
    let (lw, lh) = g.settings.low_res();
    let Some(mut m) = g.mission.take() else { return };
    let versus = m.kind == MissionKind::Versus;
    let vps: Vec<[u32; 4]> = if versus { vec![[0, 0, lw / 2, lh], [lw / 2, 0, lw - lw / 2, lh]] } else { vec![[0, 0, lw, lh]] };
    for (pi, vp) in vps.iter().enumerate() {
        let view = m.view(pi, *vp, &g.settings);
        let mut dl = DrawList::default();
        m.draw(&g.content, &mut dl, &view, &g.settings);
        views.push((view, dl));
    }
    if m.photo {
        post.fmv = if g.photo_filter == 3 { 0.7 } else { 0.0 };
        post.crt = g.photo_filter == 1;
        post.composite = g.photo_filter == 2;
        if g.magazine {
            draw_magazine(&mut g.ui, lw as f32, lh as f32, g.time);
        } else {
            g.ui.text(4.0, 4.0, "PHOTO MODE: CONFIRM SNAP  TAB FILTER  Q COVER  BACK EXIT", [255, 255, 255, 160]);
        }
    } else {
        for (pi, vp) in vps.iter().enumerate() {
            let view = m.view(pi, *vp, &g.settings);
            hud::draw(&mut g.ui, &m, &g.content, &g.settings, g.using_pad[pi], &view, pi as u8);
        }
    }
    if versus {
        g.ui.rect(lw as f32 / 2.0 - 1.0, 0.0, 2.0, lh as f32, col::BLACK);
    }
    if let Some(sel) = g.pause {
        let (w, h) = (lw as f32, lh as f32);
        g.ui.rect(0.0, 0.0, w, h, [0, 0, 20, 140]);
        g.ui.panel(w * 0.5 - 60.0, h * 0.3, 120.0, 70.0);
        let items: Vec<String> = ["RESUME", "PHOTO MODE", "SETTINGS", "RETREAT", "QUIT GAME"].iter().map(|s| s.to_string()).collect();
        g.ui.menu(w * 0.5 - 44.0, h * 0.3 + 8.0, &items, sel, &[]);
        if let MissionKind::War { .. } = m.kind {
            g.ui.text_center(w * 0.5, h * 0.3 + 76.0, "RETREAT KEEPS LOOT; NO TERRITORY", col::GREY);
        }
    }
    g.mission = Some(m);
}

fn draw_magazine(ui: &mut Ui, w: f32, h: f32, t: f32) {
    // A fake 1998 games-magazine cover (an original masthead, not a real publication).
    ui.rect(0.0, 0.0, w, 34.0, [200, 20, 30, 230]);
    ui.text_big(8.0, 4.0, "HARROWMOUTH", col::WHITE, 2.0);
    ui.text_big(8.0, 20.0, "MONTHLY", col::YELLOW, 1.0);
    ui.text_right(w - 6.0, 6.0, "ISSUE 07", col::WHITE);
    ui.text_right(w - 6.0, 16.0, "JAN 1998", col::WHITE);
    ui.rect(w - 70.0, h * 0.4, 64.0, 34.0, [250, 230, 40, 240]);
    ui.text(w - 66.0, h * 0.4 + 4.0, "EXCLUSIVE!", [200, 20, 30, 255]);
    ui.text(w - 66.0, h * 0.4 + 14.0, "200+ PARTS", col::BLACK);
    ui.text(w - 66.0, h * 0.4 + 23.0, "REVEALED", col::BLACK);
    ui.text_big(8.0, h - 38.0, "SAINT VS SEA", col::WHITE, 2.0);
    ui.text(8.0, h - 18.0, "THE TIDE BETWEEN: WE PLAYED IT", col::YELLOW);
    ui.text_right(w - 6.0, h - 10.0, &format!("{:04}", (t * 10.0) as u32 % 10000), col::GREY);
}
