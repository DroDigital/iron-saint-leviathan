//! Headless tools: deterministic screenshot scenarios (CI artifacts, regression checks) and the
//! soak test (bot plays every mode for N simulated minutes; must not panic).

use crate::game::{Game, Launch, Screen};
use glam::Vec2;
use isl_audio::{Audio, AudioSettings};
use isl_data::Side;
use isl_input::{Action, Input, PlayerInput};
use isl_render::{DrawList, PostSettings, Renderer, View};
use std::collections::HashSet;
use std::path::Path;

fn press(actions: &[Action], mv: Vec2, cam: Vec2) -> PlayerInput {
    let mut p = PlayerInput::default();
    for a in actions {
        p.pressed.insert(*a);
        p.held.insert(*a);
    }
    p.move_axis = mv;
    p.cam_axis = cam;
    p
}

fn step(g: &mut Game, input: &mut Input, audio: &mut Audio, p: PlayerInput, frames: usize) {
    for i in 0..frames {
        input.players[0] = if i == 0 { p.clone() } else { PlayerInput { pressed: HashSet::new(), ..p.clone() } };
        input.players[1] = PlayerInput::default();
        g.update(1.0 / 60.0, input, audio);
    }
}

fn render(g: &mut Game, r: &mut Option<Renderer>) -> Option<Vec<u8>> {
    let r = r.as_mut()?;
    let (w, h) = g.settings.low_res();
    r.set_low_res(w, h);
    if let Some(st) = g.take_static() {
        r.upload_static(&st);
    }
    let mut views: Vec<(View, DrawList)> = vec![];
    let mut post = PostSettings::default();
    g.draw(&mut views, &mut post);
    let refs: Vec<(View, &DrawList)> = views.iter().map(|(v, d)| (*v, d)).collect();
    let img = r.render_to_image(&refs, &g.ui.verts, &post);
    g.last_stats = r.stats;
    Some(img)
}

fn save(img: &[u8], w: u32, h: u32, path: &Path) {
    let k = 2;
    let mut big = vec![0u8; (w * k * h * k * 4) as usize];
    for y in 0..h * k {
        for x in 0..w * k {
            let s = (((y / k) * w + x / k) * 4) as usize;
            let d = ((y * w * k + x) * 4) as usize;
            big[d..d + 4].copy_from_slice(&img[s..s + 4]);
            big[d + 3] = 255;
        }
    }
    image::save_buffer(path, &big, w * k, h * k, image::ColorType::Rgba8).expect("png");
}

fn setup() -> (Game, Input, Audio, Option<Renderer>) {
    // SAFETY: single-threaded tool startup.
    unsafe { std::env::set_var("ISL_NO_AUDIO", "1") };
    let g = Game::new(true);
    let input = Input::new(g.settings.bindings.clone());
    let audio = Audio::new(AudioSettings::default());
    let r = match Renderer::new_headless(g.settings.low_res()) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("no GPU adapter ({e}); running without rendering");
            None
        }
    };
    (g, input, audio, r)
}

pub fn screenshots(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let (mut g, mut input, mut audio, mut r) = setup();
    let (w, h) = g.settings.low_res();
    let shot = |g: &mut Game, name: &str, r: &mut Option<Renderer>| {
        if let Some(img) = render(g, r) {
            save(&img, w, h, &dir.join(format!("{name}.png")));
            println!("wrote {name}.png ({} draw calls, {} tris)", g.last_stats.draw_calls, g.last_stats.tris);
        }
    };
    g.screen = Screen::Title(1.0);
    step(&mut g, &mut input, &mut audio, PlayerInput::default(), 30);
    shot(&mut g, "01_title", &mut r);
    g.new_campaign(None, 0, false);
    g.screen = Screen::WarMap { sel: 0, side: Side::Saint };
    step(&mut g, &mut input, &mut audio, PlayerInput::default(), 5);
    shot(&mut g, "02_war_map", &mut r);
    // Saint sortie in the Harbor.
    g.build_mission(&Launch::War { district: "harbor".into(), side: Side::Saint, finale: None }, &mut audio);
    step(&mut g, &mut input, &mut audio, press(&[], Vec2::new(0.0, 1.0), Vec2::ZERO), 150);
    step(&mut g, &mut input, &mut audio, press(&[Action::LockOn], Vec2::ZERO, Vec2::new(0.4, 0.0)), 30);
    shot(&mut g, "03_harbor_saint", &mut r);
    // Teleport next to the boss to show a fight.
    if let Some(m) = g.mission.as_mut() {
        let (c, _) = m.world.city.arena;
        let pid = m.world.player_id(0).unwrap() as usize;
        m.world.fighters[pid].pos = c + glam::Vec3::new(0.0, 0.0, -26.0);
        m.world.fighters[pid].lock = None;
    }
    step(&mut g, &mut input, &mut audio, press(&[Action::LockOn], Vec2::new(0.0, 0.3), Vec2::ZERO), 120);
    step(&mut g, &mut input, &mut audio, press(&[Action::Light], Vec2::ZERO, Vec2::ZERO), 20);
    if std::env::var("ISL_DEBUG").is_ok() {
        if let Some(m) = g.mission.as_ref() {
            for f in &m.world.fighters {
                if f.alive && (f.kind.is_bossish() || f.kind.is_player()) {
                    eprintln!(
                        "{:?} pos {:?} act {} alt {} burrowed {} lock {:?} cam {:?}",
                        f.kind,
                        f.pos,
                        f.act.name(),
                        f.altitude,
                        f.burrowed,
                        f.lock,
                        m.cams[0].pos
                    );
                }
            }
        }
    }
    shot(&mut g, "04_boss_leviathan", &mut r);
    // Kaiju sortie.
    g.build_mission(&Launch::War { district: "foundry_quarter".into(), side: Side::Kaiju, finale: None }, &mut audio);
    step(&mut g, &mut input, &mut audio, press(&[], Vec2::new(0.2, 1.0), Vec2::new(0.2, 0.0)), 200);
    shot(&mut g, "05_foundry_kaiju", &mut r);
    // Foundry screen.
    g.mission = None;
    let lo = g.active_or_premade(Side::Saint);
    g.screen = Screen::Foundry(Box::new(crate::foundry::Assembly::new(Side::Saint, lo)));
    step(&mut g, &mut input, &mut audio, PlayerInput::default(), 20);
    shot(&mut g, "06_foundry_screen", &mut r);
    let lo = g.active_or_premade(Side::Kaiju);
    g.screen = Screen::Foundry(Box::new(crate::foundry::Assembly::new(Side::Kaiju, lo)));
    step(&mut g, &mut input, &mut audio, PlayerInput::default(), 20);
    shot(&mut g, "07_nest_screen", &mut r);
    g.screen = Screen::Card { sel: 0, confirm: false };
    step(&mut g, &mut input, &mut audio, PlayerInput::default(), 2);
    shot(&mut g, "08_memory_card", &mut r);
    g.screen = Screen::Cutscene(Box::new(crate::fmv::Fmv::intro(g.content.lore.intro.clone(), Box::new(Screen::Main { sel: 0 }))));
    step(&mut g, &mut input, &mut audio, PlayerInput::default(), 90);
    shot(&mut g, "09_fake_fmv", &mut r);
    // Versus split-screen.
    let a = g.active_or_premade(Side::Saint);
    let b = g.active_or_premade(Side::Kaiju);
    g.build_mission(&Launch::Versus { a: Box::new(a), b: Box::new(b) }, &mut audio);
    step(&mut g, &mut input, &mut audio, press(&[], Vec2::new(0.0, 1.0), Vec2::ZERO), 90);
    shot(&mut g, "10_link_cable_versus", &mut r);
    g.screen = Screen::DiscSwap { t: 0.5, to: Side::Kaiju, next: Box::new(Launch::RunFight) };
    shot(&mut g, "11_disc_swap", &mut r);
    Ok(())
}

