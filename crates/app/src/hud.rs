//! In-mission HUD. Chunky PSX bars, lock-on part readout, boss bar, prompts and the Sync glitch.

use crate::mission::{Mission, MissionKind};
use crate::settings::Settings;
use glam::Vec2;
use isl_data::*;
use isl_input::{Action, Binding, Bindings};
use isl_render::View;
use isl_sim::fighter::{Act, Fighter, FighterKind};
use isl_ui::{Ui, col, icon};

fn stamina_label(f: &Fighter) -> (&'static str, [u8; 4], f32) {
    let s = &f.stamina;
    match s.kind {
        // Heat shows how hot you are (fills up), the others show what's left.
        StaminaKind::Heat => ("HEAT", if s.overheated { col::RED } else { col::ORANGE }, 1.0 - s.frac()),
        StaminaKind::Battery => ("BATT", col::CYAN, s.frac()),
        StaminaKind::Fuel => ("FUEL", col::YELLOW, s.frac()),
        StaminaKind::Hunger => ("HUNGER", [200, 160, 90, 255], s.frac()),
        StaminaKind::Blood => ("BLOOD", col::BLOOD, s.frac()),
        StaminaKind::Breath => ("BREATH", if s.exhaling() { col::WHITE } else { [140, 200, 230, 255] }, s.frac()),
    }
}

/// Draw the HUD for player `p` inside `vp` (x, y, w, h).
/// Label of the first binding for an action on the active device class.
pub fn prompt(b: &Bindings, a: Action, pad: bool) -> String {
    b.map.get(&a).and_then(|v| v.iter().find(|x| matches!(x, Binding::Pad(_) | Binding::Axis(..)) == pad)).map_or("-".into(), |x| x.label())
}

