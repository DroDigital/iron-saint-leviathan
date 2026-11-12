//! Platform-independent game: screens, flow between them, campaign bookkeeping and saving.
//! The windowed app and the headless tools (screenshots, soak test) drive the same `Game`.

use crate::fmv::Fmv;
use crate::foundry::Assembly;
use crate::mission::{End, Mission, MissionKind};
use crate::settings::Settings;
use glam::Vec3;
use isl_audio::{Audio, Sfx, music::Layer};
use isl_data::*;
use isl_ghost::HabitProfile;
use isl_input::{Action, Input, PlayerInput};
use isl_render::{DrawList, PostSettings, Vertex, View};
use isl_save::{BlockData, CorruptItem, Entry, MemoryCard, Paths};
use isl_sim::archive::daily_seed;
use isl_sim::build::{self, Loadout};
use isl_sim::fighter::{Act, FighterKind, SimEvent};
use isl_sim::parts::{Condition, Origin, PartInstance};
use isl_sim::run::TideRun;
use isl_sim::war::{Campaign, side_ix};
use isl_sim::world::*;
use isl_ui::Ui;
use std::path::PathBuf;

pub enum Request {
    Quit,
    ApplyVideo,
    SavePhoto,
}

#[derive(Clone, Debug)]
pub enum Launch {
    War { district: String, side: Side, finale: Option<String> },
    RunFight,
    Gauntlet { side: Side, queue: Vec<NemesisSpec>, index: usize },
    Arena { loadout: Box<Loadout> },
    Versus { a: Box<Loadout>, b: Box<Loadout> },
}

pub enum Screen {
    Boot(f32),
    Title(f32),
    Main { sel: usize },
    NewWar { sel: usize, condition: usize, rot: u8, use_corrupt: bool },
    WarMap { sel: usize, side: Side },
    Briefing { district: String, side: Side, sel: usize },
    DiscSwap { t: f32, to: Side, next: Box<Launch> },
    Loading { t: f32, lore: usize, next: Box<Launch> },
    Mission,
    Results { title: String, lines: Vec<String>, next: Box<Screen> },
    Foundry(Box<Assembly>),
    Card { sel: usize, confirm: bool },
    Archive { tab: usize, sel: usize },
    Settings { tab: usize, sel: usize },
    Cutscene(Box<Fmv>),
    RunSetup { sel: usize, omens: Vec<bool>, daily: bool },
    RunDraft { sel: usize },
    Gauntlet { sel: usize, side: Side },
    Versus { sel: usize, a: usize, b: usize },
    Codes { text: String, msg: String },
    Corrupt { sel: usize, ending: String },
}

pub struct Game {
    pub content: Content,
    pub data_dir: Option<PathBuf>,
    data_stamp: u64,
    reload_t: f32,
    pub settings: Settings,
    pub paths: Paths,
    pub card: MemoryCard,
    pub campaign: Option<Campaign>,
    pub screen: Screen,
    pub mission: Option<Mission>,
    pub run: Option<TideRun>,
    pub gauntlet: Option<(Side, Vec<NemesisSpec>, usize)>,
    pub ui: Ui,
    pub time: f32,
    pub requests: Vec<Request>,
    pub pending_static: Option<Vec<Vertex>>,
    pub toast: Option<(String, f32)>,
    pub preview: crate::visuals::Visuals,
    pub pause: Option<usize>,
    pub magazine: bool,
    pub photo_filter: usize,
    pub typed: String,
    pub headless: bool,
    pub frame: u64,
    pub dev_overlay: bool,
    pub last_stats: isl_render::FrameStats,
    /// Per player: is a gamepad the active device (for button prompts)?
    pub using_pad: [bool; 2],
}

pub fn read_content() -> (Content, Option<PathBuf>) {
    // Release: game.pak next to the executable. Dev: loose data/ (hot reloadable).
    if let Ok(exe) = std::env::current_exe() {
        for cand in [exe.with_file_name("game.pak"), exe.parent().map(|p| p.join("../Resources/game.pak")).unwrap_or_default()] {
            if let Ok(bytes) = std::fs::read(&cand)
                && let Ok(files) = isl_assets::pak::read(&bytes)
            {
                let set: FileSet = files.into_iter().map(|(k, v)| (k, String::from_utf8_lossy(&v).to_string())).collect();
                match Content::from_files(&set) {
                    Ok(c) => return (c, None),
                    Err(e) => tracing::error!("game.pak invalid: {e}"),
                }
            }
        }
    }
    let dir = find_data_dir().expect("data/ directory or game.pak not found");
    match Content::load_dir(&dir) {
        Ok(c) => (c, Some(dir)),
        Err(e) => panic!("content failed to load: {e}"),
    }
}

fn dir_stamp(dir: &std::path::Path) -> u64 {
    let mut h = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                    h ^= m.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64).rotate_left(h as u32 & 31);
                }
            }
        }
    }
    h
}

pub fn today() -> (i32, u32, u32) {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64;
    isl_sim::archive::civil_from_days(days)
}