/// Bot plays through screens and missions for `secs` simulated seconds.
pub fn soak(secs: f32, render_every: usize) -> anyhow::Result<()> {
    let (mut g, mut input, mut audio, mut r) = setup();
    let mut rng = 12345u32;
    let mut rnd = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng
    };
    g.new_campaign(None, 0, false);
    let frames = (secs * 60.0) as usize;
    let districts: Vec<String> = g.content.districts.iter().filter(|d| !d.finale).map(|d| d.id.clone()).collect();
    let mut missions = 0;
    for f in 0..frames {
        if !matches!(g.screen, Screen::Mission | Screen::Loading { .. } | Screen::DiscSwap { .. })
            || g.mission.is_none() && matches!(g.screen, Screen::Mission)
        {
            // Start another sortie on a rotating district and side.
            let side = if missions % 2 == 0 { Side::Saint } else { Side::Kaiju };
            let d = districts[missions % districts.len()].clone();
            missions += 1;
            if missions % 5 == 0 {
                let a = g.active_or_premade(Side::Saint);
                g.build_mission(&Launch::Arena { loadout: Box::new(a), back_to_foundry: false }, &mut audio);
            } else {
                g.build_mission(&Launch::War { district: d, side, finale: None }, &mut audio);
            }
        }
        // Bot: walk toward enemies, lock, mash, dodge, heal.
        let mut acts = vec![];
        let k = rnd();
        if k % 9 == 0 {
            acts.push(Action::Light);
        }
        if k % 23 == 0 {
            acts.push(Action::Heavy);
        }
        if k % 31 == 0 {
            acts.push(Action::Dodge);
        }
        if k % 97 == 0 {
            acts.push(Action::LockOn);
        }
        if k % 151 == 0 {
            acts.push(Action::Heal);
        }
        if k % 211 == 0 {
            acts.push(Action::Special);
        }
        if k % 401 == 0 {
            acts.push(Action::Interact);
        }
        let mv = Vec2::new(((f as f32) * 0.01).sin(), 1.0);
        let mut p = press(&acts, mv, Vec2::new(0.3, 0.0));
        p.held.insert(Action::Guard);
        if k % 3 != 0 {
            p.held.remove(&Action::Guard);
        }
        input.players[0] = p;
        g.update(1.0 / 60.0, &mut input, &mut audio);
        // Retreat after ~90 s to rotate content.
        if let Some(m) = g.mission.as_ref() {
            if m.world.tick > 60 * 90 {
                g.pause = Some(3);
                input.players[0] = press(&[Action::Confirm], Vec2::ZERO, Vec2::ZERO);
                g.update(1.0 / 60.0, &mut input, &mut audio);
            }
        }
        if matches!(g.screen, Screen::Results { .. }) {
            input.players[0] = press(&[Action::Confirm], Vec2::ZERO, Vec2::ZERO);
            g.update(1.0 / 60.0, &mut input, &mut audio);
        }
        if render_every > 0 && f % render_every == 0 {
            render(&mut g, &mut r);
        }
        if f % 3600 == 0 {
            println!(
                "soak: {}s, missions {missions}, screen ok, fighters {}",
                f / 60,
                g.mission.as_ref().map_or(0, |m| m.world.fighters.len())
            );
        }
    }
    println!("soak complete: {missions} missions, {} frames", frames);
    Ok(())
}