pub fn draw(ui: &mut Ui, m: &Mission, content: &Content, settings: &Settings, using_pad: bool, view: &View, p: u8) {
    let Some(f) = m.world.player(p) else { return };
    let [vx, vy, vw, vh] = view.viewport.map(|x| x as f32);
    if f.build.fx.no_hud {
        // Black Signal omen: only death and boss lines remain.
        draw_center_banners(ui, m, vx, vy, vw, vh, f);
        return;
    }
    // Low sync / heresy glitches the HUD.
    ui.glitch = if f.loadout.side == Side::Saint { ((1.0 - f.build.sync) * 1.4).min(1.0) } else { 0.0 };
    let x = vx + 8.0;
    let y = vy + 8.0;
    let w = (vw * 0.42).min(140.0);
    ui.bar(x, y, w, 5.0, f.hp_frac(), [200, 40, 40, 255], [40, 10, 10, 255]);
    let (lab, c, frac) = stamina_label(f);
    ui.bar(x, y + 8.0, w * 0.85, 4.0, frac, c, [20, 20, 30, 255]);
    ui.text(x + w * 0.85 + 4.0, y + 6.0, lab, c);
    if f.stamina.kind == StaminaKind::Fuel {
        ui.bar(x, y + 14.0, w * 0.5, 2.0, f.stamina.reserve / f.stamina.reserve_max.max(1.0), [140, 120, 40, 255], [20, 20, 20, 255]);
    }
    if f.stamina.kind == StaminaKind::Hunger {
        ui.bar(x, y + 14.0, w * 0.5, 2.0, f.stamina.food, [200, 140, 60, 255], [20, 20, 20, 255]);
    }
    // Posture (centre, only when building up).
    if f.posture > 1.0 {
        let pw = (vw * 0.3).min(90.0);
        ui.bar(vx + vw * 0.5 - pw * 0.5, vy + vh - 22.0, pw, 3.0, f.posture_frac(), col::YELLOW, [30, 30, 10, 255]);
    }
    // Heals and currency.
    let heal_icon = match f.build.heal {
        HealKind::RepairCharges(_) => icon::BLOCK,
        HealKind::Regrowth => icon::HEART,
    };
    let mut hx = x;
    for i in 0..f.heal_max {
        let c = if i < f.heal_charges { col::GREEN } else { col::DARK };
        ui.text(hx, y + 19.0, &heal_icon.to_string(), c);
        hx += 7.0;
    }
    let cur = format!("{} {}", f.currency, f.loadout.side.currency());
    ui.text_right(vx + vw - 6.0, vy + vh - 12.0, &cur, col::WHITE);
    // Heresy readout.
    if f.build.heretic_count > 0 {
        let s = if f.loadout.side == Side::Saint {
            format!("SYNC {:>3}%", (f.build.sync * 100.0) as u32)
        } else {
            format!("MACHINE-BLOOD {:>3}%", (f.build.machine_blood * 100.0) as u32)
        };
        ui.text(x, y + 28.0, &s, if f.build.heretic_form { col::MAGENTA } else { col::GREY });
        if f.build.heretic_form {
            let k = prompt(&settings.bindings, Action::Heretic, using_pad);
            ui.text(x, y + 37.0, &format!("[{k}] HERETIC FORM"), col::MAGENTA);
        }
    }
    // Tide clock and weather.
    if p == 0 && m.kind != MissionKind::Versus {
        let tc = &m.world.cfg.tide;
        let s = format!("{:02}:00 TIDE {:+.1} {}", tc.hour_of_day(), tc.tide(), tc.weather.name());
        ui.text_right(vx + vw - 6.0, vy + 6.0, &s, col::GREY);
    }
    // Lock-on reticle and target part readout.
    if let Some(l) = f.lock
        && let Some(t) = m.world.fighters.get(l.target as usize)
    {
        let height = t.height()
            * if l.slot == Slot::Head {
                0.9
            } else if l.slot == Slot::Legs {
                0.2
            } else {
                0.55
            };
        let (tp, _) = m.interp(t.id as usize, m.alpha());
        let wp = tp + glam::Vec3::Y * (height + t.altitude);
        if let Some(s) = Mission::project(view, wp) {
            let pulse = if matches!(t.act, Act::PostureBroken) { settings.danger() } else { col::WHITE };
            // Diamond reticle (shape, not just colour).
            ui.text(s.x - 2.5, s.y - 10.0, &icon::DOWN.to_string(), pulse);
            ui.text(s.x - 2.5, s.y + 4.0, &icon::UP.to_string(), pulse);
            ui.text(s.x - 10.0, s.y - 3.0, &icon::RIGHT.to_string(), pulse);
            ui.text(s.x + 5.0, s.y - 3.0, &icon::LEFT.to_string(), pulse);
        }
        let px = vx + vw - (vw * 0.4).min(120.0) - 6.0;
        let py = vy + 18.0;
        ui.text(px, py, &t.name, col::WHITE);
        let show_hp = f.build.reveal || t.kind.is_bossish() || true;
        let mut ly = py + 10.0;
        for s in Slot::ALL {
            if t.loadout.get(s).is_none() {
                continue;
            }
            let maxhp = t.build.parts[s.index()].as_ref().map(|p| p.hp);
            let frac = if t.destroyed[s.index()] { 0.0 } else { maxhp.map_or(0.0, |mx| t.part_hp[s.index()] / mx) };
            let sel = s == l.slot;
            let c = if t.destroyed[s.index()] {
                col::DARK
            } else if sel {
                col::GOLD
            } else {
                col::GREY
            };
            let short = s.short_for(t.loadout.side);
            ui.text(px, ly, &format!("{}{}", if sel { icon::CURSOR } else { ' ' }, short), c);
            if show_hp && !t.destroyed[s.index()] {
                ui.bar(px + 62.0, ly + 2.0, 40.0, 3.0, frac, if sel { col::GOLD } else { [150, 150, 160, 255] }, [20, 20, 20, 255]);
            }
            ly += 8.0;
        }
        if matches!(t.act, Act::PostureBroken) {
            let k = prompt(&settings.bindings, Action::Heavy, using_pad);
            ui.text_center(vx + vw * 0.5, vy + vh * 0.62, &format!("[{k}] SEVER"), settings.danger());
        }
    }
    // Boss bar.
    if m.kind != MissionKind::Versus
        && let Some(b) = m.world.fighters.iter().find(|b| {
            b.alive
                && !matches!(b.act, Act::Dead)
                && b.kind.is_bossish()
                && b.ai.as_ref().is_some_and(|a| a.awake)
                && b.team.hostile(f.team)
                && b.pos.distance(f.pos) < 160.0
        })
    {
        let bw = vw * 0.7;
        let bx = vx + (vw - bw) * 0.5;
        let by = vy + vh - 34.0;
        ui.text(bx, by - 9.0, &b.name, col::WHITE);
        ui.bar(bx, by, bw, 4.0, b.hp_frac(), [200, 40, 40, 255], [40, 10, 10, 255]);
        ui.bar(bx + bw * 0.3, by + 6.0, bw * 0.4, 2.0, b.posture_frac(), col::YELLOW, [30, 30, 10, 255]);
    }
    // Versus: opponent HP on top.
    if m.kind == MissionKind::Versus
        && let Some(o) = m.world.player(1 - p)
    {
        ui.text(vx + vw - 70.0, vy + vh - 22.0, &o.name.chars().take(10).collect::<String>(), col::GREY);
    }
    // Context prompts.
    let pad = using_pad;
    let mut ptxt = None;
    if let Some(cp) = m.world.city.checkpoints.iter().find(|c| c.kind.side() == f.loadout.side && c.pos.distance(f.pos) < 14.0) {
        ptxt = Some(format!("[{}] REST AT {}", prompt(&settings.bindings, Action::Interact, pad), cp.name));
    } else if let Some(w) = m.world.wrecks.iter().find(|w| w.district == m.world.cfg.district && w.pos.distance(f.pos) < 12.0) {
        if w.side == f.loadout.side && w.currency > 0 {
            ptxt = Some(format!("[{}] RECOVER {} {}", prompt(&settings.bindings, Action::Interact, pad), w.currency, w.side.currency()));
        } else if w.side != f.loadout.side && !w.harvested {
            ptxt = Some(format!(
                "[{}] HARVEST YOUR OLD {}",
                prompt(&settings.bindings, Action::Interact, pad),
                if w.side == Side::Saint { "WRECK" } else { "CARCASS" }
            ));
        }
    } else if matches!(f.act, Act::Climbing { .. }) {
        ptxt = Some(format!(
            "[{}] RIP   [{}] DROP",
            prompt(&settings.bindings, Action::Heavy, pad),
            prompt(&settings.bindings, Action::Dodge, pad)
        ));
    } else if m.world.fighters.iter().any(|b| {
        matches!(&b.kind, FighterKind::Boss(id) if content.boss(id).is_some_and(|bd| bd.huge))
            && matches!(b.act, Act::PostureBroken | Act::Stagger { .. })
            && b.pos.distance(f.pos) < b.radius() + f.radius() + 10.0
    }) {
        ptxt = Some(format!("[{}] CLIMB", prompt(&settings.bindings, Action::Interact, pad)));
    }
    if let Some(s) = ptxt {
        ui.text_center(vx + vw * 0.5, vy + vh * 0.72, &s, col::WHITE);
    }
    // Messages.
    let mut my = vy + vh * 0.42;
    for msg in m.msgs.iter().rev().take(4) {
        let mut c = msg.color;
        c[3] = (msg.ttl.min(1.0) * 255.0) as u8;
        ui.text(x, my, &msg.text, c);
        my -= 9.0;
    }
    ui.glitch = 0.0;
    draw_center_banners(ui, m, vx, vy, vw, vh, f);
}