impl Game {
    pub fn new(headless: bool) -> Game {
        let (content, data_dir) = read_content();
        let paths = Paths::discover();
        let settings = if headless { Settings::default() } else { Settings::load(&paths.settings()) };
        let card = if headless { MemoryCard::new() } else { isl_save::load_card(&paths.card()).unwrap_or_else(|_| MemoryCard::new()) };
        let campaign = card.campaign_index().and_then(|i| match &card.entries[i].data {
            BlockData::Campaign(c) => Some((**c).clone()),
            _ => None,
        });
        let (w, h) = settings.low_res();
        let data_stamp = data_dir.as_deref().map_or(0, dir_stamp);
        Game {
            content,
            data_dir,
            data_stamp,
            reload_t: 0.0,
            dev_overlay: settings.dev_overlay,
            settings,
            paths,
            card,
            campaign,
            screen: Screen::Boot(0.0),
            mission: None,
            run: None,
            gauntlet: None,
            ui: Ui::new(w as f32, h as f32),
            time: 0.0,
            requests: vec![],
            pending_static: None,
            toast: None,
            preview: Default::default(),
            pause: None,
            magazine: false,
            photo_filter: 0,
            typed: String::new(),
            headless,
            frame: 0,
            last_stats: Default::default(),
            using_pad: [false; 2],
        }
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), 3.0));
    }

    pub fn low(&self) -> (f32, f32) {
        let (w, h) = self.settings.low_res();
        (w as f32, h as f32)
    }

    // ------------------------------------------------------------------ persistence

    pub fn save_settings(&self) {
        if !self.headless {
            self.settings.save(&self.paths.settings());
        }
    }

    pub fn save_card(&mut self) {
        // Campaign always lives in one entry, updated in place.
        if let Some(c) = &self.campaign {
            let blocks = isl_save::campaign_blocks(c);
            let e = Entry {
                title: format!("WAR {} - TIDE {}", c.war, c.tide.tide_count),
                icon: 0,
                blocks,
                data: BlockData::Campaign(Box::new(c.clone())),
            };
            let r = match self.card.campaign_index() {
                Some(i) => self.card.replace(i, e),
                None => self.card.add(e).map(|_| ()),
            };
            if let Err(err) = r {
                self.toast(format!("MEMORY CARD: {err}. DELETE SOMETHING TO SAVE."));
            }
        }
        if !self.headless
            && let Err(e) = isl_save::save_card(&self.paths.card(), &self.card)
        {
            self.toast(format!("SAVE FAILED: {e}"));
        }
    }

    pub fn save_build(&mut self, lo: &Loadout) -> bool {
        let b = build::resolve(lo, &self.content, &[], [false; 7]);
        let (war, tide) = self.campaign.as_ref().map_or((0, 0), |c| (c.war, c.tide.tide_count));
        let e = Entry {
            title: lo.name.clone(),
            icon: if lo.side == Side::Saint { 1 } else { 2 },
            blocks: b.block_cost,
            data: BlockData::Build { loadout: lo.clone(), profile: self.card.system.profile.clone(), war, tide, usage: 0 },
        };
        if self.card.system.archive.first_build.is_none() {
            self.card.system.archive.first_build = Some(lo.clone());
            self.card.system.archive.set_rival("THE ROOKIE", lo.clone(), war, tide);
        }
        match self.card.add(e) {
            Ok(_) => {
                self.save_card();
                true
            }
            Err(err) => {
                self.toast(format!("{err}"));
                false
            }
        }
    }

    // ------------------------------------------------------------------ hot reload

    fn hot_reload(&mut self, dt: f32) {
        if !cfg!(feature = "dev") {
            return;
        }
        self.reload_t += dt;
        if self.reload_t < 1.0 {
            return;
        }
        self.reload_t = 0.0;
        let Some(dir) = self.data_dir.clone() else { return };
        let s = dir_stamp(&dir);
        if s != self.data_stamp {
            self.data_stamp = s;
            match Content::load_dir(&dir) {
                Ok(c) => {
                    self.content = c;
                    self.toast("CONTENT RELOADED");
                }
                Err(e) => self.toast(format!("RELOAD FAILED: {}", e.to_string().chars().take(80).collect::<String>())),
            }
        }
    }

    // ------------------------------------------------------------------ mission launch

    fn effects_for(&self, side: Side, chassis: Option<&str>) -> (Vec<Effect>, Vec<Effect>) {
        let mut player = self.card.system.archive.meta_effects(chassis, &self.content);
        let mut enemy = vec![];
        if let Some(c) = &self.campaign {
            for t in self.content.disc_rot.iter().filter(|t| t.tier <= c.disc_rot) {
                for e in &t.effects {
                    match e {
                        Effect::EnemyDamage(_) | Effect::EnemyHp(_) | Effect::EnemySpeed(_) | Effect::Echo => enemy.push(e.clone()),
                        _ => player.push(e.clone()),
                    }
                }
            }
            for o in c.omens.iter().filter_map(|o| self.content.omen(o)) {
                for e in &o.effects {
                    match e {
                        Effect::EnemyDamage(_) | Effect::EnemyHp(_) | Effect::EnemySpeed(_) | Effect::Echo => enemy.push(e.clone()),
                        _ => player.push(e.clone()),
                    }
                }
            }
        }
        let _ = side;
        (player, enemy)
    }

    fn nemesis_from(&self, lo: &Loadout, profile: &HabitProfile, war: u32, tide: u32, rival: Option<String>, level: u32) -> NemesisSpec {
        let mut params = profile.to_params();
        params.aggression = (params.aggression + level as f32 * 0.05).min(0.95);
        let name = rival.clone().unwrap_or_else(|| format!("{} (YOU)", lo.name));
        NemesisSpec {
            name,
            rival,
            loadout: lo.clone(),
            params,
            hp_mul: 2.6 + level as f32 * 0.2,
            intro: profile.intro_card(war, tide, &self.content),
        }
    }

    /// Saved builds of `side` (from the card) turned into nemeses, plus named rivals.
    pub fn nemeses_of(&self, side: Side) -> Vec<NemesisSpec> {
        let mut v: Vec<NemesisSpec> = self
            .card
            .builds()
            .filter(|(_, lo, ..)| lo.side == side)
            .map(|(_, lo, p, w, t)| self.nemesis_from(lo, p, w, t, None, 0))
            .collect();
        for r in &self.card.system.archive.rivals {
            if r.loadout.side == side {
                v.push(self.nemesis_from(&r.loadout, &self.card.system.profile, r.war, r.tide, Some(r.title.clone()), r.level));
            }
        }
        v
    }

    pub fn start_launch(&mut self, l: Launch) {
        // Disc swap when the side changes.
        let side = match &l {
            Launch::War { side, .. } => Some(*side),
            Launch::RunFight => self.run.as_ref().and_then(|r| r.current()).map(|f| f.side),
            _ => None,
        };
        let last = self.campaign.as_ref().map(|c| c.last_side);
        if let (Some(s), Some(ls)) = (side, last)
            && s != ls
        {
            if let Some(c) = self.campaign.as_mut() {
                c.last_side = s;
            }
            self.screen = Screen::DiscSwap { t: 0.0, to: s, next: Box::new(l) };
            return;
        }
        if let (Launch::RunFight, Some(s)) = (&l, side)
            && self.run.as_ref().is_some_and(|r| r.index > 0)
        {
            self.screen = Screen::DiscSwap { t: 0.0, to: s, next: Box::new(l) };
            return;
        }
        let lore = (self.time * 7.0) as usize;
        self.screen = Screen::Loading { t: 0.0, lore, next: Box::new(l) };
    }

    pub fn build_mission(&mut self, l: &Launch, audio: &mut Audio) {
        let c = &self.content;
        let seed = (self.time * 1000.0) as u64 ^ 0x5157_4E54 ^ self.frame;
        let assists = self.settings.assists();
        let (world, kind) = match l {
            Launch::War { district, side, finale } => {
                let camp = self.campaign.as_mut().expect("campaign");
                let dw = camp.district(district).cloned().expect("district");
                let events = camp.take_event(district);
                let lo = camp.active[side_ix(*side)].clone();
                let war = camp.war;
                let tide = camp.tide.clone();
                let wrecks = camp.wrecks_in(district);
                let mut cfg = WorldConfig {
                    district: district.clone(),
                    state: dw.state(),
                    side: *side,
                    tide,
                    seed,
                    mode: Mode::Mission,
                    player_fx: vec![],
                    enemy_fx: vec![],
                    assists,
                    persistent: dw.damage.clone(),
                    wrecks,
                    spawns: None,
                    density: 1.0,
                    war,
                    bell: false,
                    static_night: vec![],
                    demo_disc: None,
                };
                for e in &events {
                    if let Some(ev) = c.events.iter().find(|x| &x.id == e) {
                        match ev.kind {
                            EventKind::CathedralBell => cfg.bell = true,
                            EventKind::DemoDisc => cfg.demo_disc = ev.spawn.clone(),
                            EventKind::Evacuation => cfg.density = 1.4,
                            EventKind::NightOfStatic => {}
                            EventKind::Blackout => {}
                            EventKind::SupplyDrop => {}
                        }
                    }
                }
                if events.iter().any(|e| c.events.iter().any(|x| &x.id == e && x.kind == EventKind::NightOfStatic)) {
                    cfg.static_night = self.nemeses_of(side.other()).into_iter().take(3).collect();
                } else if self.campaign.as_ref().is_some_and(|c| c.missions[0] + c.missions[1] >= 2) {
                    // A nemesis may stalk the district: one of your saved builds from the other side.
                    let pool = self.nemeses_of(side.other());
                    if !pool.is_empty() && seed.is_multiple_of(3) {
                        cfg.static_night = vec![pool[(seed as usize / 3) % pool.len()].clone()];
                    }
                }
                let (pfx, efx) = self.effects_for(*side, lo.chassis.as_deref());
                cfg.player_fx = pfx;
                cfg.enemy_fx = efx;
                if let Some(ending) = finale
                    && ending == "tide_between"
                {
                    // Your best custom build of the other side, driven by your own ghost.
                    let other = self.best_build(side.other());
                    let spec = self.nemesis_from(&other, &self.card.system.profile, war, 0, Some("YOUR OTHER HALF".into()), 3);
                    cfg.spawns = Some(vec![SpawnSpec::Nemesis(Box::new(spec))]);
                    cfg.mode = Mode::Gauntlet;
                }
                let mut w = World::new(c, cfg, vec![lo]);
                let supply = events.iter().any(|e| c.events.iter().any(|x| &x.id == e && x.kind == EventKind::SupplyDrop));
                if supply && let Some(pid) = w.player_id(0) {
                    w.fighters[pid as usize].currency += 400;
                }
                let k = match finale {
                    Some(e) => MissionKind::Finale { ending: e.clone() },
                    None => MissionKind::War { district: district.clone(), side: *side },
                };
                (w, k)
            }
            Launch::RunFight => {
                let run = self.run.as_ref().expect("run");
                let f = run.current().expect("fight").clone();
                let (pfx, efx) = run.omen_effects(c);
                let lo = run.builds[side_ix(f.side)].clone();
                let cfg = WorldConfig {
                    district: f.district.clone(),
                    state: f.state,
                    side: f.side,
                    tide: Default::default(),
                    seed: run.seed ^ run.index as u64,
                    mode: Mode::Run,
                    player_fx: pfx,
                    enemy_fx: efx,
                    assists,
                    persistent: vec![],
                    wrecks: vec![],
                    spawns: Some(f.spawns.clone()),
                    density: 1.0,
                    war: 0,
                    bell: false,
                    static_night: vec![],
                    demo_disc: None,
                };
                (World::new(c, cfg, vec![lo]), MissionKind::Run)
            }
            Launch::Gauntlet { side, queue, index } => {
                let lo = self.active_or_premade(*side);
                let spec = queue[*index].clone();
                let cfg = WorldConfig {
                    district: "breakwater".into(),
                    state: DistrictState::Contested,
                    side: *side,
                    tide: Default::default(),
                    seed,
                    mode: Mode::Gauntlet,
                    player_fx: self.card.system.archive.meta_effects(lo.chassis.as_deref(), c),
                    enemy_fx: vec![],
                    assists,
                    persistent: vec![],
                    wrecks: vec![],
                    spawns: Some(vec![SpawnSpec::Nemesis(Box::new(spec))]),
                    density: 1.0,
                    war: 0,
                    bell: false,
                    static_night: vec![],
                    demo_disc: None,
                };
                (World::new(c, cfg, vec![lo]), MissionKind::Gauntlet)
            }
            Launch::Arena { loadout } => {
                let dummy = if loadout.side == Side::Saint { "dummy_kaiju" } else { "dummy_saint" };
                let habits = self.card.system.profile.to_params();
                let cfg = WorldConfig {
                    district: "harbor".into(),
                    state: DistrictState::Fortified,
                    side: loadout.side,
                    tide: Default::default(),
                    seed,
                    mode: Mode::Arena,
                    player_fx: vec![],
                    enemy_fx: vec![Effect::EnemyDamage(0.5)],
                    assists,
                    persistent: vec![],
                    wrecks: vec![],
                    spawns: Some(vec![
                        SpawnSpec::Dummy { id: dummy.into(), habits: None },
                        SpawnSpec::Dummy { id: dummy.into(), habits: Some(habits) },
                    ]),
                    density: 1.0,
                    war: 0,
                    bell: false,
                    static_night: vec![],
                    demo_disc: None,
                };
                (World::new(c, cfg, vec![(**loadout).clone()]), MissionKind::Arena)
            }
            Launch::Versus { a, b } => {
                let cfg = WorldConfig {
                    district: "breakwater".into(),
                    state: DistrictState::Contested,
                    side: a.side,
                    tide: Default::default(),
                    seed,
                    mode: Mode::Versus,
                    player_fx: vec![],
                    enemy_fx: vec![],
                    assists,
                    persistent: vec![],
                    wrecks: vec![],
                    spawns: Some(vec![]),
                    density: 1.0,
                    war: 0,
                    bell: false,
                    static_night: vec![],
                    demo_disc: None,
                };
                (World::new(c, cfg, vec![(**a).clone(), (**b).clone()]), MissionKind::Versus)
            }
        };
        let mut m = Mission::new(world, kind, c);
        if let Launch::Arena { .. } = l {
            m.note("TEST ARENA: LEFT DUMMY IS INERT, RIGHT DUMMY REPLAYS YOUR HABITS", [200, 200, 220, 255]);
        }
        if let Some(n) = m.world.fighters.iter().find(|f| matches!(f.kind, FighterKind::Nemesis { .. }))
            && let Some(spec) = m
                .world
                .cfg
                .spawns
                .as_ref()
                .and_then(|s| s.iter().find_map(|x| if let SpawnSpec::Nemesis(n) = x { Some(n.clone()) } else { None }))
                .or(m.world.cfg.static_night.first().cloned().map(Box::new))
        {
            m.intro = Some(crate::mission::Intro { title: n.name.clone(), sub: "NEMESIS".into(), card: spec.intro.clone(), ttl: 5.0 });
        }
        let d = c.district(&m.world.cfg.district).unwrap();
        audio.load_music(isl_sim::rng::hash_str(&d.id) ^ m.world.cfg.state as u64, &d.music);
        audio.set_layer(Layer::Explore);
        self.mission = Some(m);
        self.screen = Screen::Mission;
        self.pause = None;
    }

    pub fn active_or_premade(&self, side: Side) -> Loadout {
        if let Some(c) = &self.campaign {
            return c.active[side_ix(side)].clone();
        }
        let ch = self.content.starting_chassis(side).next().unwrap();
        Loadout::premade(ch, &self.content, 1)
    }

    /// Your "best" build of a side: the most-used saved build, else the active loadout.
    pub fn best_build(&self, side: Side) -> Loadout {
        let a = &self.card.system.archive;
        self.card
            .builds()
            .filter(|(_, lo, ..)| lo.side == side)
            .max_by_key(|(_, lo, ..)| a.usage.get(&lo.hash()).copied().unwrap_or(0))
            .map(|(_, lo, ..)| lo.clone())
            .unwrap_or_else(|| self.active_or_premade(side))
    }

    // ------------------------------------------------------------------ mission end

    fn finish_mission(&mut self, end: End, audio: &mut Audio) {
        let Some(m) = self.mission.take() else { return };
        audio.stop_music();
        let content = self.content.clone();
        let c = &content;
        let player = m.world.player(0).cloned();
        let stats = m.world.stats.clone();
        let won = end == End::Victory;
        // Habits always feed the profile.
        self.card.system.profile.merge(&m.recorder.profile);
        if let Some(p) = &player {
            self.card.system.archive.note_usage(p.loadout.hash(), m.play_ticks);
        }
        // Rage-quit detection for THE GRUDGE: quitting within 20 s of dying.
        if end == End::Retreat
            && let (Some(d), Some(p)) = (m.last_death_tick, &player)
            && m.world.tick.saturating_sub(d) < 1200
        {
            let (w, t) = self.campaign.as_ref().map_or((0, 0), |c| (c.war, c.tide.tide_count));
            self.card.system.archive.set_rival("THE GRUDGE", p.loadout.clone(), w, t);
        }
        // Champion: most-used build at its best.
        if let Some(p) = &player {
            let a = &self.card.system.archive;
            let best = a.usage.iter().max_by_key(|(_, v)| **v).map(|(k, _)| *k);
            if best == Some(p.loadout.hash()) && p.loadout.chassis.is_none() {
                let (w, t) = self.campaign.as_ref().map_or((0, 0), |c| (c.war, c.tide.tide_count));
                self.card.system.archive.set_rival("THE CHAMPION", p.loadout.clone(), w, t);
            }
        }
        let mut lines = vec![];
        let collected = m.world.collected.clone();
        let unlocks = self.card.system.archive.record_mission(c, m.world.cfg.side, &stats, &collected);
        for n in &stats.nemeses_killed {
            if let Some(r) = self.card.system.archive.rivals.iter_mut().find(|r| &r.title == n) {
                r.defeated += 1;
            }
        }
        match m.kind.clone() {
            MissionKind::War { district, side } => {
                let camp = self.campaign.as_mut().unwrap();
                let ruin = m.world.city.ruin();
                camp.apply_result(c, &district, side, won, &stats, ruin, m.world.city.damage_mask());
                camp.merge_wrecks(&district, m.world.wrecks.clone());
                let carried = player.as_ref().map_or(0, |p| p.currency);
                camp.inventory[side_ix(side)].currency += carried;
                lines.push(format!(
                    "{} {}",
                    if won { "VICTORY AT" } else { "RETREAT FROM" },
                    c.district(&district).map_or("", |d| d.name.as_str())
                ));
                lines.push(format!("{} BANKED: {}", side.currency(), carried));
                lines.push(format!("PARTS HARVESTED: {}", collected.len()));
                for p in collected.iter().take(6) {
                    let name = c.part(&p.def).map_or(p.def.clone(), |d| d.name.clone());
                    let dest = c.part(&p.def).map_or(side, |d| d.side);
                    lines.push(format!("  {} [{}] -> {}", name, p.condition.name(), if dest == Side::Saint { "FOUNDRY" } else { "NEST" }));
                }
                camp.store_parts(collected.clone(), c);
                if let Some((ct, inst)) = camp.check_contract(c, &district, side, &stats, won) {
                    lines.push(format!("CONTRACT DONE: {} (+{})", ct.name, ct.reward_currency));
                    camp.store_parts(vec![inst], c);
                }
                if won {
                    if let Some(ev) = camp.roll_event(c) {
                        lines.push(format!("WAR EVENT: {}", ev.name));
                    }
                    if camp.missions.iter().sum::<u32>() % 4 == 0 {
                        camp.roll_contracts(c);
                    }
                }
                let xp = if won { 400 } else { 120 } + stats.kills * 15;
                if let Some(ch) = player.as_ref().and_then(|p| p.loadout.chassis.clone()).or_else(|| {
                    player.as_ref().and_then(|p| p.loadout.get(Slot::Frame)).and_then(|p| p.origin.owner_tag().map(String::from))
                }) {
                    for l in self.card.system.archive.add_xp(&ch, xp, c) {
                        lines.push(l);
                    }
                }
                if self.campaign.as_ref().is_some_and(|c| c.breakwater_open) {
                    lines.push("THE BREAKWATER IS OPEN.".into());
                }
            }
            MissionKind::Finale { ending } => {
                if won {
                    lines.extend(unlocks.clone());
                    self.save_card();
                    self.begin_ending(&ending);
                    return;
                } else {
                    lines.push("THE BREAKWATER HOLDS.".into());
                }
            }
            MissionKind::Run => {
                let run = self.run.as_mut().unwrap();
                if won {
                    run.win_fight(c, collected.clone());
                    for p in &collected {
                        self.card.system.archive.register(&p.def);
                    }
                    if run.over {
                        let score = run.score;
                        self.finish_run(true, score);
                        return;
                    }
                    self.screen = Screen::RunDraft { sel: 0 };
                    self.save_card();
                    return;
                } else {
                    run.lose();
                    let score = run.score;
                    self.finish_run(false, score);
                    return;
                }
            }
            MissionKind::Gauntlet => {
                if let Some((side, queue, idx)) = self.gauntlet.clone() {
                    if won && idx + 1 < queue.len() {
                        self.gauntlet = Some((side, queue.clone(), idx + 1));
                        self.start_launch(Launch::Gauntlet { side, queue, index: idx + 1 });
                        return;
                    }
                    lines.push(if won { "GAUNTLET CLEARED".into() } else { format!("FELL TO {}", queue[idx].name) });
                    self.gauntlet = None;
                }
            }
            MissionKind::Arena => {
                self.save_card();
                if let Some(c) = &self.campaign {
                    let side = m.world.cfg.side;
                    let lo = c.active[side_ix(side)].clone();
                    self.screen = Screen::Foundry(Box::new(Assembly::new(side, lo)));
                } else {
                    self.screen = Screen::Main { sel: 0 };
                }
                return;
            }
            MissionKind::Versus => {
                let a_dead = m.world.player(0).is_some_and(|f| matches!(f.act, Act::Dead));
                lines.push(if a_dead { "PLAYER 2 WINS".into() } else { "PLAYER 1 WINS".into() });
            }
        }
        lines.extend(unlocks);
        self.save_card();
        let title = if won { "MISSION COMPLETE" } else { "MISSION OVER" }.to_string();
        let next = if self.campaign.is_some() && matches!(m.kind, MissionKind::War { .. } | MissionKind::Finale { .. }) {
            Screen::WarMap { sel: 0, side: m.world.cfg.side }
        } else {
            Screen::Main { sel: 0 }
        };
        self.screen = Screen::Results { title, lines, next: Box::new(next) };
    }

    fn finish_run(&mut self, won: bool, score: u32) {
        let run = self.run.take().unwrap();
        let a = &mut self.card.system.archive;
        a.run_best = a.run_best.max(score);
        if let Some(day) = &run.daily {
            let e = a.daily_best.entry(day.clone()).or_default();
            *e = (*e).max(score);
        }
        let mut lines =
            vec![format!("SCORE {score}"), format!("SEED {:08X}", run.seed as u32), format!("FIGHTS {}/{}", run.index, run.fights.len())];
        for p in run.harvested.iter().take(8) {
            lines.push(format!("CODEX: {}", self.content.part(&p.def).map_or(p.def.as_str(), |d| d.name.as_str())));
        }
        for b in &run.builds {
            if let Some(ch) = b.get(Slot::Frame).and_then(|p| p.origin.owner_tag()) {
                let ch = ch.to_string();
                let _ = self.card.system.archive.add_xp(&ch, score / 10, &self.content);
            }
        }
        self.save_card();
        self.screen = Screen::Results {
            title: if won { "THE TIDE RECEDES".into() } else { "THE TIDE TAKES YOU".into() },
            lines,
            next: Box::new(Screen::Main { sel: 0 }),
        };
    }

    pub fn begin_ending(&mut self, ending: &str) {
        let c = &self.content;
        let e = c.endings.iter().find(|e| e.id == ending).cloned();
        let mut next = Screen::Corrupt { sel: 0, ending: ending.to_string() };
        // Disc 3: the corrupted chain has reached three wars and you found the tide between.
        let chain = self.card.corrupted().map_or(0, |(_, _, _, ch)| ch);
        if ending == "tide_between"
            && chain >= 2
            && let Some(d3) = c.endings.iter().find(|e| e.id == "disc3")
        {
            let mut lines = d3.card.clone();
            lines.extend(c.lore.disc3.clone());
            next = Screen::Cutscene(Box::new(Fmv::ending("disc3", lines, Box::new(next))));
        }
        let lines = e.map_or(vec![ending.to_string()], |e| {
            let mut v = vec![e.name.clone()];
            v.extend(e.card.clone());
            v
        });
        self.screen = Screen::Cutscene(Box::new(Fmv::ending(ending, lines, Box::new(next))));
    }

    /// End of war: apply ending rot to saved builds, record, and write the corrupted block.
    pub fn conclude_war(&mut self, ending: &str, carry: Option<CorruptItem>) {
        let c = &self.content;
        for e in self.card.entries.iter_mut() {
            if let BlockData::Build { loadout, .. } = &mut e.data {
                let mut parts: Vec<PartInstance> = loadout.slots.iter().flatten().cloned().collect();
                Campaign::apply_ending_rot(ending, &mut parts, c);
                let mut it = parts.into_iter();
                for s in loadout.slots.iter_mut() {
                    if s.is_some() {
                        *s = it.next();
                    }
                }
            }
        }
        let unlocks = self.card.system.archive.finish_war(ending, c);
        let war = self.campaign.as_ref().map_or(1, |c| c.war);
        if ending == "disc3" || (ending == "tide_between" && self.card.corrupted().is_some_and(|x| x.3 >= 2)) {
            self.card.system.archive.seen_endings_disc3 = true;
        }
        if let Some(item) = carry {
            let chain = self.card.corrupted().map_or(0, |(_, _, _, ch)| ch) + 1;
            if let Some((i, ..)) = self.card.corrupted() {
                self.card.delete(i);
            }
            self.card.system.archive.record.corrupted_chain = chain;
            let _ = self.card.add(Entry { title: "????????".into(), icon: 9, blocks: 1, data: BlockData::Corrupted { item, war, chain } });
        }
        if let Some(i) = self.card.campaign_index() {
            self.card.delete(i);
        }
        self.campaign = None;
        self.save_card();
        self.screen = Screen::Results { title: format!("WAR {war} ENDED"), lines: unlocks, next: Box::new(Screen::Main { sel: 0 }) };
    }

    pub fn new_campaign(&mut self, condition: Option<String>, rot: u8, use_corrupt: bool) {
        let content = self.content.clone();
        let c = &content;
        let war = self.card.system.archive.record.wars + 1;
        let cond = condition.as_ref().and_then(|id| c.conditions.iter().find(|x| &x.id == id));
        let s = c.starting_chassis(Side::Saint).next().unwrap();
        let k = c.starting_chassis(Side::Kaiju).next().unwrap();
        let seed = (self.time * 977.0) as u64 ^ (war as u64) << 20 ^ 0x1998;
        let mut camp = Campaign::new(c, war, seed, cond, Loadout::premade(s, c, 10), Loadout::premade(k, c, 30));
        camp.disc_rot = rot;
        if use_corrupt && let Some((_, item, _, _)) = self.card.corrupted() {
            match item.clone() {
                CorruptItem::Part(mut p) => {
                    if !p.quirks.iter().any(|q| q == "corrupted") {
                        p.quirks.push("corrupted".into());
                    }
                    p.origin = Origin::Corrupted { war: war - 1 };
                    camp.store_parts(vec![p], c);
                }
                CorruptItem::Trait(t) => camp.omens.push(t),
            }
        }
        self.campaign = Some(camp);
        self.save_card();
        let intro = c.lore.intro.clone();
        self.screen = Screen::Cutscene(Box::new(Fmv::intro(intro, Box::new(Screen::WarMap { sel: 0, side: Side::Saint }))));
    }

    // ------------------------------------------------------------------ per-frame

    pub fn update(&mut self, dt: f32, input: &mut Input, audio: &mut Audio) {
        self.time += dt;
        for i in 0..2 {
            if input.players[i].using_pad {
                self.using_pad[i] = true;
            } else if !input.players[i].pressed.is_empty() {
                self.using_pad[i] = false;
            }
        }
        self.frame += 1;
        self.ui.text_scale = self.settings.subtitle_size;
        if let Some(t) = self.toast.as_mut() {
            t.1 -= dt;
        }
        if self.toast.as_ref().is_some_and(|t| t.1 <= 0.0) {
            self.toast = None;
        }
        self.hot_reload(dt);
        audio.update(dt);
        let screen = std::mem::replace(&mut self.screen, Screen::Boot(0.0));
        self.screen = crate::menus::update(self, screen, dt, input, audio);
        if let Screen::Mission = self.screen {
            self.update_mission(dt, input, audio);
        }
    }

    fn update_mission(&mut self, dt: f32, input: &mut Input, audio: &mut Audio) {
        let inputs: [PlayerInput; 2] = [input.players[0].clone(), input.players[1].clone()];
        // Pause menu.
        if let Some(sel) = self.pause {
            let mut sel = sel;
            let p = &inputs[0];
            let n = 5;
            if p.hit(Action::MenuDown) {
                sel = (sel + 1) % n;
                audio.play(Sfx::UiMove, None, 0.5, 1.0);
            }
            if p.hit(Action::MenuUp) {
                sel = (sel + n - 1) % n;
                audio.play(Sfx::UiMove, None, 0.5, 1.0);
            }
            if p.hit(Action::Back) || p.hit(Action::Pause) {
                self.pause = None;
                return;
            }
            self.pause = Some(sel);
            if p.hit(Action::Confirm) {
                audio.play(Sfx::UiOk, None, 0.6, 1.0);
                match sel {
                    0 => self.pause = None,
                    1 => {
                        self.pause = None;
                        if let Some(m) = self.mission.as_mut() {
                            m.enter_photo();
                        }
                    }
                    2 => self.screen = Screen::Settings { tab: 0, sel: 0 },
                    3 => {
                        self.pause = None;
                        self.finish_mission(End::Retreat, audio);
                    }
                    _ => {
                        self.pause = None;
                        self.finish_mission(End::Retreat, audio);
                        self.requests.push(Request::Quit);
                    }
                }
            }
            return;
        }
        let Some(m) = self.mission.as_mut() else {
            self.screen = Screen::Main { sel: 0 };
            return;
        };
        if m.photo {
            let p = &inputs[0];
            if p.hit(Action::Back) || p.hit(Action::Pause) {
                m.photo = false;
            }
            if p.hit(Action::TabNext) {
                self.photo_filter = (self.photo_filter + 1) % 4;
            }
            if p.hit(Action::TabPrev) {
                self.magazine = !self.magazine;
            }
            if p.hit(Action::Confirm) || p.hit(Action::Photo) {
                self.requests.push(Request::SavePhoto);
                audio.play(Sfx::Click, None, 0.8, 0.6);
            }
            let m = self.mission.as_mut().unwrap();
            m.update(&self.content, dt, &inputs, &self.settings, audio);
            return;
        }
        if inputs[0].hit(Action::Pause) {
            self.pause = Some(0);
            audio.play(Sfx::UiOk, None, 0.6, 1.0);
            return;
        }
        if inputs[0].hit(Action::Photo) {
            m.enter_photo();
            return;
        }
        let rests_before = m.world.stats.ticks;
        let _ = rests_before;
        m.update(&self.content, dt, &inputs, &self.settings, audio);
        for (p, s, ms) in m.rumble.drain(..) {
            if self.settings.rumble {
                input.rumble(p, s, ms);
            }
        }
        // Music layer from situation.
        let boss = m
            .world
            .fighters
            .iter()
            .find(|f| f.alive && f.kind.is_bossish() && f.ai.as_ref().is_some_and(|a| a.awake) && !matches!(f.act, Act::Dead));
        let layer = match boss {
            Some(b) => match b.boss_phase {
                0 => Layer::Boss1,
                1 => Layer::Boss2,
                _ => Layer::Boss3,
            },
            None => {
                let me = m.world.player(0);
                let near = me.is_some_and(|p| {
                    m.world.fighters.iter().any(|f| {
                        f.alive
                            && !matches!(f.act, Act::Dead)
                            && f.team.hostile(p.team)
                            && f.ai.as_ref().is_some_and(|a| a.awake)
                            && f.pos.distance(p.pos) < 50.0
                    })
                });
                if near { Layer::Combat } else { Layer::Explore }
            }
        };
        audio.set_layer(layer);
        // Rest advanced the tide: sync campaign clock into the world.
        if m.rests > 0 {
            m.rests = 0;
            if let Some(c) = self.campaign.as_mut() {
                c.rest();
                m.world.cfg.tide = c.tide.clone();
                let d = self.content.district(&m.world.cfg.district).unwrap();
                let sv = &d.states[m.world.cfg.state.index()];
                m.world.water = sv.water + c.tide.water_offset();
                let (l, f, n, fr) = crate::district_mesh::lighting(d, m.world.cfg.state, &c.tide, &m.world.city);
                m.light = l;
                m.fog = f;
                m.fog_near = n;
                m.fog_far = fr;
                m.static_dirty = true;
            }
        }
        if let Some(end) = m.end {
            self.finish_mission(end, audio);
        }
    }

    /// Fill the frame: 3D views, UI, post settings.
    pub fn draw(&mut self, views: &mut Vec<(View, DrawList)>, post: &mut PostSettings) {
        let (w, h) = self.low();
        self.ui.begin(w, h, self.time);
        post.snap = self.settings.vertex_jitter;
        post.dither = self.settings.dither;
        post.crt = self.settings.crt && !self.settings.photosensitive;
        post.composite = self.settings.composite;
        post.scanline = self.settings.scanlines;
        post.integer_scale = self.settings.integer_scale;
        post.time = self.time;
        post.fmv = 0.0;
        crate::menus::draw(self, views, post);
        if let Some((t, ttl)) = &self.toast {
            let a = (ttl.min(1.0) * 255.0) as u8;
            let tw = self.ui.text_width(t) + 12.0;
            self.ui.rect((w - tw) * 0.5, h - 26.0, tw, 13.0, [0, 0, 0, a / 2]);
            self.ui.text_center(w * 0.5, h - 23.0, &t.clone(), [255, 230, 160, a]);
        }
        if self.dev_overlay {
            let s = format!("{} DC {} TRI {}", self.frame, self.last_stats.draw_calls, self.last_stats.tris);
            self.ui.text(2.0, h - 9.0, &s, isl_ui::col::GREEN);
        }
    }

    /// Feed mission events into the static mesh rebuild request.
    pub fn take_static(&mut self) -> Option<Vec<Vertex>> {
        if let Some(m) = self.mission.as_mut()
            && m.static_dirty
        {
            m.static_dirty = false;
            let d = self.content.district(&m.world.cfg.district).unwrap();
            return Some(crate::district_mesh::build(&m.world.city, d, &m.light));
        }
        self.pending_static.take()
    }

    #[allow(dead_code)]
    fn unused(&self, _: SimEvent, _: Condition, _: Vec3) {}
    pub fn daily_seed_today() -> (u64, String) {
        let (y, m, d) = today();
        (daily_seed(y, m, d), format!("{y:04}-{m:02}-{d:02}"))
    }
}
