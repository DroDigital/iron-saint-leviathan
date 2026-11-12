//! The mission world: one district, its fighters, projectiles, hazards, loot and wrecks, stepped
//! at a fixed 60 Hz. Fully deterministic for a given seed and command stream.

use crate::ai::{self, AiParams, Brain, Percept, Seen};
use crate::build::{self, Loadout, MoveRef, WeightClass};
use crate::city::{CheckpointKind, City};
use crate::fighter::*;
use crate::geom::{self, Aabb, Capsule};
use crate::parts::{Condition, Effects, Origin, PartInstance};
use crate::rng::SimRng;
use crate::stamina::{DT, StaminaCtx};
use crate::tide::{TideClock, Weather};
use glam::{Vec2, Vec3};
use isl_data::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Global damage scale so content numbers stay readable.
pub const DAMAGE_SCALE: f32 = 1.6;
const HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Assists {
    /// Wider parry window.
    pub steady_hands: bool,
    /// Slower bosses.
    pub old_bones: bool,
    /// Keep half your currency on death.
    pub insurance: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NemesisSpec {
    pub name: String,
    pub rival: Option<String>,
    pub loadout: Loadout,
    pub params: AiParams,
    pub hp_mul: f32,
    pub intro: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SpawnSpec {
    Enemy(String),
    Boss(String),
    Nemesis(Box<NemesisSpec>),
    /// Training dummy; with params it replays recorded habits.
    Dummy {
        id: String,
        habits: Option<AiParams>,
    },
    Ally(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Mission,
    Arena,
    Versus,
    Run,
    Gauntlet,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub district: String,
    pub state: DistrictState,
    pub side: Side,
    pub tide: TideClock,
    pub seed: u64,
    pub mode: Mode,
    pub player_fx: Vec<Effect>,
    pub enemy_fx: Vec<Effect>,
    pub assists: Assists,
    pub persistent: Vec<Vec<bool>>,
    pub wrecks: Vec<Wreck>,
    pub spawns: Option<Vec<SpawnSpec>>,
    /// Extra enemy count multiplier (events, evacuation).
    pub density: f32,
    pub war: u32,
    /// Event modifiers.
    pub bell: bool,
    pub static_night: Vec<NemesisSpec>,
    pub demo_disc: Option<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wreck {
    pub uid: u64,
    pub side: Side,
    pub district: String,
    pub pos: Vec3,
    pub currency: u32,
    pub loadout: Loadout,
    pub war: u32,
    pub tide: u32,
    pub harvested: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub owner: u32,
    pub team: Team,
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f32,
    pub life: u16,
    pub gravity: f32,
    pub homing: f32,
    pub target: Option<u32>,
    pub mv: String,
    pub slot: Slot,
    pub hallucination: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hazard {
    pub kind: HazardKind,
    pub pos: Vec3,
    pub radius: f32,
    pub life: u16,
    pub team: Team,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PickupItem {
    Part(PartInstance),
    Currency(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pickup {
    pub pos: Vec3,
    pub item: PickupItem,
    pub age: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MissionStats {
    pub kills: u32,
    pub parries: u32,
    pub heals: u32,
    pub damage_taken: f32,
    /// (owner tag, slot, side of victim) for each sever.
    pub severs: Vec<(String, Slot, Side)>,
    /// (owner tag, slot) for parts harvested off corpses/drops (cores etc.).
    pub harvested: Vec<(String, Slot)>,
    pub eaten: u32,
    pub ticks: u64,
    pub deaths: u32,
    pub boss_killed: Option<String>,
    pub nemeses_killed: Vec<String>,
    /// Tags of chassis-bosses beaten (licence wins against).
    pub beaten_tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub tick: u64,
    pub rng: SimRng,
    pub cfg: WorldConfig,
    pub city: City,
    pub fighters: Vec<Fighter>,
    pub projectiles: Vec<Projectile>,
    pub hazards: Vec<Hazard>,
    pub pickups: Vec<Pickup>,
    pub wrecks: Vec<Wreck>,
    #[serde(skip)]
    pub events: Vec<SimEvent>,
    #[serde(skip)]
    history: Vec<VecDeque<Seen>>,
    pub water: f32,
    pub next_uid: u64,
    pub boss: Option<u32>,
    pub boss_awake: bool,
    pub last_checkpoint: usize,
    pub stats: MissionStats,
    pub victory: bool,
    pub collected: Vec<PartInstance>,
    pub reveal: u16,
    pub enemy_fx: Effects,
    respawn_list: Vec<(SpawnSpec, Vec3)>,
    /// Fog density multiplier for the renderer (weather).
    pub fog_mul: f32,
}

fn two<T>(v: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert!(a != b);
    if a < b {
        let (l, r) = v.split_at_mut(b);
        (&mut l[a], &mut r[0])
    } else {
        let (l, r) = v.split_at_mut(a);
        (&mut r[0], &mut l[b])
    }
}

pub fn parts_loadout(
    name: &str,
    side: Side,
    parts: &[String; 7],
    content: &Content,
    origin: Origin,
    uid: u64,
    paint: [[u8; 3]; 2],
) -> Loadout {
    let slots = std::array::from_fn(|i| {
        content.part(&parts[i]).map(|d| PartInstance {
            uid: uid + i as u64,
            def: d.id.clone(),
            condition: Condition::Pristine,
            quirks: d.quirks.clone(),
            origin: origin.clone(),
            memory: d.memory.clone(),
        })
    });
    Loadout { name: name.to_string(), side, chassis: None, slots, paint, decal: 0, stencil: String::new() }
}

impl World {
    pub fn new(content: &Content, cfg: WorldConfig, players: Vec<Loadout>) -> World {
        let d = content.district(&cfg.district).expect("district").clone();
        let mut state = cfg.state;
        if cfg.player_fx.iter().any(|e| matches!(e, Effect::Riptide)) && state == DistrictState::Fortified {
            state = DistrictState::Contested;
        }
        let persistent = cfg.persistent.clone();
        let mut city = City::generate(&d, state, &persistent);
        if matches!(cfg.mode, Mode::Arena | Mode::Versus) {
            // Clear a big plaza around the arena.
            let (c, r) = city.arena;
            city.buildings.retain(|b| b.aabb.center().distance(c) > r + 30.0);
        }
        let sv = &d.states[state.index()];
        let mut enemy_fx = Effects::default();
        for e in &cfg.enemy_fx {
            enemy_fx.add(e, None);
        }
        let riptide = cfg.player_fx.iter().any(|e| matches!(e, Effect::Riptide));
        let water = sv.water + cfg.tide.water_offset() + if riptide { 1.5 } else { 0.0 };
        let fog_mul = match cfg.tide.weather {
            Weather::FogBank => 0.55,
            Weather::Storm => 0.8,
            _ => 1.0,
        };
        let mut w = World {
            tick: 0,
            rng: SimRng::new(cfg.seed),
            wrecks: cfg.wrecks.clone(),
            cfg,
            city,
            fighters: vec![],
            projectiles: vec![],
            hazards: vec![],
            pickups: vec![],
            events: vec![],
            history: vec![],
            water,
            next_uid: 1 << 40,
            boss: None,
            boss_awake: false,
            last_checkpoint: 0,
            stats: MissionStats::default(),
            victory: false,
            collected: vec![],
            reveal: 0,
            enemy_fx,
            respawn_list: vec![],
            fog_mul,
        };
        // Players.
        let side = w.cfg.side;
        let start_cp = w.city.checkpoints.iter().position(|c| c.kind.side() == side).unwrap_or(0);
        w.last_checkpoint = start_cp;
        for (n, lo) in players.into_iter().enumerate() {
            let mut pos = w.city.checkpoints[start_cp].pos + Vec3::new(n as f32 * 14.0, 0.0, -10.0);
            let team = if n == 0 { Team::Player } else { Team::Player2 };
            if w.cfg.mode == Mode::Versus || w.cfg.mode == Mode::Arena {
                let (c, _) = w.city.arena;
                pos = c + Vec3::new(0.0, 0.0, if n == 0 { -24.0 } else { 24.0 });
            }
            let fx = w.cfg.player_fx.clone();
            let id = w.spawn_loadout(content, team, FighterKind::Player(n as u8), "player".into(), lo.name.clone(), lo, fx, pos, 1.0);
            if n == 1 {
                w.fighters[id as usize].yaw = std::f32::consts::PI;
            }
        }
        // Enemies.
        if let Some(list) = w.cfg.spawns.clone() {
            let (c, r) = w.city.arena;
            let n = list.len().max(1);
            for (i, s) in list.into_iter().enumerate() {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let pos = c + Vec3::new(a.sin() * r * 0.5, 0.0, a.cos().abs() * r * 0.6 + 6.0);
                w.spawn_spec(content, s.clone(), pos);
                w.respawn_list.push((s, pos));
            }
        } else if w.cfg.mode == Mode::Mission {
            let list = match side {
                Side::Saint => sv.saint_enemies.clone(),
                Side::Kaiju => sv.kaiju_enemies.clone(),
            };
            let count = ((w.city.spawns.len() as f32) * 0.7 * w.cfg.density).round() as usize;
            let spawns = w.city.spawns.clone();
            for (i, pos) in spawns.into_iter().take(count.max(4)).enumerate() {
                if list.is_empty() {
                    break;
                }
                let id = list[i % list.len()].clone();
                let n = if content.enemy(&id).is_some_and(|e| e.archetype == Archetype::Swarmer) { 3 } else { 1 };
                for k in 0..n {
                    let p = pos + Vec3::new(k as f32 * 6.0, 0.0, k as f32 * 3.0);
                    w.spawn_spec(content, SpawnSpec::Enemy(id.clone()), p);
                    w.respawn_list.push((SpawnSpec::Enemy(id.clone()), p));
                }
            }
            // District boss, dormant in the arena.
            let boss_id = match side {
                Side::Saint => d.saint_boss.clone(),
                Side::Kaiju => d.kaiju_boss.clone(),
            };
            let bpos = w.city.arena.0 + Vec3::new(0.0, 0.0, 8.0);
            let bid = w.spawn_spec(content, SpawnSpec::Boss(boss_id), bpos);
            w.boss = bid;
            // Events.
            if w.cfg.bell {
                let p = w.city.arena.0 + Vec3::new(-20.0, 0.0, 30.0);
                match side {
                    Side::Saint => {
                        w.spawn_spec(content, SpawnSpec::Ally("boss_st_vesper".into()), p);
                    }
                    Side::Kaiju => {
                        w.spawn_spec(content, SpawnSpec::Boss("boss_st_vesper".into()), p);
                    }
                }
            }
            if let Some((a, b)) = w.cfg.demo_disc.clone() {
                let p = w.city.spawns.first().copied().unwrap_or(Vec3::ZERO);
                let id = if side == Side::Saint { a } else { b };
                w.spawn_spec(content, SpawnSpec::Enemy(id), p);
            }
            for (i, n) in w.cfg.static_night.clone().into_iter().enumerate() {
                let p = w.city.spawns.get(i + 3).copied().unwrap_or(Vec3::new(i as f32 * 20.0, 0.0, 0.0));
                w.spawn_spec(content, SpawnSpec::Nemesis(Box::new(n)), p);
            }
        }
        w.history = vec![VecDeque::new(); w.fighters.len()];
        w
    }

    fn uid(&mut self) -> u64 {
        self.next_uid += 16;
        self.next_uid
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_loadout(
        &mut self,
        content: &Content,
        team: Team,
        kind: FighterKind,
        tag: String,
        name: String,
        lo: Loadout,
        fx: Vec<Effect>,
        pos: Vec3,
        scale: f32,
    ) -> u32 {
        let b = build::resolve(&lo, content, &fx, [false; 7]);
        // Reuse a despawned slot so long sessions (many rests/respawns) never grow unbounded.
        let reuse = self.fighters.iter().position(|f| !f.alive && !f.kind.is_player());
        let id = reuse.unwrap_or(self.fighters.len()) as u32;
        if reuse.is_some() {
            for o in self.fighters.iter_mut() {
                if o.lock.is_some_and(|l| l.target == id) {
                    o.lock = None;
                }
                if let Some(a) = o.ai.as_mut() {
                    if a.target == Some(id) {
                        a.target = None;
                    }
                }
                if o.owner == Some(id) {
                    o.owner = None;
                }
            }
            self.projectiles.retain(|p| p.owner != id);
        }
        let mut f = Fighter::new(id, team, kind, tag, name, lo, b, pos, scale);
        f.extra_fx = fx;
        f.anim_speed = f.build.fx.get(Stat::AttackSpeed);
        f.pos.y = self.city.ground(pos.x, pos.z).max(0.0);
        if f.build.flying {
            f.altitude = 5.0;
        }
        // Face the arena / opponent.
        let c = self.city.arena.0;
        f.yaw = geom::dir_yaw(c - pos);
        if let Some(i) = reuse {
            self.fighters[i] = f;
            if let Some(h) = self.history.get_mut(i) {
                h.clear();
            }
        } else {
            self.fighters.push(f);
            self.history.push(VecDeque::new());
        }
        id
    }

    pub fn spawn_spec(&mut self, content: &Content, s: SpawnSpec, pos: Vec3) -> Option<u32> {
        let enemy_hp = self.enemy_fx.enemy_hp;
        let enemy_dmg = self.enemy_fx.enemy_damage;
        let enemy_speed = self.enemy_fx.enemy_speed;
        let uid = self.uid();
        let habits = match &s {
            SpawnSpec::Dummy { habits, .. } => habits.clone(),
            _ => None,
        };
        let ally = matches!(s, SpawnSpec::Ally(_));
        match s {
            SpawnSpec::Enemy(id) | SpawnSpec::Dummy { id, .. } => {
                let e = content.enemy(&id)?.clone();
                let lo = parts_loadout(&e.name, e.side, &e.parts, content, Origin::Enemy(e.id.clone()), uid, [e.color, e.color]);
                let fid = self.spawn_loadout(
                    content,
                    Team::Enemy,
                    FighterKind::Enemy(e.id.clone()),
                    e.id.clone(),
                    e.name.clone(),
                    lo,
                    vec![],
                    pos,
                    e.scale,
                );
                let f = &mut self.fighters[fid as usize];
                let hm = e.hp_mul * enemy_hp;
                f.build.hp_max *= hm;
                f.hp = f.build.hp_max;
                f.build.posture_max *= 0.6 + 0.4 * e.hp_mul.min(1.0);
                f.dmg_mul = enemy_dmg;
                f.speed_mul = enemy_speed;
                f.currency_value = e.currency;
                let mut brain = Brain::new(e.archetype, e.reaction);
                if let Some(p) = habits {
                    // Habit dummy: replays your recorded habits (but never attacks for real damage).
                    brain = Brain::new(Archetype::Brute, 14);
                    brain.params = p;
                    brain.aggro = 999.0;
                }
                if e.archetype == Archetype::Dummy {
                    f.build.hp_max = 99_999.0;
                    f.hp = 99_999.0;
                }
                if e.archetype == Archetype::Ambusher {
                    f.burrowed = true;
                }
                f.ai = Some(brain);
                Some(fid)
            }
            SpawnSpec::Boss(id) | SpawnSpec::Ally(id) => {
                let b = content.boss(&id)?.clone();
                let lo = parts_loadout(
                    &b.name,
                    b.side,
                    &b.parts,
                    content,
                    Origin::Boss(b.chassis.clone().unwrap_or(b.id.clone())),
                    uid,
                    [[200, 200, 200], [90, 90, 90]],
                );
                let tag = b.chassis.clone().unwrap_or(b.id.clone());
                let team = if ally { Team::Ally } else { Team::Enemy };
                let fid = self.spawn_loadout(
                    content,
                    team,
                    FighterKind::Boss(b.id.clone()),
                    tag,
                    b.name.clone(),
                    lo,
                    vec![],
                    pos,
                    b.scale / self.fighter_base_size(content, &b.parts),
                );
                let old = self.cfg.assists.old_bones;
                let f = &mut self.fighters[fid as usize];
                f.build.hp_max *= b.hp_mul * enemy_hp;
                f.hp = f.build.hp_max;
                f.build.posture_max *= 1.0 + (b.hp_mul - 1.0) * 0.45;
                f.dmg_mul = enemy_dmg;
                f.speed_mul = enemy_speed * if old { 0.8 } else { 1.0 };
                if old {
                    f.anim_speed *= 0.8;
                }
                f.currency_value = b.currency;
                let mut brain = Brain::new(Archetype::Boss, b.reaction);
                brain.boss = Some(b.id.clone());
                brain.aggro = if ally { 80.0 } else { 0.0 };
                f.ai = Some(brain);
                if ally {
                    f.kind = FighterKind::Ally(b.id.clone());
                }
                Some(fid)
            }
            SpawnSpec::Nemesis(n) => {
                let tag = n.loadout.chassis.clone().unwrap_or_else(|| "nemesis".into());
                let fid = self.spawn_loadout(
                    content,
                    Team::Enemy,
                    FighterKind::Nemesis { name: n.name.clone(), rival: n.rival.clone() },
                    tag,
                    n.name.clone(),
                    n.loadout.clone(),
                    vec![],
                    pos,
                    1.0,
                );
                let f = &mut self.fighters[fid as usize];
                f.build.hp_max *= n.hp_mul * enemy_hp;
                f.hp = f.build.hp_max;
                f.build.posture_max *= 1.3;
                f.dmg_mul = enemy_dmg;
                f.currency_value = 800;
                let mut brain = Brain::new(Archetype::Brute, 14);
                brain.params = n.params.clone();
                brain.aggro = 90.0;
                f.ai = Some(brain);
                Some(fid)
            }
        }
    }

    /// Spawn an ally boss (e.g. St. Vesper answering the bell for the Saints).
    pub fn make_ally(&mut self, id: u32) {
        if let Some(f) = self.fighters.get_mut(id as usize) {
            f.team = Team::Ally;
            if let Some(b) = f.ai.as_mut() {
                b.aggro = 80.0;
            }
        }
    }

    fn fighter_base_size(&self, content: &Content, parts: &[String; 7]) -> f32 {
        content.part(&parts[0]).map_or(1.0, |p| if p.stats.size > 0.0 { p.stats.size } else { 1.0 })
    }

    pub fn player(&self, n: u8) -> Option<&Fighter> {
        self.fighters.iter().find(|f| f.kind == FighterKind::Player(n))
    }
    pub fn player_id(&self, n: u8) -> Option<u32> {
        self.player(n).map(|f| f.id)
    }

    fn emit(&mut self, e: SimEvent) {
        self.events.push(e);
    }

    // ------------------------------------------------------------------ main step

    pub fn step(&mut self, content: &Content, cmds: &[Command]) {
        self.tick += 1;
        self.stats.ticks += 1;
        let t = content.tuning().clone();
        // Fix up allies spawned via the bell.
        for f in self.fighters.iter_mut() {
            if let FighterKind::Boss(id) = f.kind.clone() {
                if self.cfg.bell && id == "boss_st_vesper" && self.cfg.side == Side::Saint && f.team == Team::Enemy {
                    f.team = Team::Ally;
                    f.kind = FighterKind::Ally(id.clone());
                    if let Some(b) = f.ai.as_mut() {
                        b.aggro = 80.0;
                        b.boss = Some(id);
                    }
                }
            }
        }
        self.record_history(content);
        self.weather_tick(content);

        let n = self.fighters.len();
        let mut commands = vec![Command::default(); n];
        let mut scripts: Vec<Option<usize>> = vec![None; n];
        for i in 0..n {
            if !self.fighters[i].alive || matches!(self.fighters[i].act, Act::Dead) {
                continue;
            }
            match self.fighters[i].kind {
                FighterKind::Player(p) => {
                    let mut c = cmds.get(p as usize).copied().unwrap_or_default();
                    // Low sync / jittery servos: controls drift.
                    let drift = self.fighters[i].build.fx.drift;
                    if drift > 0.0 && c.move_dir.length_squared() > 0.01 {
                        let a = (self.tick as f32 * 0.021).sin() * drift;
                        c.move_dir = Vec2::from_angle(a).rotate(c.move_dir);
                    }
                    commands[i] = c;
                }
                _ => {
                    let (c, s) = self.ai_think(content, i);
                    commands[i] = c;
                    scripts[i] = s;
                }
            }
        }
        for i in 0..n {
            self.control(content, i, commands[i], scripts[i], &t);
        }
        for i in 0..n {
            self.advance(content, i, &t);
        }
        for i in 0..n {
            self.movement(content, i, commands[i]);
        }
        self.separate();
        self.update_projectiles(content);
        self.update_hazards();
        for i in 0..n {
            self.status_tick(content, i, &t);
        }
        self.boss_tick(content);
        self.pickup_tick();
        self.hallucinations(content);
        // Despawn corpses.
        for f in self.fighters.iter_mut() {
            f.lifetime += 1;
            if matches!(f.act, Act::Dead) && !f.kind.is_player() {
                f.despawn += 1;
                if f.despawn > 240 {
                    f.alive = false;
                }
            }
            if f.hallucination && (f.lifetime > 600 || matches!(f.act, Act::Dead)) {
                f.alive = false;
            }
            if f.status.hit_flash > 0 {
                f.status.hit_flash -= 1;
            }
        }
    }

    fn record_history(&mut self, content: &Content) {
        while self.history.len() < self.fighters.len() {
            self.history.push(VecDeque::new());
        }
        for (i, f) in self.fighters.iter().enumerate() {
            let attack = match &f.act {
                Act::Attack { mv, skip, .. } => content.mv(&mv.id).map(|m| {
                    (m.id.clone(), f.act_t, m.windup.saturating_sub(*skip), m.feint, m.active, m.reach * f.size() + m.radius * f.size())
                }),
                _ => None,
            };
            let s = Seen {
                id: f.id,
                pos: f.pos,
                yaw: f.yaw,
                radius: f.radius(),
                hp_frac: f.hp_frac(),
                attack,
                dodging: matches!(f.act, Act::Dodge { .. }),
                healing: matches!(f.act, Act::Heal),
                broken: matches!(f.act, Act::PostureBroken),
                act_start: f.act_start,
            };
            let h = &mut self.history[i];
            h.push_back(s);
            if h.len() > HISTORY {
                h.pop_front();
            }
        }
    }

    /// What `who` looked like `delay` ticks ago.
    pub fn observe(&self, who: u32, delay: u16) -> Option<&Seen> {
        let h = self.history.get(who as usize)?;
        let idx = h.len().checked_sub(1 + delay as usize).unwrap_or(0);
        h.get(idx)
    }

    fn weather_tick(&mut self, _content: &Content) {
        if self.reveal > 0 {
            self.reveal -= 1;
        }
        if self.cfg.tide.weather == Weather::Storm && self.rng.chance(1.0 / 420.0) {
            let h = self.city.half();
            let p = Vec3::new(self.rng.range(-h, h), 0.0, self.rng.range(-h, h));
            self.reveal = 50;
            self.emit(SimEvent::Lightning { pos: p });
        }
    }

    fn ai_think(&mut self, content: &Content, i: usize) -> (Command, Option<usize>) {
        let me = &self.fighters[i];
        let Some(brain) = me.ai.clone() else { return (Command::default(), None) };
        // Target: nearest hostile (current positions are fine for *choosing*; reactions use delay).
        let mut best: Option<(f32, u32)> = None;
        for o in &self.fighters {
            if o.alive && !matches!(o.act, Act::Dead) && me.team.hostile(o.team) && !o.hallucination {
                // Lure: Goliath Moth prefers fires/lit targets; we just bias by distance.
                let d = o.pos.distance(me.pos);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, o.id));
                }
            }
        }
        let mut brain = brain;
        brain.target = best.map(|(_, id)| id);
        let seen = brain.target.and_then(|t| self.observe(t, brain.reaction)).cloned();
        let reach = me.build.moves.light.as_ref().and_then(|m| content.mv(&m.id)).map_or(3.0, |m| m.reach + m.radius * 0.5) * me.size();
        let (boss_moves, boss_speed): (Vec<(String, u32)>, f32) = match &brain.boss {
            Some(b) => {
                content.boss(b).and_then(|bd| bd.phases.get(me.boss_phase)).map(|ph| (ph.moves.clone(), ph.speed)).unwrap_or_default()
            }
            None => (vec![], 1.0),
        };
        let p = Percept {
            tick: self.tick,
            me_pos: me.pos,
            me_yaw: me.yaw,
            me_hp: me.hp_frac(),
            me_reach: reach,
            me_busy: me.act.busy(),
            me_can_act: me.stamina.can_act() || me.kind.is_bossish(),
            me_heals: me.heal_charges,
            me_flying: me.build.flying,
            target: seen.as_ref(),
            boss_moves: &boss_moves,
            boss_speed: if boss_speed > 0.0 { boss_speed } else { 1.0 },
            has_special: me.build.moves.special.is_some(),
            has_bash: me.build.moves.bash.is_some(),
            can_parry: me.build.can_parry,
        };
        let mut rng = self.rng.fork(i as u64 ^ self.tick);
        let out = ai::think(&mut brain, &p, &mut rng);
        self.fighters[i].ai = Some(brain);
        (out.cmd, out.script)
    }

    // ------------------------------------------------------------------ control

    fn can_start(&self, i: usize) -> bool {
        let f = &self.fighters[i];
        match &f.act {
            Act::Idle | Act::Guard => true,
            Act::Attack { mv, skip, .. } => {
                // Combo cancel window: last 45% of recovery.
                // (Checked by caller through buffer; here only idle/guard.)
                let _ = (mv, skip);
                false
            }
            _ => false,
        }
    }

    fn control(&mut self, content: &Content, i: usize, c: Command, script: Option<usize>, t: &TuningDef) {
        if !self.fighters[i].alive || matches!(self.fighters[i].act, Act::Dead) {
            return;
        }
        // Lock-on.
        if c.lock {
            self.toggle_lock(i);
        }
        if c.lock_next || c.lock_prev {
            self.cycle_lock(i, if c.lock_next { 1.0 } else { -1.0 });
        }
        if c.lock_part {
            self.cycle_lock_part(i);
        }
        // Struggle out of a grab.
        if let Act::Grabbed { .. } = self.fighters[i].act {
            if c.dodge {
                let f = &mut self.fighters[i];
                f.status.grabbed = f.status.grabbed.saturating_sub(14);
            }
            return;
        }
        // Climbing.
        if let Act::Climbing { target, slot, .. } = self.fighters[i].act {
            if c.heavy || c.light {
                self.rip(content, i, target as usize, slot, if c.heavy { 1.0 } else { 0.45 });
            }
            if c.dodge {
                self.set_act(i, Act::Stagger { dur: 20 });
                self.emit(SimEvent::Thrown { id: i as u32 });
            }
            return;
        }
        // Buffer inputs so presses during recovery chain smoothly.
        let press = if c.heretic {
            Some(Buffered::Heretic)
        } else if c.dodge {
            Some(Buffered::Dodge)
        } else if c.parry {
            Some(Buffered::Parry)
        } else if c.heal {
            Some(Buffered::Heal)
        } else if c.special {
            Some(Buffered::Special)
        } else if c.light && c.guard {
            Some(Buffered::Bash)
        } else if c.heavy {
            Some(Buffered::Heavy)
        } else if c.light {
            Some(Buffered::Light)
        } else {
            None
        };
        if let Some(p) = press {
            self.fighters[i].buffered = Some((p, 10));
        }
        if let Some(sidx) = script {
            // Scripted boss move.
            let f = &self.fighters[i];
            if self.can_start(i) {
                if let Some(b) = f.ai.as_ref().and_then(|b| b.boss.clone()) {
                    if let Some(ph) = content.boss(&b).and_then(|bd| bd.phases.get(f.boss_phase)) {
                        if let Some((mid, _)) = ph.moves.get(sidx) {
                            let mv = MoveRef { id: mid.clone(), slot: Slot::Frame };
                            self.start_attack(content, i, mv, true);
                            return;
                        }
                    }
                }
            }
        }
        // Combo window: allow cancelling late recovery into the next light/heavy/dodge.
        let in_cancel = match &self.fighters[i].act {
            Act::Attack { mv, skip, .. } => content.mv(&mv.id).is_some_and(|m| {
                let tot = (m.windup + m.feint + m.active + m.recovery).saturating_sub(*skip) as u32;
                self.fighters[i].act_t as f32 > tot as f32 - m.recovery as f32 * 0.45
            }),
            _ => false,
        };
        let ready = self.can_start(i) || in_cancel;
        if let Some((b, ttl)) = self.fighters[i].buffered {
            if ready {
                let used = self.try_action(content, i, b, c, t);
                if used {
                    self.fighters[i].buffered = None;
                }
            }
            if let Some((_, ttl2)) = self.fighters[i].buffered.as_mut() {
                *ttl2 = ttl.saturating_sub(1);
                if *ttl2 == 0 {
                    self.fighters[i].buffered = None;
                }
            }
        }
        // Guard (held).
        let f = &mut self.fighters[i];
        match f.act {
            Act::Idle if c.guard && !c.light => {
                f.act = Act::Guard;
                f.act_t = 0;
            }
            Act::Guard if !c.guard => {
                f.act = Act::Idle;
                f.act_t = 0;
            }
            _ => {}
        }
        // Interact.
        if c.interact && matches!(self.fighters[i].act, Act::Idle | Act::Guard) {
            self.interact(content, i);
        }
    }

    fn try_action(&mut self, content: &Content, i: usize, b: Buffered, c: Command, t: &TuningDef) -> bool {
        let f = &self.fighters[i];
        let can = f.stamina.can_act();
        match b {
            Buffered::Dodge => {
                if !can && f.stamina.kind != StaminaKind::Heat {
                    return false;
                }
                let class = f.build.class;
                let kind = match class {
                    WeightClass::Light => DodgeKind::Dash,
                    WeightClass::Medium => DodgeKind::Hop,
                    WeightClass::Heavy => DodgeKind::Slide,
                };
                let ci = class.index();
                let mut dir =
                    if c.move_dir.length_squared() > 0.04 { c.move_dir.normalize() } else { -Vec2::new(f.yaw.sin(), f.yaw.cos()) };
                if f.build.overloaded {
                    dir *= 0.6;
                }
                // Label relative to facing (for quirks and habit recording).
                let fwd = Vec2::new(f.yaw.sin(), f.yaw.cos());
                let right = Vec2::new(fwd.y, -fwd.x);
                let label = if dir.dot(fwd) < -0.5 {
                    DodgeDir::Back
                } else if dir.dot(fwd) > 0.5 {
                    DodgeDir::Forward
                } else if dir.dot(right) < 0.0 {
                    DodgeDir::Left
                } else {
                    DodgeDir::Right
                };
                let bonus = f.build.fx.dodge_bonus(label);
                let iframes = (t.dodge_iframes[ci] as i16 + bonus).max(4) as u16;
                let dist = t.dodge_distance[ci] * f.size().sqrt();
                let ticks = t.dodge_ticks[ci];
                let cost = 14.0 + f.build.load * 10.0;
                let fm = &mut self.fighters[i];
                fm.stamina.spend(cost);
                self.set_act(i, Act::Dodge { dir, kind, iframes, dist, ticks, label });
                self.emit(SimEvent::Dodge { id: i as u32, dir: label });
                true
            }
            Buffered::Parry => {
                if !f.build.can_parry {
                    // No parrying part: falls back to a guard.
                    self.fighters[i].act = Act::Guard;
                    return true;
                }
                self.set_act(i, Act::Parry);
                self.emit(SimEvent::ParryAttempt { id: i as u32 });
                true
            }
            Buffered::Heal => {
                let ok = match f.build.heal {
                    HealKind::RepairCharges(_) => f.heal_charges > 0,
                    HealKind::Regrowth => f.heal_charges > 0 || f.currency >= t.regrowth_cost,
                };
                if ok {
                    self.set_act(i, Act::Heal);
                }
                true
            }
            Buffered::Heretic => {
                if let Some(m) = f.build.moves.heretic.clone() {
                    self.start_attack(content, i, m, false);
                }
                true
            }
            Buffered::Special => {
                if let Some(m) = f.build.moves.special.clone() {
                    if can {
                        self.start_attack(content, i, m, false);
                    }
                }
                true
            }
            Buffered::Bash => {
                if let Some(m) = f.build.moves.bash.clone() {
                    if can {
                        self.start_attack(content, i, m, false);
                    }
                }
                true
            }
            Buffered::Heavy => {
                // Sever / execute a posture-broken, locked target.
                if let Some(target) = self.sever_target(i) {
                    let slot = self.fighters[i].lock.filter(|l| l.target == target).map(|l| l.slot);
                    let tf = &self.fighters[target as usize];
                    let slot = slot
                        .filter(|s| s.severable() && tf.part_alive(*s))
                        .or_else(|| tf.alive_parts().into_iter().find(|s| s.severable()))
                        .unwrap_or(Slot::Frame);
                    self.set_act(i, Act::Severing { target, slot });
                    return true;
                }
                if let Some(m) = f.build.moves.heavy.clone() {
                    if can {
                        self.start_attack(content, i, m, false);
                    }
                }
                true
            }
            Buffered::Light => {
                let m = f.mimic.clone().or_else(|| f.build.moves.light.clone());
                // Hunger: devour a nearby building when standing next to one and not locked.
                if let Some(eat) = f.build.moves.eat.clone() {
                    if f.lock.is_none() && self.nearest_chunk(i, 6.0).is_some() && f.stamina.food < 0.95 {
                        self.start_attack(content, i, eat, false);
                        return true;
                    }
                }
                if let Some(m) = m {
                    if can {
                        self.start_attack(content, i, m, false);
                    }
                }
                true
            }
        }
    }

    fn sever_target(&self, i: usize) -> Option<u32> {
        let f = &self.fighters[i];
        let cand: Vec<u32> = match f.lock {
            Some(l) => vec![l.target],
            None => self.fighters.iter().filter(|o| f.team.hostile(o.team)).map(|o| o.id).collect(),
        };
        for id in cand {
            let o = &self.fighters[id as usize];
            if o.alive
                && matches!(o.act, Act::PostureBroken)
                && !o.hallucination
                && o.pos.distance(f.pos) < f.radius() + o.radius() + 7.0 * f.size()
            {
                return Some(id);
            }
        }
        None
    }

    fn set_act(&mut self, i: usize, a: Act) {
        let tick = self.tick;
        let f = &mut self.fighters[i];
        f.act = a;
        f.act_t = 0;
        f.anim_acc = 0.0;
        f.act_start = tick;
    }

    pub fn start_attack(&mut self, content: &Content, i: usize, mv: MoveRef, scripted: bool) {
        let Some(m) = content.mv(&mv.id) else { return };
        // Echo omen: enemies copy the player's last move.
        let mut mv = mv;
        if self.enemy_fx.echo && self.fighters[i].team == Team::Enemy && !scripted && m.kind == MoveKind::Light {
            if let Some(last) = self.player(0).and_then(|p| p.last_move.clone()) {
                if content.mv(&last).is_some_and(|x| x.windup >= 10) {
                    mv = MoveRef { id: last, slot: mv.slot };
                }
            }
        }
        let m = content.mv(&mv.id).unwrap();
        let f = &mut self.fighters[i];
        if !scripted && !f.kind.is_bossish() {
            f.stamina.spend(m.stamina);
        }
        let mut skip = 0;
        if f.build.fx.early_fire > 0.0 && self.rng.chance(f.build.fx.early_fire) {
            skip = (m.windup as f32 * f.build.fx.early_skip) as u16;
        }
        f.last_move = Some(mv.id.clone());
        let boss = f.kind.is_bossish();
        let pos = f.pos;
        let burrow = m.effect == Some(MoveEffect::Burrow);
        self.set_act(i, Act::Attack { mv: mv.clone(), hit: vec![], skip, chunks_hit: false });
        if burrow {
            self.fighters[i].burrowed = true;
        }
        self.emit(SimEvent::AttackStart { id: i as u32, mv: mv.id.clone(), cue: m.cue, telegraph: m.telegraph, feint: m.feint, pos, boss });
    }

    // ------------------------------------------------------------------ lock-on

    fn lock_candidates(&self, i: usize) -> Vec<(f32, u32)> {
        let f = &self.fighters[i];
        let range = f.build.lock_range * 1.6;
        let fwd = f.forward();
        let mut v: Vec<(f32, u32)> = self
            .fighters
            .iter()
            .filter(|o| o.alive && !matches!(o.act, Act::Dead) && f.team.hostile(o.team) && o.pos.distance(f.pos) < range)
            .filter(|o| !o.burrowed || self.reveal > 0)
            .map(|o| {
                let d = (o.pos - f.pos).normalize_or_zero();
                let ang = geom::angle_diff(geom::dir_yaw(fwd), geom::dir_yaw(d));
                (ang, o.id)
            })
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v
    }

    fn toggle_lock(&mut self, i: usize) {
        if self.fighters[i].build.fx.no_lock {
            return;
        }
        if self.fighters[i].lock.is_some() {
            self.fighters[i].lock = None;
            self.emit(SimEvent::LockOn { id: i as u32, target: None });
            return;
        }
        let c = self.lock_candidates(i);
        if let Some((_, id)) = c.iter().min_by(|a, b| a.0.abs().total_cmp(&b.0.abs())) {
            let slot = Slot::Frame;
            self.fighters[i].lock = Some(LockOn { target: *id, slot });
            self.emit(SimEvent::LockOn { id: i as u32, target: Some(*id) });
        }
    }

    fn cycle_lock(&mut self, i: usize, dir: f32) {
        let Some(cur) = self.fighters[i].lock else { return };
        let c = self.lock_candidates(i);
        if c.len() < 2 {
            return;
        }
        let idx = c.iter().position(|x| x.1 == cur.target).unwrap_or(0) as i32;
        let n = c.len() as i32;
        let next = ((idx + if dir > 0.0 { 1 } else { -1 }) % n + n) % n;
        self.fighters[i].lock = Some(LockOn { target: c[next as usize].1, slot: Slot::Frame });
    }

    fn cycle_lock_part(&mut self, i: usize) {
        let Some(cur) = self.fighters[i].lock else { return };
        let parts = self.fighters[cur.target as usize].alive_parts();
        if parts.is_empty() {
            return;
        }
        let idx = parts.iter().position(|s| *s == cur.slot).map_or(0, |p| (p + 1) % parts.len());
        self.fighters[i].lock = Some(LockOn { target: cur.target, slot: parts[idx] });
    }

    // ------------------------------------------------------------------ interact

    fn interact(&mut self, content: &Content, i: usize) {
        let f = &self.fighters[i];
        if !f.kind.is_player() {
            return;
        }
        let side = f.loadout.side;
        let pos = f.pos;
        // Rest at own-side checkpoint.
        if let Some((ci, _)) = self.city.checkpoints.iter().enumerate().find(|(_, c)| c.kind.side() == side && c.pos.distance(pos) < 14.0) {
            self.rest(content, ci);
            return;
        }
        // Wrecks.
        if let Some(wi) = self.wrecks.iter().position(|w| w.pos.distance(pos) < 12.0 && w.district == self.cfg.district) {
            let w = self.wrecks[wi].clone();
            if w.side == side && w.currency > 0 {
                self.fighters[i].currency += w.currency;
                self.wrecks[wi].currency = 0;
                self.emit(SimEvent::WreckRecovered { amount: w.currency });
                return;
            } else if w.side != side && !w.harvested {
                // Strip your own remains from the other side: a failed run always pays off.
                let mut names = vec![];
                let mut cands: Vec<&PartInstance> = w.loadout.slots.iter().flatten().collect();
                let n = 2.min(cands.len());
                for _ in 0..n {
                    let k = self.rng.below(cands.len() as u32) as usize;
                    let p = cands.remove(k);
                    if let Some(d) = content.part(&p.def) {
                        let cond = if self.rng.chance(0.5) { Condition::Scarred } else { Condition::Mangled };
                        let uid = self.uid();
                        let mut inst =
                            PartInstance::harvest(d, cond, Origin::OwnRemains { war: w.war, tide: w.tide }, uid, &mut self.rng, content);
                        inst.memory = p.memory.clone();
                        names.push(d.name.clone());
                        self.stats.harvested.push((w.loadout.chassis.clone().unwrap_or_default(), d.slot));
                        self.collected.push(inst);
                    }
                }
                self.wrecks[wi].harvested = true;
                self.emit(SimEvent::WreckHarvested { parts: names });
                return;
            }
        }
        // Climb a huge boss.
        for o in 0..self.fighters.len() {
            if o == i {
                continue;
            }
            let of = &self.fighters[o];
            let huge = matches!(&of.kind, FighterKind::Boss(b) if content.boss(b).is_some_and(|bd| bd.huge));
            if huge && of.alive && f.team.hostile(of.team) && of.pos.distance(pos) < of.radius() + f.radius() + 10.0 {
                let vulnerable = matches!(of.act, Act::PostureBroken | Act::Stagger { .. })
                    || matches!(&of.act, Act::Attack { mv, .. } if content.mv(&mv.id).is_some_and(|m| of.act_t > (m.windup + m.feint + m.active) as u32));
                if vulnerable {
                    let bd = content.boss(match &of.kind {
                        FighterKind::Boss(b) => b,
                        _ => unreachable!(),
                    });
                    let wp = bd.and_then(|b| b.weak_points.iter().find(|w| of.part_alive(w.slot)).cloned());
                    if let Some(wp) = wp {
                        self.set_act(i, Act::Climbing { target: o as u32, slot: wp.slot, height: wp.height });
                        self.emit(SimEvent::Climb { id: i as u32, target: o as u32 });
                        return;
                    }
                }
            }
        }
    }

    pub fn rest(&mut self, content: &Content, ci: usize) {
        self.last_checkpoint = ci;
        let ids: Vec<usize> = self.fighters.iter().filter(|f| f.kind.is_player()).map(|f| f.id as usize).collect();
        for i in ids {
            self.restore_player(content, i);
        }
        self.respawn_enemies(content);
        self.emit(SimEvent::Rest { checkpoint: ci });
    }

    fn restore_player(&mut self, content: &Content, i: usize) {
        let f = &mut self.fighters[i];
        // Repair/regrow destroyed parts. Machine-blooded (heretic) parts on a Kaiju cannot regrow.
        let mut destroyed = [false; 7];
        for s in Slot::ALL {
            let heretic = f.build.parts[s.index()].as_ref().is_some_and(|p| p.heretic)
                || (f.destroyed[s.index()]
                    && f.loadout.get(s).and_then(|p| content.part(&p.def)).is_some_and(|d| d.side != f.loadout.side));
            if f.destroyed[s.index()] && f.loadout.side == Side::Kaiju && heretic {
                destroyed[s.index()] = true;
            }
            if f.loadout.get(s).is_none() {
                destroyed[s.index()] = true;
            }
        }
        f.destroyed = destroyed;
        f.build = build::resolve(&f.loadout, content, &f.extra_fx, f.destroyed);
        for s in Slot::ALL {
            f.part_hp[s.index()] = f.build.parts[s.index()].as_ref().map_or(0.0, |p| p.hp);
        }
        f.hp = f.build.hp_max;
        f.posture = 0.0;
        f.heal_charges = f.heal_max;
        f.stamina.refill();
        f.status = Status::default();
        f.act = Act::Idle;
        f.act_t = 0;
    }

    fn respawn_enemies(&mut self, content: &Content) {
        // Remove dead/alive minor enemies and re-create from the respawn list.
        for f in self.fighters.iter_mut() {
            if matches!(f.kind, FighterKind::Enemy(_) | FighterKind::Verse | FighterKind::Hallucination) {
                f.alive = false;
                f.act = Act::Dead;
                f.lock = None;
            }
        }
        let alive: Vec<bool> = self.fighters.iter().map(|f| f.alive).collect();
        for f in self.fighters.iter_mut() {
            if f.lock.is_some_and(|l| !alive.get(l.target as usize).copied().unwrap_or(false)) {
                f.lock = None;
            }
        }
        let list = self.respawn_list.clone();
        for (s, p) in list {
            if matches!(s, SpawnSpec::Enemy(_)) {
                self.spawn_spec(content, s, p);
            }
        }
    }

    pub fn respawn_player(&mut self, content: &Content, n: u8) {
        let Some(id) = self.player_id(n) else { return };
        let cp = self.city.checkpoints[self.last_checkpoint].pos;
        let i = id as usize;
        self.fighters[i].alive = true;
        self.restore_player(content, i);
        self.fighters[i].pos = cp + Vec3::new(0.0, 0.0, -8.0);
        self.fighters[i].vel = Vec3::ZERO;
        self.fighters[i].lock = None;
        self.respawn_enemies(content);
        // Boss resets.
        if let Some(b) = self.boss {
            let bf = &mut self.fighters[b as usize];
            if bf.alive && !matches!(bf.act, Act::Dead) {
                bf.hp = bf.build.hp_max;
                bf.posture = 0.0;
                bf.pos = bf.spawn_pos;
                bf.act = Act::Idle;
                bf.boss_phase = 0;
                self.boss_awake = false;
            }
        }
    }

    fn nearest_chunk(&self, i: usize, reach: f32) -> Option<(usize, usize)> {
        let f = &self.fighters[i];
        let c = f.pos + f.forward() * (f.radius() + reach * 0.5) + Vec3::Y * 3.0;
        let r = reach.max(3.0);
        let mut best: Option<(f32, usize, usize)> = None;
        for (bi, b) in self.city.buildings.iter().enumerate() {
            if !b.aabb.expand(r + 2.0).contains(Vec3::new(c.x, b.aabb.min.y + 0.5, c.z)) {
                continue;
            }
            for ci in 0..b.alive.len() {
                if b.alive[ci] {
                    let a = b.chunk_aabb(ci);
                    let d = a.closest(c).distance(c);
                    if d < r && best.is_none_or(|(bd, _, _)| d < bd) {
                        best = Some((d, bi, ci));
                    }
                }
            }
        }
        best.map(|(_, b, c)| (b, c))
    }

    // ------------------------------------------------------------------ advance actions

    fn advance(&mut self, content: &Content, i: usize, t: &TuningDef) {
        if !self.fighters[i].alive {
            return;
        }
        // Animation speed via accumulator (Old Bones, AttackSpeed).
        let steps = {
            let f = &mut self.fighters[i];
            let sp = if matches!(f.act, Act::Attack { .. }) { f.anim_speed } else { 1.0 };
            f.anim_acc += sp;
            let s = f.anim_acc.floor() as u32;
            f.anim_acc -= s as f32;
            s
        };
        for _ in 0..steps {
            self.fighters[i].act_t += 1;
            self.advance_one(content, i, t);
        }
    }

    fn advance_one(&mut self, content: &Content, i: usize, t: &TuningDef) {
        let act = self.fighters[i].act.clone();
        let at = self.fighters[i].act_t;
        match act {
            Act::Attack { mv, skip, .. } => {
                let Some(m) = content.mv(&mv.id) else {
                    self.set_act(i, Act::Idle);
                    return;
                };
                let w = (m.windup + m.feint).saturating_sub(skip) as u32;
                let a = m.active as u32;
                let total = w + a + m.recovery as u32;
                if at == w {
                    self.on_active_start(content, i, m, &mv);
                }
                if at >= w && at < w + a {
                    self.attack_hits(content, i, m, &mv, at - w);
                }
                if at == w + a && m.effect == Some(MoveEffect::Burrow) {
                    self.fighters[i].burrowed = false;
                }
                if at >= total {
                    let f = &mut self.fighters[i];
                    f.combo = f.combo.wrapping_add(1);
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Dodge { ticks, .. } => {
                if at >= ticks as u32 {
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Parry => {
                if at >= 30 {
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Stagger { dur } => {
                if at >= dur as u32 {
                    self.set_act(i, Act::Idle);
                }
            }
            Act::PostureBroken => {
                if at >= t.posture_break_ticks as u32 {
                    self.fighters[i].posture = 0.0;
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Vent => {
                if at >= t.vent_ticks as u32 {
                    self.fighters[i].stamina.vent();
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Heal => {
                let dur = match self.fighters[i].build.heal {
                    HealKind::RepairCharges(_) => t.heal_ticks as u32,
                    HealKind::Regrowth => t.heal_ticks as u32 * 3 / 2,
                };
                if at == dur / 2 {
                    self.apply_heal(content, i, t);
                }
                if at >= dur {
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Severing { target, slot } => {
                if at == 1 {
                    self.emit(SimEvent::Shake { amount: 0.6, pos: self.fighters[i].pos });
                }
                if at >= 40 {
                    self.do_sever(content, i, target as usize, slot);
                    self.set_act(i, Act::Idle);
                }
            }
            Act::Climbing { target, .. } => {
                let tf = &self.fighters[target as usize];
                if !tf.alive || matches!(tf.act, Act::Dead) {
                    self.set_act(i, Act::Idle);
                    return;
                }
                // The colossus shakes you; stamina is your grip.
                if at % 80 == 79 {
                    let p = self.fighters[target as usize].pos;
                    self.emit(SimEvent::Shake { amount: 0.8, pos: p });
                    let f = &mut self.fighters[i];
                    f.stamina.value -= 28.0;
                    f.stamina.since_spend = 0.0;
                    if f.stamina.value <= 0.0 {
                        f.stamina.value = 0.0;
                        self.set_act(i, Act::Stagger { dur: 40 });
                        self.emit(SimEvent::Thrown { id: i as u32 });
                    }
                }
            }
            Act::Grabbed { by } => {
                let f = &mut self.fighters[i];
                if f.status.grabbed == 0 || !self.fighters[by as usize].alive {
                    self.set_act(i, Act::Stagger { dur: 12 });
                } else {
                    let f = &mut self.fighters[i];
                    f.status.grabbed -= 1;
                    let dps = f.build.hp_max * 0.05 / 60.0;
                    f.hp -= dps;
                    if f.hp <= 0.0 {
                        self.kill(content, i, Some(by as usize));
                    }
                }
            }
            Act::Idle | Act::Guard | Act::Dead => {}
        }
    }

    fn apply_heal(&mut self, content: &Content, i: usize, t: &TuningDef) {
        let f = &mut self.fighters[i];
        let mul = f.build.fx.heal_mul * f.build.fx.get(Stat::HealPower);
        let before = f.hp_frac();
        match f.build.heal {
            HealKind::RepairCharges(_) => {
                if f.heal_charges == 0 {
                    return;
                }
                f.heal_charges -= 1;
                f.hp = (f.hp + f.build.hp_max * t.repair_heal * mul).min(f.build.hp_max);
            }
            HealKind::Regrowth => {
                if f.heal_charges > 0 {
                    f.heal_charges -= 1;
                } else if f.currency >= t.regrowth_cost {
                    f.currency -= t.regrowth_cost;
                } else {
                    return;
                }
                f.hp = (f.hp + f.build.hp_max * t.regrowth_heal * mul).min(f.build.hp_max);
                // Regrowth also regrows one severed/destroyed organic part.
                if !f.build.fx.no_regrow {
                    let side = f.loadout.side;
                    if let Some(s) = Slot::ALL.into_iter().find(|s| {
                        f.destroyed[s.index()] && f.loadout.get(*s).and_then(|p| content.part(&p.def)).is_some_and(|d| d.side == side)
                    }) {
                        f.destroyed[s.index()] = false;
                        f.build = build::resolve(&f.loadout, content, &f.extra_fx, f.destroyed);
                        f.part_hp[s.index()] = f.build.parts[s.index()].as_ref().map_or(0.0, |p| p.hp * 0.5);
                    }
                }
            }
        }
        if f.kind.is_player() {
            self.stats.heals += 1;
        }
        self.emit(SimEvent::Heal { id: i as u32, hp_frac: before });
    }

    fn on_active_start(&mut self, content: &Content, i: usize, m: &MoveDef, mv: &MoveRef) {
        let f = &self.fighters[i];
        let size = f.size();
        let pos = f.center();
        let fwd = f.forward();
        let team = f.team;
        let target = f.lock.map(|l| l.target).or_else(|| f.ai.as_ref().and_then(|a| a.target));
        let hallucination = f.hallucination;
        if let Some(p) = &m.projectile {
            for k in 0..p.count {
                let spread = if p.count > 1 { (k as f32 / (p.count - 1) as f32 - 0.5) * p.spread.to_radians() } else { 0.0 };
                let mut dir = geom::yaw_dir(f.yaw + spread);
                // Aim at the target if it is in front.
                if let Some(tid) = target {
                    let tp = self.fighters[tid as usize].center();
                    let d = (tp - pos).normalize_or_zero();
                    if d.dot(fwd) > 0.3 {
                        let flat = Vec3::new(d.x, 0.0, d.z).normalize_or_zero();
                        dir = (Vec2::from_angle(spread).rotate(Vec2::new(flat.x, flat.z))).extend(0.0);
                        dir = Vec3::new(dir.x, 0.0, dir.y);
                        let up = if p.gravity > 0.0 {
                            let dist = tp.distance(pos);
                            // Lob: aim so the arc lands near the target.
                            let tt = dist / p.speed;
                            (0.5 * p.gravity * tt) / p.speed
                        } else {
                            d.y
                        };
                        dir = (dir + Vec3::Y * up).normalize_or_zero();
                    }
                }
                self.projectiles.push(Projectile {
                    owner: i as u32,
                    team,
                    pos: pos + fwd * (f.radius() + 1.0),
                    vel: dir * p.speed,
                    radius: p.radius * size.sqrt(),
                    life: p.life,
                    gravity: p.gravity,
                    homing: p.homing,
                    target,
                    mv: m.id.clone(),
                    slot: mv.slot,
                    hallucination,
                });
            }
            self.emit(SimEvent::ProjectileFired { id: i as u32, pos });
        }
        match m.effect {
            Some(MoveEffect::Split) => {
                // Split off verses (Pale Choir / Brood).
                let n = 3;
                let f = self.fighters[i].clone();
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU;
                    let p = f.pos + Vec3::new(a.sin(), 0.0, a.cos()) * (f.radius() + 6.0);
                    let lo = f.loadout.clone();
                    let id = self.spawn_loadout(
                        content,
                        f.team,
                        FighterKind::Verse,
                        f.tag.clone(),
                        format!("{} VERSE", f.name),
                        lo,
                        vec![],
                        p,
                        f.scale * 0.45,
                    );
                    let v = &mut self.fighters[id as usize];
                    v.build.hp_max = f.build.hp_max * 0.08;
                    v.hp = v.build.hp_max;
                    v.owner = Some(i as u32);
                    v.ai = Some(Brain::new(Archetype::Swarmer, 14));
                    v.currency_value = 10;
                    v.bodies = 3;
                }
                let me = &mut self.fighters[i];
                me.bodies = me.bodies.saturating_sub(3).max(3);
            }
            Some(MoveEffect::Recombine) => {
                let mut heal = 0.0;
                for v in self.fighters.iter_mut() {
                    if v.owner == Some(i as u32) && v.alive && !matches!(v.act, Act::Dead) {
                        heal += v.hp;
                        v.alive = false;
                        v.act = Act::Dead;
                    }
                }
                let me = &mut self.fighters[i];
                me.hp = (me.hp + heal).min(me.build.hp_max);
                me.bodies = 12;
            }
            Some(MoveEffect::Flood) => {
                let p = self.fighters[i].pos;
                self.hazards.push(Hazard { kind: HazardKind::Flood, pos: p, radius: 10.0 * size, life: 600, team });
                self.emit(SimEvent::Hazard { pos: p, radius: 10.0 * size, kind: HazardKind::Flood });
            }
            Some(MoveEffect::Spores) if m.kind == MoveKind::Burst => {
                let p = self.fighters[i].pos;
                self.hazards.push(Hazard { kind: HazardKind::Spores, pos: p, radius: m.radius * size, life: 300, team });
                self.emit(SimEvent::Hazard { pos: p, radius: m.radius * size, kind: HazardKind::Spores });
            }
            Some(MoveEffect::Static) if m.damage == 0.0 => {
                let p = self.fighters[i].pos;
                self.hazards.push(Hazard { kind: HazardKind::Smoke, pos: p, radius: m.radius * size * 1.5, life: 360, team });
                self.emit(SimEvent::Hazard { pos: p, radius: m.radius * size, kind: HazardKind::Smoke });
            }
            _ => {}
        }
        if m.kind == MoveKind::Burst || m.kind == MoveKind::Heretic {
            let p = self.fighters[i].pos;
            self.emit(SimEvent::Explosion { pos: p, radius: m.radius * size });
            self.emit(SimEvent::Shake { amount: 0.4 * size, pos: p });
        }
    }

    /// Hitbox of an attack at active tick `k`.
    fn hitbox(&self, i: usize, m: &MoveDef, k: u32) -> (Vec3, f32) {
        let f = &self.fighters[i];
        let size = f.size();
        let n = m.active.max(1) as f32;
        let sweep = if m.arc > 0.0 { (-0.5 + (k as f32 + 0.5) / n) * m.arc.to_radians() } else { 0.0 };
        let dir = geom::yaw_dir(f.yaw - sweep);
        let c = f.pos + Vec3::Y * (f.height() * 0.4 + f.altitude) + dir * (m.reach * size + f.radius() * 0.5);
        (c, m.radius * size)
    }

    fn attack_hits(&mut self, content: &Content, i: usize, m: &MoveDef, mv: &MoveRef, k: u32) {
        if m.kind == MoveKind::Summon || (m.damage <= 0.0 && m.posture <= 0.0 && m.kind != MoveKind::Eat) {
            return;
        }
        let (c, r) = self.hitbox(i, m, k);
        // Eat / building damage.
        if m.kind == MoveKind::Eat {
            if k == 0 {
                if let Some((bi, ci)) = self.nearest_chunk(i, m.reach * self.fighters[i].size() + 4.0) {
                    self.destroy_chunk(bi, ci, Some(i));
                }
            }
            return;
        }
        if m.projectile.is_none() {
            let team = self.fighters[i].team;
            let hit_list = match &self.fighters[i].act {
                Act::Attack { hit, .. } => hit.clone(),
                _ => vec![],
            };
            for j in 0..self.fighters.len() {
                if j == i || hit_list.contains(&(j as u32)) {
                    continue;
                }
                let o = &self.fighters[j];
                if !o.alive || matches!(o.act, Act::Dead) || !team.hostile(o.team) {
                    continue;
                }
                if o.burrowed && !matches!(o.act, Act::Attack { .. }) {
                    continue;
                }
                let cap = Capsule { base: o.pos + Vec3::Y * o.altitude, height: o.height(), r: o.radius() };
                if cap.sphere_hit(c, r) {
                    if let Act::Attack { hit, .. } = &mut self.fighters[i].act {
                        hit.push(j as u32);
                    }
                    let from = self.fighters[i].pos;
                    self.resolve_hit(content, i, j, m, mv.slot, from, 1.0);
                }
            }
            // Smash buildings (limited per swing).
            let chunks_hit = matches!(self.fighters[i].act, Act::Attack { chunks_hit: true, .. });
            if !chunks_hit && m.damage > 0.0 {
                let mut n = 0;
                let dmg = m.damage * DAMAGE_SCALE * self.fighters[i].size();
                for bi in 0..self.city.buildings.len() {
                    if !self.city.buildings[bi].aabb.sphere_hit(c, r) {
                        continue;
                    }
                    for ci in 0..self.city.buildings[bi].alive.len() {
                        if n >= 3 {
                            break;
                        }
                        let b = &mut self.city.buildings[bi];
                        if b.alive[ci] && b.chunk_aabb(ci).sphere_hit(c, r) {
                            b.chunk_hp[ci] -= dmg;
                            n += 1;
                            if b.chunk_hp[ci] <= 0.0 {
                                self.destroy_chunk(bi, ci, None);
                            }
                        }
                    }
                }
                if n > 0 {
                    if let Act::Attack { chunks_hit, .. } = &mut self.fighters[i].act {
                        *chunks_hit = true;
                    }
                }
            }
        }
    }

    fn destroy_chunk(&mut self, bi: usize, ci: usize, eater: Option<usize>) {
        let gone = self.city.buildings[bi].destroy(ci);
        for g in gone {
            let a = self.city.buildings[bi].chunk_aabb(g);
            let eaten = eater.is_some() && g == ci;
            self.emit(SimEvent::ChunkDestroyed { building: bi as u32, chunk: g as u32, pos: a.center(), eaten });
        }
        if let Some(e) = eater {
            let f = &mut self.fighters[e];
            f.stamina.on_eat(0.22);
            f.hp = (f.hp + f.build.hp_max * 0.02).min(f.build.hp_max);
            if f.kind.is_player() {
                self.stats.eaten += 1;
            }
            self.emit(SimEvent::Eat { id: e as u32, food: 0.22 });
        }
    }

    /// Pick which part a hit lands on.
    fn pick_part(&mut self, attacker: usize, target: usize, from: Vec3) -> Slot {
        let a = &self.fighters[attacker];
        let o = &self.fighters[target];
        if let Some(l) = a.lock {
            if l.target == target as u32 && o.part_alive(l.slot) && self.rng.chance(0.75) {
                return l.slot;
            }
        }
        // Geometry: which side of the target faces the attacker?
        let to = (from - o.pos).normalize_or_zero();
        let fwd = o.forward();
        let right = Vec3::new(fwd.z, 0.0, -fwd.x);
        let side = to.dot(right);
        let front = to.dot(fwd);
        let mut w = [40u32, 12, 12, 14, 0, 8, 6];
        if side < -0.3 {
            w[Slot::LeftArm.index()] += 14;
        }
        if side > 0.3 {
            w[Slot::RightArm.index()] += 14;
        }
        if front < -0.3 {
            w[Slot::Back.index()] += 16;
        }
        for s in Slot::ALL {
            if !o.part_alive(s) {
                w[s.index()] = 0;
            }
        }
        let k = self.rng.weighted(&w).unwrap_or(0);
        Slot::ALL[k]
    }

    /// Resolve one hit from `i` on `j`. `scale` scales damage (projectile splash etc.).
    fn resolve_hit(&mut self, content: &Content, i: usize, j: usize, m: &MoveDef, slot_from: Slot, from: Vec3, scale: f32) {
        let t = content.tuning();
        if self.fighters[i].hallucination {
            return; // Hallucinations cannot hurt you.
        }
        if self.fighters[j].hallucination {
            let p = self.fighters[j].pos;
            self.fighters[j].act = Act::Dead;
            self.fighters[j].alive = false;
            self.emit(SimEvent::Hallucination { id: j as u32 });
            self.emit(SimEvent::Explosion { pos: p, radius: 2.0 });
            return;
        }
        if self.fighters[j].invulnerable() {
            self.emit(SimEvent::DodgeEvaded { id: j as u32, attacker: i as u32 });
            return;
        }
        let a = &self.fighters[i];
        let o = &self.fighters[j];
        let to_att = (from - o.pos).normalize_or_zero();
        let facing = o.forward().dot(to_att) > 0.34;
        // Parry.
        let mut window = t.parry_window as i32 + o.build.fx.parry_window as i32;
        if o.kind.is_player() && self.cfg.assists.steady_hands {
            window = window.max(t.steady_hands_parry as i32);
        }
        let parried = matches!(o.act, Act::Parry) && (o.act_t as i32) <= window && m.blockable && facing;
        if parried {
            let post = m.posture * 1.5 + 25.0;
            let p = o.pos;
            if o.kind.is_player() {
                self.stats.parries += 1;
            }
            let att = &mut self.fighters[i];
            att.posture += post;
            att.posture_idle = 0;
            let broke = att.posture >= att.build.posture_max;
            self.emit(SimEvent::Parried { by: j as u32, attacker: i as u32, pos: p });
            if broke {
                self.break_posture(i);
            } else {
                self.set_act(i, Act::Stagger { dur: 34 });
            }
            return;
        }
        // Damage.
        let part_power = a.build.parts[slot_from.index()].as_ref().map_or(1.0, |p| p.power);
        let part_impact = a.build.parts[slot_from.index()].as_ref().map_or(1.0, |p| p.impact);
        let side_bonus = self.cfg.tide.side_bonus(a.loadout.side);
        let mut dmg = m.damage
            * DAMAGE_SCALE
            * part_power
            * a.dmg_mul
            * a.build.fx.get(Stat::Damage)
            * a.build.fx.desperate_mul(a.hp_frac())
            * a.build.fx.grudge_vs(&o.tag)
            * side_bonus
            * scale;
        let mut post = m.posture * part_impact * a.build.fx.get(Stat::PostureDamage) * scale;
        // Burrowed enemies and grid power.
        let heavy = matches!(m.kind, MoveKind::Heavy | MoveKind::Burst | MoveKind::Heretic | MoveKind::Grab);
        let slot = self.pick_part(i, j, from);
        let o = &self.fighters[j];
        let armor =
            if slot == Slot::Frame { o.build.armor } else { o.build.parts[slot.index()].as_ref().map_or(o.build.armor, |p| p.armor) };
        dmg *= 100.0 / (100.0 + armor);
        let guarding = matches!(o.act, Act::Guard) && facing && m.blockable;
        let mut blocked = false;
        if guarding {
            blocked = true;
            let g = o.build.guard;
            let chip = dmg * (1.0 - g);
            let stam = dmg * 0.45;
            dmg = chip;
            post *= 0.75;
            let of = &mut self.fighters[j];
            of.stamina.spend(stam);
            if !of.stamina.can_act() && of.stamina.kind != StaminaKind::Heat {
                post *= 3.0; // guard break
            }
            self.emit(SimEvent::Guard { id: j as u32 });
        }
        let (glass, is_player_target) = {
            let o = &self.fighters[j];
            (o.build.fx.glass_hull, o.kind.is_player())
        };
        // Apply.
        {
            let of = &mut self.fighters[j];
            if !blocked {
                of.part_hp[slot.index()] -= dmg;
            }
            of.hp -= dmg * if blocked { 1.0 } else { slot.core_bleed() };
            if glass && !blocked {
                of.hp = 0.0;
            }
            of.posture += post;
            of.posture_idle = 0;
            of.status.hit_flash = 6;
            of.last_attacker = Some(i as u32);
            if is_player_target {
                self.stats.damage_taken += dmg;
            }
        }
        {
            let af = &mut self.fighters[i];
            af.stamina.on_damage_dealt(dmg);
            if af.build.fx.leech > 0.0 {
                af.hp = (af.hp + dmg * af.build.fx.leech).min(af.build.hp_max);
            }
        }
        let hp_pos = self.fighters[j].center();
        self.emit(SimEvent::Hit { attacker: i as u32, target: j as u32, slot, dmg, pos: hp_pos, blocked, heavy });
        if heavy {
            self.emit(SimEvent::Shake { amount: 0.25, pos: hp_pos });
        }
        // Effects.
        let ignite = self.fighters[i].build.fx.ignite;
        if !blocked {
            let eff = m.effect;
            let of = &mut self.fighters[j];
            match eff {
                Some(MoveEffect::Burn) => of.status.burn = 240,
                Some(MoveEffect::Spores) => of.status.spores = 240,
                Some(MoveEffect::Tether) => of.status.tether = 180,
                Some(MoveEffect::Static) => {
                    of.status.static_ = 180;
                    of.lock = None;
                }
                _ => {}
            }
            if ignite {
                of.status.burn = of.status.burn.max(120);
            }
            if let Some(e) = eff {
                if matches!(e, MoveEffect::Burn | MoveEffect::Spores | MoveEffect::Tether | MoveEffect::Static) {
                    self.emit(SimEvent::StatusApplied { id: j as u32, effect: e });
                }
            }
            if eff == Some(MoveEffect::Constrict) && !self.fighters[j].kind.is_bossish() {
                self.set_act(j, Act::Grabbed { by: i as u32 });
                self.fighters[j].status.grabbed = 100;
                self.emit(SimEvent::Grabbed { by: i as u32, target: j as u32 });
            }
            if eff == Some(MoveEffect::Mimic) {
                // ST. NULL: copy the struck part's attack.
                let pdef = self.fighters[j].loadout.get(slot).and_then(|p| content.part(&p.def));
                if let Some(d) = pdef {
                    if let Some(mid) =
                        d.moves.iter().find(|x| content.mv(x).is_some_and(|mm| matches!(mm.kind, MoveKind::Light | MoveKind::Heavy)))
                    {
                        self.fighters[i].mimic = Some(MoveRef { id: mid.clone(), slot: Slot::RightArm });
                        self.emit(SimEvent::Mimic { id: i as u32, mv: mid.clone() });
                    }
                }
            }
        }
        // Part destruction.
        if !blocked && slot.severable() {
            let of = &self.fighters[j];
            if of.part_hp[slot.index()] <= 0.0 && !of.destroyed[slot.index()] {
                let maxhp = of.build.parts[slot.index()].as_ref().map_or(1.0, |p| p.hp);
                let overkill = -of.part_hp[slot.index()] / maxhp;
                self.destroy_part(content, j, slot, if overkill > 0.35 { Condition::Mangled } else { Condition::Scarred }, i);
            }
        }
        // Death / posture / stagger.
        if self.fighters[j].hp <= 0.0 {
            self.kill(content, j, Some(i));
            return;
        }
        let of = &self.fighters[j];
        if of.posture >= of.build.posture_max && !matches!(of.act, Act::PostureBroken) {
            self.break_posture(j);
            return;
        }
        let hyper = matches!(&of.act, Act::Attack { mv, .. } if content.mv(&mv.id).is_some_and(|mm| mm.hyper_armor))
            || matches!(of.act, Act::PostureBroken | Act::Severing { .. } | Act::Climbing { .. } | Act::Grabbed { .. });
        let flinch = if of.kind.is_bossish() {
            false
        } else if of.kind.is_player() {
            heavy || post > of.build.posture_max * 0.18
        } else {
            post * 2.0 + dmg > of.build.posture_max * 0.25
        };
        if !hyper && flinch && !blocked {
            let dur = (12.0 + post * 0.4).min(36.0) as u16;
            self.set_act(j, Act::Stagger { dur });
            let push = (self.fighters[j].pos - from).normalize_or_zero() * (2.0 + post * 0.05);
            self.fighters[j].vel += Vec3::new(push.x, 0.0, push.z) * 4.0;
            // Grabs release when the grabber is hit.
        }
        // Being hit while constricting releases the victim.
        let jid = j as u32;
        for k in 0..self.fighters.len() {
            if matches!(self.fighters[k].act, Act::Grabbed { by } if by == jid) && heavy {
                self.fighters[k].status.grabbed = 0;
            }
        }
    }

    fn break_posture(&mut self, j: usize) {
        let p = self.fighters[j].pos;
        let f = &mut self.fighters[j];
        f.posture = f.build.posture_max;
        // Climbers get thrown when their perch breaks.
        self.set_act(j, Act::PostureBroken);
        self.emit(SimEvent::PostureBroken { id: j as u32, pos: p });
    }

    fn destroy_part(&mut self, content: &Content, j: usize, slot: Slot, cond: Condition, by: usize) {
        let f = &mut self.fighters[j];
        f.destroyed[slot.index()] = true;
        f.part_hp[slot.index()] = 0.0;
        f.build = build::resolve(&f.loadout, content, &f.extra_fx, f.destroyed);
        let pos = f.center();
        let hp_max_scaled = f.build.hp_max;
        let _ = hp_max_scaled;
        self.emit(SimEvent::PartDestroyed { id: j as u32, slot, pos });
        let by_player = self.fighters[by].kind.is_player();
        if by_player {
            self.drop_part(content, j, slot, cond);
        }
        // Rescale enemy HP (resolve recomputed base HP).
        self.rescale_enemy(content, j);
    }

    fn rescale_enemy(&mut self, content: &Content, j: usize) {
        let f = &mut self.fighters[j];
        let mul = match &f.kind {
            FighterKind::Enemy(id) => content.enemy(id).map_or(1.0, |e| e.hp_mul) * self.enemy_fx.enemy_hp,
            FighterKind::Boss(id) | FighterKind::Ally(id) => content.boss(id).map_or(1.0, |b| b.hp_mul) * self.enemy_fx.enemy_hp,
            _ => 1.0,
        };
        f.build.hp_max *= mul;
        if matches!(f.kind, FighterKind::Boss(_)) {
            f.build.posture_max *= 1.0 + (mul - 1.0) * 0.45;
        }
    }

    /// Cross-Harvest: drops are the victim's own parts; the victim is always of the side opposite
    /// the player in a mission, so Saint parts only drop for Kaiju players and vice versa.
    fn drop_part(&mut self, content: &Content, j: usize, slot: Slot, cond: Condition) {
        if matches!(self.cfg.mode, Mode::Versus | Mode::Arena) {
            return;
        }
        let uid = self.uid();
        let f = &self.fighters[j];
        if f.hallucination || f.team == Team::Ally {
            return;
        }
        let Some(inst) = f.loadout.get(slot).cloned() else { return };
        let Some(def) = content.part(&inst.def) else { return };
        if def.side == self.cfg.side {
            // Same anatomy as the player: never drops (the core rule).
            return;
        }
        let origin = match &f.kind {
            FighterKind::Enemy(id) => Origin::Enemy(id.clone()),
            FighterKind::Boss(id) => Origin::Boss(f.tag.clone()).clone_with(id),
            FighterKind::Nemesis { name, .. } => Origin::Nemesis(name.clone()),
            _ => Origin::Enemy(f.tag.clone()),
        };
        let mut part = PartInstance::harvest(def, cond, origin, uid, &mut self.rng, content);
        // Nemesis drops come back with added scars.
        if matches!(f.kind, FighterKind::Nemesis { .. }) {
            part.condition = inst.condition.worse();
            part.quirks = inst.quirks.clone();
            for q in crate::parts::roll_quirks(&mut self.rng, content, def.side, Condition::Scarred, false) {
                if !part.quirks.contains(&q) {
                    part.quirks.push(q);
                }
            }
        }
        if let Some(cursed) = (f.tag.starts_with("demo_")).then_some("demo_curse") {
            part.quirks.push(cursed.into());
        }
        let pos = f.pos + Vec3::Y * 2.0;
        self.emit(SimEvent::Loot { def: def.id.clone(), condition: part.condition, pos });
        self.pickups.push(Pickup { pos, item: PickupItem::Part(part), age: 0 });
    }

    fn do_sever(&mut self, content: &Content, i: usize, target: usize, slot: Slot) {
        let tf = &self.fighters[target];
        if !tf.alive || matches!(tf.act, Act::Dead) {
            return;
        }
        let pos = tf.center();
        let tag = tf.tag.clone();
        let side = tf.loadout.side;
        if slot.severable() && tf.part_alive(slot) {
            {
                let f = &mut self.fighters[target];
                f.destroyed[slot.index()] = true;
                f.part_hp[slot.index()] = 0.0;
                f.build = build::resolve(&f.loadout, content, &f.extra_fx, f.destroyed);
            }
            self.rescale_enemy(content, target);
            if self.fighters[i].kind.is_player() {
                self.stats.severs.push((tag, slot, side));
                self.drop_part(content, target, slot, Condition::Pristine);
            }
            self.emit(SimEvent::Severed { by: i as u32, target: target as u32, slot, pos });
        }
        // A sever also deals a heavy blow.
        let f = &mut self.fighters[target];
        let dmg = f.build.hp_max * if f.kind.is_bossish() { 0.12 } else { 0.45 };
        f.hp -= dmg;
        f.posture = 0.0;
        if f.hp <= 0.0 {
            self.kill(content, target, Some(i));
        } else {
            self.set_act(target, Act::Stagger { dur: 40 });
        }
    }

    fn rip(&mut self, content: &Content, i: usize, target: usize, slot: Slot, k: f32) {
        let f = &mut self.fighters[i];
        if !f.stamina.can_act() {
            return;
        }
        f.stamina.spend(6.0);
        let tf = &mut self.fighters[target];
        let maxhp = tf.build.parts[slot.index()].as_ref().map_or(100.0, |p| p.hp);
        tf.part_hp[slot.index()] -= maxhp * 0.1 * k;
        tf.hp -= tf.build.hp_max * 0.01 * k;
        tf.posture += 6.0 * k;
        let pos = tf.center();
        self.emit(SimEvent::Hit {
            attacker: i as u32,
            target: target as u32,
            slot,
            dmg: maxhp * 0.1 * k,
            pos,
            blocked: false,
            heavy: k > 0.9,
        });
        if self.fighters[target].part_hp[slot.index()] <= 0.0 && self.fighters[target].part_alive(slot) {
            // Ripped off clean: pristine.
            let tag = self.fighters[target].tag.clone();
            let side = self.fighters[target].loadout.side;
            {
                let f = &mut self.fighters[target];
                f.destroyed[slot.index()] = true;
                f.build = build::resolve(&f.loadout, content, &f.extra_fx, f.destroyed);
            }
            self.rescale_enemy(content, target);
            self.stats.severs.push((tag, slot, side));
            self.drop_part(content, target, slot, Condition::Pristine);
            self.emit(SimEvent::Rip { id: i as u32, target: target as u32, slot });
            self.set_act(i, Act::Idle);
            let tf = &mut self.fighters[target];
            tf.hp -= tf.build.hp_max * 0.1;
        }
        if self.fighters[target].hp <= 0.0 {
            self.kill(content, target, Some(i));
            self.set_act(i, Act::Idle);
        }
    }

    fn kill(&mut self, content: &Content, j: usize, by: Option<usize>) {
        if matches!(self.fighters[j].act, Act::Dead) {
            return;
        }
        self.set_act(j, Act::Dead);
        let f = self.fighters[j].clone();
        let by_player = by.is_some_and(|b| self.fighters[b].kind.is_player() || self.fighters[b].team == Team::Ally);
        self.emit(SimEvent::Death { id: j as u32, pos: f.pos, tag: f.tag.clone(), by_player, side: f.loadout.side });
        self.emit(SimEvent::Shake { amount: 0.5 * f.size(), pos: f.pos });
        if f.kind.is_player() {
            self.player_death(j);
            return;
        }
        if f.hallucination {
            self.fighters[j].alive = false;
            return;
        }
        // Grabbed victims are freed.
        for k in 0..self.fighters.len() {
            if matches!(self.fighters[k].act, Act::Grabbed { by } if by == j as u32) {
                self.fighters[k].status.grabbed = 0;
            }
        }
        if !by_player {
            return;
        }
        self.stats.kills += 1;
        let player = self.player_id(0).map(|p| p as usize);
        if let Some(p) = player {
            let greed = self.fighters[p].build.fx.greed;
            let amount = (f.currency_value as f32 * greed) as u32;
            if amount > 0 && self.cfg.mode != Mode::Versus {
                self.fighters[p].currency += amount;
                self.emit(SimEvent::Currency { amount });
            }
        }
        match &f.kind {
            FighterKind::Boss(id) => {
                self.stats.boss_killed = Some(id.clone());
                self.stats.beaten_tags.push(f.tag.clone());
                // Bosses always yield their core (hearts/reactors are buried; only death frees them).
                if f.part_alive(Slot::Core) {
                    self.drop_part(content, j, Slot::Core, Condition::Scarred);
                    self.stats.harvested.push((f.tag.clone(), Slot::Core));
                }
                if let Some(s) = f.alive_parts().into_iter().find(|s| s.severable()) {
                    self.drop_part(content, j, s, Condition::Scarred);
                }
                if Some(j as u32) == self.boss {
                    self.victory = true;
                }
            }
            FighterKind::Nemesis { name, .. } => {
                self.stats.nemeses_killed.push(name.clone());
                for s in f.alive_parts().into_iter().filter(|s| s.severable()).take(2) {
                    self.drop_part(content, j, s, Condition::Scarred);
                }
                if self.cfg.mode == Mode::Gauntlet || self.cfg.mode == Mode::Run {
                    self.check_wave_clear();
                }
            }
            _ => {
                let harvester = player.map_or(1.0, |p| self.fighters[p].build.fx.harvester);
                if self.rng.chance(0.22 * harvester) {
                    let alive: Vec<Slot> = f.alive_parts();
                    if let Some(s) = self.rng.pick(&alive).copied() {
                        let cond = if self.rng.chance(0.4) { Condition::Mangled } else { Condition::Scarred };
                        self.drop_part(content, j, s, cond);
                        if s == Slot::Core {
                            self.stats.harvested.push((f.tag.clone(), Slot::Core));
                        }
                    }
                }
                if matches!(self.cfg.mode, Mode::Run | Mode::Arena | Mode::Gauntlet) {
                    self.check_wave_clear();
                }
            }
        }
    }

    fn check_wave_clear(&mut self) {
        let left = self.fighters.iter().any(|f| {
            f.team == Team::Enemy
                && f.alive
                && !matches!(f.act, Act::Dead)
                && !f.hallucination
                && !matches!(f.kind, FighterKind::Enemy(ref id) if id.starts_with("dummy"))
        });
        if !left && self.cfg.spawns.is_some() {
            self.victory = true;
        }
    }

    fn player_death(&mut self, j: usize) {
        self.stats.deaths += 1;
        let f = self.fighters[j].clone();
        if self.cfg.mode != Mode::Mission {
            self.emit(SimEvent::PlayerDied { id: j as u32, pos: f.pos });
            return;
        }
        // Any older unrecovered wreck of yours on this side loses its currency (one trip only).
        for w in self.wrecks.iter_mut() {
            if w.side == f.loadout.side && w.district == self.cfg.district {
                w.currency = 0;
            }
        }
        let keep = if self.cfg.assists.insurance { f.currency / 2 } else { 0 };
        let drop = if f.build.fx.no_recovery { 0 } else { f.currency - keep };
        let uid = self.uid();
        self.wrecks.push(Wreck {
            uid,
            side: f.loadout.side,
            district: self.cfg.district.clone(),
            pos: f.pos,
            currency: drop,
            loadout: f.loadout.clone(),
            war: self.cfg.war,
            tide: self.cfg.tide.tide_count,
            harvested: false,
        });
        self.fighters[j].currency = keep;
        self.emit(SimEvent::PlayerDied { id: j as u32, pos: f.pos });
    }

    // ------------------------------------------------------------------ movement

    fn movement(&mut self, content: &Content, i: usize, c: Command) {
        let water = self.water;
        let tick = self.tick;
        let f = &self.fighters[i];
        if !f.alive {
            return;
        }
        let act = f.act.clone();
        let mut desired = Vec3::ZERO;
        let speed = f.build.speed * f.speed_mul * if f.status.tether > 0 { 0.5 } else { 1.0 };
        let size = f.size();
        let mut face_to: Option<f32> = None;
        let lock_pos = f.lock.and_then(|l| self.fighters.get(l.target as usize)).filter(|t| t.alive).map(|t| t.pos);
        let ai_target_pos = f.ai.as_ref().and_then(|a| a.target).and_then(|t| self.fighters.get(t as usize)).map(|t| t.pos);
        let target_pos = lock_pos.or(ai_target_pos);
        match &act {
            Act::Idle | Act::Guard => {
                let mut d = Vec3::new(c.move_dir.x, 0.0, c.move_dir.y);
                if d.length() > 1.0 {
                    d = d.normalize();
                }
                let gk = if matches!(act, Act::Guard) { 0.45 } else { 1.0 };
                desired = d * speed * gk;
                // Lopsided builds drift sideways while moving.
                if d.length_squared() > 0.01 {
                    let right = Vec3::new(f.forward().z, 0.0, -f.forward().x);
                    desired += right * f.build.com.x * 0.25 * speed * 0.1;
                }
                if let Some(tp) = target_pos {
                    if f.lock.is_some() || f.ai.is_some() {
                        face_to = Some(geom::dir_yaw(tp - f.pos));
                    }
                } else if d.length_squared() > 0.01 {
                    face_to = Some(geom::dir_yaw(d));
                }
                if f.ai.is_some() && d.length_squared() > 0.01 && target_pos.is_none() {
                    face_to = Some(geom::dir_yaw(d));
                }
            }
            Act::Attack { mv, skip, .. } => {
                if let Some(m) = content.mv(&mv.id) {
                    let w = (m.windup + m.feint).saturating_sub(*skip) as u32;
                    let lunge_t = w + m.active as u32;
                    if f.act_t < lunge_t && m.lunge > 0.0 {
                        let mut dist = m.lunge * size / lunge_t.max(1) as f32 * 60.0;
                        if let Some(tp) = target_pos {
                            let gap = tp.distance(f.pos) - f.radius() * 2.0;
                            if gap < 1.0 {
                                dist = 0.0;
                            }
                        }
                        desired = f.forward() * dist;
                    }
                    // Tracking during windup (bosses track less late in the windup).
                    if f.act_t < w {
                        if let Some(tp) = target_pos {
                            face_to = Some(geom::dir_yaw(tp - f.pos));
                        }
                    }
                    if f.burrowed && f.act_t < w {
                        if let Some(tp) = target_pos {
                            desired = (tp - f.pos).normalize_or_zero() * speed * 1.6;
                        }
                    }
                }
            }
            Act::Dodge { dir, dist, ticks, kind, .. } => {
                let k = 1.0 - f.act_t as f32 / *ticks as f32;
                let curve = match kind {
                    DodgeKind::Dash => 2.2 * k,
                    DodgeKind::Hop => 1.6 * (k * std::f32::consts::PI).sin().max(0.2),
                    DodgeKind::Slide => 2.0 * k * k + 0.2,
                };
                desired = Vec3::new(dir.x, 0.0, dir.y) * (*dist / *ticks as f32 * 60.0) * curve;
            }
            Act::Climbing { target, height, .. } => {
                let t = &self.fighters[*target as usize];
                let p = t.pos + t.forward() * -(t.radius() * 0.6);
                let alt = t.height() * height * 0.85;
                let f = &mut self.fighters[i];
                f.pos = p;
                f.altitude = alt;
                f.vel = Vec3::ZERO;
                return;
            }
            Act::Severing { target, .. } => {
                if let Some(t) = self.fighters.get(*target as usize) {
                    face_to = Some(geom::dir_yaw(t.pos - f.pos));
                }
            }
            Act::Grabbed { by } => {
                let g = &self.fighters[*by as usize];
                let p = g.pos + g.forward() * (g.radius() + f.radius() * 0.5);
                let f = &mut self.fighters[i];
                f.pos = p;
                f.vel = Vec3::ZERO;
                return;
            }
            _ => {}
        }
        // Water.
        let ground = self.city.ground(f.pos.x, f.pos.z);
        let depth = (water - ground).max(0.0);
        let f = &mut self.fighters[i];
        f.in_water = depth;
        if !f.build.flying && depth > 1.0 {
            let k = if depth > 3.0 { f.build.swim } else { 1.0 + (f.build.swim - 1.0) * (depth - 1.0) / 2.0 };
            desired *= k.max(0.2);
        }
        // Turn.
        if let Some(y) = face_to {
            let step = f.build.turn_speed * DT * if matches!(act, Act::Attack { .. }) { 0.35 } else { 1.0 };
            f.yaw = geom::approach_angle(f.yaw, y, step);
        }
        // Integrate with weight-dependent acceleration.
        let accel = if matches!(act, Act::Dodge { .. }) { 1.0 } else { (0.3 - f.build.load * 0.12).clamp(0.1, 0.3) };
        let target_vel = desired;
        f.vel.x += (target_vel.x - f.vel.x) * accel;
        f.vel.z += (target_vel.z - f.vel.z) * accel;
        if matches!(act, Act::Stagger { .. } | Act::PostureBroken | Act::Vent | Act::Heal | Act::Dead) {
            f.vel *= 0.85;
        }
        let mut np = f.pos + Vec3::new(f.vel.x, 0.0, f.vel.z) * DT;
        // Deep water blocks non-swimmers.
        let ng = self.city.ground(np.x, np.z);
        if !f.build.flying && !f.build.deep_water && water - ng > 5.0 {
            np = f.pos;
            f.vel *= 0.0;
        }
        np = self.city.in_bounds(np);
        let ng = self.city.ground(np.x, np.z);
        f.pos = Vec3::new(np.x, ng.max(if f.build.deep_water { ng } else { ng }), np.z);
        // Flying altitude.
        if f.build.flying {
            let want = match &act {
                Act::Attack { .. } => 2.0,
                Act::Dead => 0.0,
                Act::PostureBroken => 0.5,
                _ => 5.0 + (tick as f32 * 0.05 + i as f32).sin() * 0.6,
            };
            f.altitude += (want - f.altitude) * 0.08;
        }
        // Footsteps.
        let sp = Vec2::new(f.vel.x, f.vel.z).length();
        if sp > 0.5 && !f.build.flying && !f.burrowed {
            let stride = 4.0 * size;
            let before = f.step_phase;
            f.step_phase += sp * DT / stride;
            if f.step_phase.floor() != before.floor() {
                let weight = f.build.weight * size * size / 100.0;
                let pos = f.pos;
                let id = f.id;
                let kaiju = f.loadout.side == Side::Kaiju && size >= 1.0;
                self.emit(SimEvent::Footstep { id, pos, weight });
                if kaiju || size > 1.3 {
                    self.emit(SimEvent::Shake { amount: 0.08 * weight.min(3.0), pos });
                }
            }
        }
        self.collide_buildings(i);
    }

    fn collide_buildings(&mut self, i: usize) {
        let f = &self.fighters[i];
        if f.burrowed {
            return;
        }
        let cap = Capsule { base: f.pos + Vec3::Y * f.altitude, height: f.height(), r: f.radius() };
        let big = f.size() >= 1.25 && f.team == Team::Enemy;
        let mut push = Vec2::ZERO;
        let mut smash = vec![];
        for (bi, b) in self.city.buildings.iter().enumerate() {
            if !b.aabb.expand(cap.r + 0.5).contains(Vec3::new(cap.base.x, b.aabb.min.y.max(cap.base.y.min(b.aabb.max.y)), cap.base.z)) {
                continue;
            }
            for ci in 0..b.alive.len() {
                if !b.alive[ci] {
                    continue;
                }
                let a = b.chunk_aabb(ci);
                let moved = Capsule { base: cap.base + Vec3::new(push.x, 0.0, push.y), ..cap };
                if let Some(p) = moved.push_out_aabb(&a) {
                    if big
                        && matches!(
                            b.kind,
                            crate::city::BuildingKind::House
                                | crate::city::BuildingKind::Stall
                                | crate::city::BuildingKind::Railcar
                                | crate::city::BuildingKind::Barricade
                        )
                    {
                        smash.push((bi, ci));
                    } else {
                        push += p;
                    }
                }
            }
        }
        let f = &mut self.fighters[i];
        f.pos.x += push.x;
        f.pos.z += push.y;
        for (bi, ci) in smash {
            self.city.buildings[bi].chunk_hp[ci] -= 3.0;
            if self.city.buildings[bi].chunk_hp[ci] <= 0.0 {
                self.destroy_chunk(bi, ci, None);
            }
        }
    }

    fn separate(&mut self) {
        let n = self.fighters.len();
        for a in 0..n {
            for b in (a + 1)..n {
                let (fa, fb) = two(&mut self.fighters, a, b);
                if !fa.alive || !fb.alive || matches!(fa.act, Act::Dead) || matches!(fb.act, Act::Dead) {
                    continue;
                }
                if fa.burrowed
                    || fb.burrowed
                    || matches!(fa.act, Act::Climbing { .. } | Act::Grabbed { .. })
                    || matches!(fb.act, Act::Climbing { .. } | Act::Grabbed { .. })
                {
                    continue;
                }
                if (fa.altitude - fb.altitude).abs() > fa.height().max(fb.height()) {
                    continue;
                }
                let d = Vec2::new(fb.pos.x - fa.pos.x, fb.pos.z - fa.pos.z);
                let r = fa.radius() + fb.radius();
                let l = d.length();
                if l < r && l > 1e-4 {
                    let n = d / l;
                    let over = r - l;
                    let wa = fa.build.weight * fa.size();
                    let wb = fb.build.weight * fb.size();
                    let ka = wb / (wa + wb);
                    let kb = 1.0 - ka;
                    fa.pos.x -= n.x * over * ka;
                    fa.pos.z -= n.y * over * ka;
                    fb.pos.x += n.x * over * kb;
                    fb.pos.z += n.y * over * kb;
                }
            }
        }
    }

    // ------------------------------------------------------------------ projectiles, hazards

    fn update_projectiles(&mut self, content: &Content) {
        let mut i = 0;
        while i < self.projectiles.len() {
            let mut p = self.projectiles[i].clone();
            let old = p.pos;
            if p.homing > 0.0 {
                if let Some(t) = p.target.and_then(|t| self.fighters.get(t as usize)) {
                    let d = (t.center() - p.pos).normalize_or_zero();
                    let sp = p.vel.length();
                    p.vel = (p.vel.normalize_or_zero() + d * p.homing * DT).normalize_or_zero() * sp;
                }
            }
            p.vel.y -= p.gravity * DT;
            p.pos += p.vel * DT;
            p.life = p.life.saturating_sub(1);
            let mut explode = p.life == 0 || p.pos.y < self.city.ground(p.pos.x, p.pos.z) - 0.5;
            let mut direct: Option<usize> = None;
            for (j, o) in self.fighters.iter().enumerate() {
                if !o.alive || matches!(o.act, Act::Dead) || !p.team.hostile(o.team) || o.burrowed {
                    continue;
                }
                let c = o.center();
                if geom::swept_sphere_hit(old, p.pos, p.radius, c, o.radius() + o.height() * 0.25) {
                    direct = Some(j);
                    explode = true;
                    break;
                }
            }
            if !explode {
                for b in &self.city.buildings {
                    if b.aabb.contains(p.pos) {
                        for ci in 0..b.alive.len() {
                            if b.alive[ci] && b.chunk_aabb(ci).contains(p.pos) {
                                explode = true;
                                break;
                            }
                        }
                    }
                    if explode {
                        break;
                    }
                }
            }
            if explode {
                self.projectiles.swap_remove(i);
                let Some(m) = content.mv(&p.mv).cloned() else { continue };
                let owner = p.owner as usize;
                if p.hallucination || owner >= self.fighters.len() {
                    continue;
                }
                let splash = p.radius >= 1.8;
                if let Some(j) = direct {
                    self.resolve_hit(content, owner, j, &m, p.slot, old, 1.0);
                }
                if splash {
                    self.emit(SimEvent::Explosion { pos: p.pos, radius: p.radius * 1.6 });
                    for j in 0..self.fighters.len() {
                        if Some(j) == direct {
                            continue;
                        }
                        let o = &self.fighters[j];
                        if o.alive
                            && !matches!(o.act, Act::Dead)
                            && p.team.hostile(o.team)
                            && o.center().distance(p.pos) < p.radius * 1.6 + o.radius()
                        {
                            self.resolve_hit(content, owner, j, &m, p.slot, p.pos, 0.6);
                        }
                    }
                    if m.effect == Some(MoveEffect::Burn) {
                        self.hazards.push(Hazard {
                            kind: HazardKind::Fire,
                            pos: Vec3::new(p.pos.x, 0.0, p.pos.z),
                            radius: p.radius * 1.4,
                            life: 240,
                            team: p.team,
                        });
                    }
                    // Shells crack buildings.
                    for bi in 0..self.city.buildings.len() {
                        if !self.city.buildings[bi].aabb.sphere_hit(p.pos, p.radius * 1.6) {
                            continue;
                        }
                        for ci in 0..self.city.buildings[bi].alive.len() {
                            let b = &mut self.city.buildings[bi];
                            if b.alive[ci] && b.chunk_aabb(ci).sphere_hit(p.pos, p.radius * 1.6) {
                                b.chunk_hp[ci] -= m.damage;
                                if b.chunk_hp[ci] <= 0.0 {
                                    self.destroy_chunk(bi, ci, None);
                                }
                            }
                        }
                    }
                }
                continue;
            }
            self.projectiles[i] = p;
            i += 1;
        }
    }

    fn update_hazards(&mut self) {
        for h in self.hazards.iter_mut() {
            h.life = h.life.saturating_sub(1);
        }
        self.hazards.retain(|h| h.life > 0);
        let hz = self.hazards.clone();
        for f in self.fighters.iter_mut() {
            if !f.alive || matches!(f.act, Act::Dead) {
                continue;
            }
            f.status.flood_heal = false;
            for h in &hz {
                if f.pos.distance(h.pos) > h.radius + f.radius() {
                    continue;
                }
                match h.kind {
                    HazardKind::Fire if h.team.hostile(f.team) => f.status.burn = f.status.burn.max(60),
                    HazardKind::Spores if h.team.hostile(f.team) => f.status.spores = f.status.spores.max(60),
                    HazardKind::Flood => {
                        if f.build.fx.has_marker("flood_heal") {
                            f.status.flood_heal = true;
                        } else if h.team.hostile(f.team) {
                            f.status.tether = f.status.tether.max(10);
                        }
                    }
                    HazardKind::Smoke if h.team.hostile(f.team) => {
                        f.lock = None;
                    }
                    _ => {}
                }
            }
        }
    }

    fn status_tick(&mut self, content: &Content, i: usize, t: &TuningDef) {
        let blackout = self.cfg.tide.blackout();
        let side_bonus = self.cfg.tide.side_bonus(self.fighters[i].loadout.side);
        let pylons = self.city.pylons.clone();
        let f = &mut self.fighters[i];
        if !f.alive || matches!(f.act, Act::Dead) {
            return;
        }
        let mut dot = 0.0;
        if f.status.burn > 0 {
            f.status.burn -= 1;
            dot += 6.0;
        }
        dot += f.build.fx.bleed;
        if f.status.flood_heal {
            f.hp = (f.hp + f.build.hp_max * 0.02 * DT).min(f.build.hp_max);
        }
        f.hp -= dot * DT;
        f.status.spores = f.status.spores.saturating_sub(1);
        f.status.tether = f.status.tether.saturating_sub(1);
        f.status.static_ = f.status.static_.saturating_sub(1);
        // Posture recovery.
        f.posture_idle = f.posture_idle.saturating_add(1);
        if f.posture_idle > 80 && !matches!(f.act, Act::PostureBroken) {
            let k = if matches!(f.act, Act::Guard) { 0.5 } else { 1.0 };
            f.posture = (f.posture - f.build.posture_max * 0.16 * f.build.fx.steady * k * DT).max(0.0);
        }
        // Stamina.
        let grid = f.loadout.slots[Slot::Core.index()].as_ref().is_some_and(|p| p.def == "st_anchor_coupling");
        let mut regen = side_bonus;
        if grid {
            let near = pylons.iter().any(|p| Vec2::new(p.x - f.pos.x, p.z - f.pos.z).length() < 45.0);
            regen *= if blackout {
                0.5
            } else if near {
                1.6
            } else {
                1.0
            };
        }
        if f.status.spores > 0 {
            regen *= 0.5;
        }
        let ctx = StaminaCtx {
            regen_mul: regen,
            idle: matches!(f.act, Act::Idle) && f.vel.length_squared() < 0.5,
            idle_drain: f.build.fx.idle_drain,
            famine: f.build.fx.famine,
        };
        f.stamina.update(&ctx);
        let overheated = f.stamina.overheated && !matches!(f.act, Act::Vent | Act::Dead);
        let dead = f.hp <= 0.0;
        if overheated && !matches!(f.act, Act::Attack { .. }) {
            self.set_act(i, Act::Vent);
            self.emit(SimEvent::Vent { id: i as u32 });
        }
        if dead {
            let by = self.fighters[i].last_attacker.map(|x| x as usize);
            self.kill(content, i, by);
        }
        let _ = t;
    }

    fn boss_tick(&mut self, content: &Content) {
        // Wake the boss when a player enters the arena.
        if let Some(b) = self.boss {
            let (c, r) = self.city.arena;
            if !self.boss_awake {
                let near = self.fighters.iter().any(|f| f.kind.is_player() && f.alive && f.pos.distance(c) < r + 6.0);
                if near {
                    self.boss_awake = true;
                    if let Some(br) = self.fighters[b as usize].ai.as_mut() {
                        br.awake = true;
                        br.aggro = 999.0;
                    }
                    self.emit(SimEvent::BossAwake { id: b });
                }
            }
        }
        for i in 0..self.fighters.len() {
            let f = &self.fighters[i];
            if !f.alive || matches!(f.act, Act::Dead) {
                continue;
            }
            let Some(bid) = f.ai.as_ref().and_then(|a| a.boss.clone()) else { continue };
            let Some(bd) = content.boss(&bid) else { continue };
            let hp = f.hp_frac();
            let mut phase = f.boss_phase;
            while phase + 1 < bd.phases.len() && hp <= bd.phases[phase + 1].at_hp {
                phase += 1;
            }
            if phase != f.boss_phase {
                self.fighters[i].boss_phase = phase;
                self.emit(SimEvent::BossPhase { id: i as u32, phase, line: bd.phases[phase].line.clone() });
            }
        }
    }

    fn pickup_tick(&mut self) {
        let players: Vec<(usize, Vec3)> =
            self.fighters.iter().filter(|f| f.kind.is_player() && f.alive).map(|f| (f.id as usize, f.pos)).collect();
        let mut k = 0;
        while k < self.pickups.len() {
            self.pickups[k].age += 1;
            let p = self.pickups[k].pos;
            if let Some(&(pi, _)) = players.iter().find(|(_, pp)| Vec2::new(pp.x - p.x, pp.z - p.z).length() < 9.0) {
                let pk = self.pickups.swap_remove(k);
                match pk.item {
                    PickupItem::Part(part) => self.collected.push(part),
                    PickupItem::Currency(c) => {
                        self.fighters[pi].currency += c;
                        self.emit(SimEvent::Currency { amount: c });
                    }
                }
                continue;
            }
            k += 1;
        }
    }

    fn hallucinations(&mut self, content: &Content) {
        let Some(p) = self.player(0).map(|p| (p.pos, p.build.fx.hallucinate)) else { return };
        let (ppos, h) = p;
        if h <= 0.0 || self.tick % 300 != 0 || !self.rng.chance((h * 2.0).min(0.9)) {
            return;
        }
        let side = self.cfg.side;
        let pool: Vec<String> = content
            .enemies
            .iter()
            .filter(|e| e.side != side && e.archetype != Archetype::Dummy && !e.id.starts_with("demo"))
            .map(|e| e.id.clone())
            .collect();
        let Some(id) = self.rng.pick(&pool).cloned() else { return };
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        let pos = ppos + Vec3::new(a.sin(), 0.0, a.cos()) * 28.0;
        if let Some(fid) = self.spawn_spec(content, SpawnSpec::Enemy(id), pos) {
            let f = &mut self.fighters[fid as usize];
            f.hallucination = true;
            f.currency_value = 0;
            if let Some(b) = f.ai.as_mut() {
                b.awake = true;
                b.aggro = 999.0;
            }
            self.emit(SimEvent::Hallucination { id: fid });
        }
    }
}

trait CloneWith {
    fn clone_with(self, id: &str) -> Origin;
}
impl CloneWith for Origin {
    fn clone_with(self, id: &str) -> Origin {
        match self {
            // Boss parts remember the chassis they came from (for Old Grudge); unique bosses use their id.
            Origin::Boss(tag) if tag.is_empty() => Origin::Boss(id.to_string()),
            o => o,
        }
    }
}

#[allow(dead_code)]
fn _assert_aabb_used(_: Aabb, _: CheckpointKind) {}