fn draw_center_banners(ui: &mut Ui, m: &Mission, vx: f32, vy: f32, vw: f32, vh: f32, f: &Fighter) {
    let cx = vx + vw * 0.5;
    if let Some((s, _)) = &m.subtitle {
        ui.text_wrap(vx + 16.0, vy + vh - 58.0, vw - 32.0, s, col::WHITE);
    }
    if let Some(i) = &m.intro {
        let a = (i.ttl.min(1.0) * 255.0) as u8;
        ui.rect(vx, vy + vh * 0.3, vw, 36.0, [0, 0, 0, a / 2]);
        ui.text_center_big(cx, vy + vh * 0.3 + 4.0, &i.title, [255, 255, 255, a], 2.0);
        ui.text_center(cx, vy + vh * 0.3 + 22.0, &i.sub, [200, 200, 200, a]);
        if !i.card.is_empty() {
            ui.text_wrap(vx + 16.0, vy + vh * 0.3 + 40.0, vw - 32.0, &i.card, [220, 220, 220, a]);
        }
    }
    if let Some(t) = m.dead_timer {
        let a = (t.min(1.0) * 220.0) as u8;
        ui.rect(vx, vy, vw, vh, [0, 0, 0, a / 2]);
        let s = if f.loadout.side == Side::Saint { "SIGNAL LOST" } else { "THE SEA TAKES YOU BACK" };
        ui.text_center_big(cx, vy + vh * 0.45, s, [200, 30, 30, a], 2.0);
    }
    if m.world.victory && m.kind != MissionKind::Versus {
        let a = (m.end_timer.min(1.0) * 255.0) as u8;
        let s = match &m.kind {
            MissionKind::War { side: Side::Saint, .. } => "DISTRICT HELD",
            MissionKind::War { side: Side::Kaiju, .. } => "DISTRICT DROWNED",
            _ => "VICTORY",
        };
        ui.text_center_big(cx, vy + vh * 0.42, s, [255, 210, 110, a], 2.0);
    }
    let _ = Vec2::ZERO;
}
