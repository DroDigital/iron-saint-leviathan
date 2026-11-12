//! A playable mission: wraps the deterministic `World` with camera, presentation, audio/VFX
//! plumbing, habit recording, interpolation and the end-of-mission report.

use crate::camera::{Camera, Focus};
use crate::district_mesh;
use crate::settings::Settings;
use crate::vfx::Vfx;
use crate::visuals::{Visuals, pose_of};
use glam::{Vec2, Vec3, Vec4};
use isl_audio::{Audio, Sfx, synth::cue_sfx};
use isl_data::*;
use isl_ghost::Recorder;
use isl_input::{Action, PlayerInput};
use isl_render::{DrawList, Lighting, View, billboard, textures};
use isl_sim::fighter::*;
use isl_sim::world::*;

pub const DT: f32 = 1.0 / 60.0;

#[derive(Clone, Debug, PartialEq)]
pub enum MissionKind {
    War { district: String, side: Side },
    Run,
    Gauntlet,
    Arena,
    Versus,
    Finale { ending: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Victory,
    Defeat,
    Retreat,
}

pub struct Msg {
    pub text: String,
    pub ttl: f32,
    pub color: [u8; 4],
}

pub struct Intro {
    pub title: String,
    pub sub: String,
    pub card: String,
    pub ttl: f32,
}

pub struct Mission {
    pub world: World,
    pub kind: MissionKind,
    pub cams: [Camera; 2],
    pub vfx: Vfx,
    pub vis: Visuals,
    pub recorder: Recorder,
    prev: Vec<(Vec3, f32)>,
    cur: Vec<(Vec3, f32)>,
    pub static_dirty: bool,
    pub light: Lighting,
    pub fog: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub flash: f32,
    pub msgs: Vec<Msg>,
    pub subtitle: Option<(String, f32)>,
    pub intro: Option<Intro>,
    pub dead_timer: Option<f32>,
    pub end: Option<End>,
    pub end_timer: f32,
    acc: f32,
    pub time: f32,
    pub hit_stop: f32,
    pub reverb: Reverb,
    pub water_tint: [u8; 3],
    pub photo: bool,
    pub photo_cam: (Vec3, f32, f32),
    pub boss_seen: bool,
    pub rumble: Vec<(usize, f32, u32)>,
    pub last_death_tick: Option<u64>,
    pub play_ticks: u64,
    pub telegraphs: Vec<(u32, f32)>,
    /// Rests taken since the game last synced the tide clock.
    pub rests: u32,
    /// One-shot presses latched until a sim tick consumes them (frames can run zero ticks).
    latched: [Command; 2],
}

impl Mission {
    pub fn new(world: World, kind: MissionKind, content: &Content) -> Mission {
        let d = content.district(&world.cfg.district).expect("district").clone();
        let (light, fog, near, far) = district_mesh::lighting(&d, world.cfg.state, &world.cfg.tide, &world.city);
        let n = world.fighters.len();
        let cur: Vec<(Vec3, f32)> = world.fighters.iter().map(|f| (f.pos, f.yaw)).collect();
        let mut cams = [Camera::default(), Camera::default()];
        for (i, c) in cams.iter_mut().enumerate() {
            if let Some(p) = world.player(i as u8) {
                c.yaw = p.yaw + std::f32::consts::PI;
                c.target = p.pos + Vec3::Y * 8.0;
            }
        }
        let player = world.player_id(0).unwrap_or(0);
        let fogm = world.fog_mul;
        let water_tint = match world.cfg.state {
            DistrictState::Flooded => [70, 110, 120],
            DistrictState::Ruined => [90, 100, 90],
            _ => [80, 120, 140],
        };
        let mut m = Mission {
            kind,
            cams,
            vfx: Vfx::default(),
            vis: Visuals::default(),
            recorder: Recorder::new(player),
            prev: cur.clone(),
            cur,
            static_dirty: true,
            light,
            fog,
            fog_near: near * fogm,
            fog_far: far * fogm,
            flash: 0.0,
            msgs: vec![],
            subtitle: None,
            intro: None,
            dead_timer: None,
            end: None,
            end_timer: 0.0,
            acc: 0.0,
            time: 0.0,
            hit_stop: 0.0,
            reverb: d.reverb,
            water_tint,
            photo: false,
            photo_cam: (Vec3::ZERO, 0.0, 0.0),
            boss_seen: false,
            rumble: vec![],
            last_death_tick: None,
            play_ticks: 0,
            telegraphs: vec![],
            rests: 0,
            latched: [Command::default(); 2],
            world,
        };
        let _ = n;
        // Head vision pushes back fog (senses).
        if let Some(p) = m.world.player(0) {
            m.fog_far += p.build.fog_vision;
        }
        m
    }

    pub fn note(&mut self, text: impl Into<String>, color: [u8; 4]) {
        self.msgs.push(Msg { text: text.into(), ttl: 4.0, color });
        if self.msgs.len() > 6 {
            self.msgs.remove(0);
        }
    }

    fn command(&self, inp: &PlayerInput, cam: &Camera, guard_toggle: bool) -> Command {
        let mv = cam.move_dir(inp.move_axis);
        Command {
            move_dir: mv,
            light: inp.hit(Action::Light),
            heavy: inp.hit(Action::Heavy),
            guard: inp.guard(guard_toggle),
            parry: inp.hit(Action::Parry),
            dodge: inp.hit(Action::Dodge),
            special: inp.hit(Action::Special),
            heal: inp.hit(Action::Heal),
            interact: inp.hit(Action::Interact),
            lock: inp.hit(Action::LockOn),
            lock_next: inp.hit(Action::LockNext),
            lock_prev: inp.hit(Action::LockPrev),
            lock_part: inp.hit(Action::LockPart),
            heretic: inp.hit(Action::Heretic),
        }
    }

    /// Advance by a frame. Returns nothing; read `end` afterwards.
    pub fn update(&mut self, content: &Content, dt: f32, inputs: &[PlayerInput; 2], settings: &Settings, audio: &mut Audio) {
        self.time += dt;
        self.flash = (self.flash - dt * 3.0).max(0.0);
        for m in self.msgs.iter_mut() {
            m.ttl -= dt;
        }
        self.msgs.retain(|m| m.ttl > 0.0);
        if let Some(s) = self.subtitle.as_mut() {
            s.1 -= dt;
        }
        if self.subtitle.as_ref().is_some_and(|s| s.1 <= 0.0) {
            self.subtitle = None;
        }
        if let Some(i) = self.intro.as_mut() {
            i.ttl -= dt;
        }
        if self.intro.as_ref().is_some_and(|i| i.ttl <= 0.0) {
            self.intro = None;
        }
        self.vfx.photosensitive = settings.photosensitive;
        if self.photo {
            self.photo_update(dt, &inputs[0]);
            return;
        }
        // Camera look.
        let versus = self.kind == MissionKind::Versus;
        let mut cmds = [Command::default(), Command::default()];
        for p in 0..(if versus { 2 } else { 1 }) {
            let inp = &inputs[p];
            let look = inp.cam_axis + Vec2::new(inp.mouse_delta.x, -inp.mouse_delta.y) * 0.08;
            let Some(pf) = self.world.player(p as u8) else { continue };
            let lock = pf.lock.and_then(|l| self.world.fighters.get(l.target as usize)).map(|t| (t.center(), t.height()));
            let huge =
                self.world.fighters.iter().any(|f| f.alive && f.kind.is_bossish() && f.size() > 1.35 && f.pos.distance(pf.pos) < 90.0);
            let focus = Focus {
                subject: self.interp(pf.id as usize, 1.0).0,
                subject_height: pf.height().max(4.0) + pf.altitude,
                lock,
                scale_mode: huge,
            };
            let city = &self.world.city;
            self.cams[p].update(dt, look, &focus, |x, z| city.ground(x, z));
            if self.dead_timer.is_none() && self.end.is_none() {
                cmds[p] = self.command(inp, &self.cams[p], settings.bindings.guard_toggle);
            }
        }
        // Latch presses so a frame that runs no sim tick (or hit-stop) never drops input.
        for p in 0..2 {
            let l = &mut self.latched[p];
            let c = cmds[p];
            l.move_dir = c.move_dir;
            l.guard = c.guard;
            l.light |= c.light;
            l.heavy |= c.heavy;
            l.parry |= c.parry;
            l.dodge |= c.dodge;
            l.special |= c.special;
            l.heal |= c.heal;
            l.interact |= c.interact;
            l.lock |= c.lock;
            l.lock_next |= c.lock_next;
            l.lock_prev |= c.lock_prev;
            l.lock_part |= c.lock_part;
            l.heretic |= c.heretic;
        }
        let mut cmds = self.latched;
        if self.hit_stop > 0.0 {
            self.hit_stop -= dt;
            return;
        }
        self.acc += dt.min(0.1);
        let mut steps = 0;
        while self.acc >= DT && steps < 4 {
            self.acc -= DT;
            steps += 1;
            self.prev = self.cur.clone();
            self.world.step(content, &cmds);
            // Only the first sub-step consumes one-shot presses.
            for c in self.latched.iter_mut() {
                *c = Command { move_dir: c.move_dir, guard: c.guard, ..Default::default() };
            }
            for c in cmds.iter_mut() {
                let mv = c.move_dir;
                let g = c.guard;
                *c = Command { move_dir: mv, guard: g, ..Default::default() };
            }
            self.cur = self.world.fighters.iter().map(|f| (f.pos, f.yaw)).collect();
            while self.prev.len() < self.cur.len() {
                self.prev.push(self.cur[self.prev.len()]);
            }
            let events: Vec<SimEvent> = self.world.events.drain(..).collect();
            self.recorder.observe(&self.world, content, &events);
            self.handle_events(content, &events, settings, audio);
            if self.dead_timer.is_none() && self.end.is_none() {
                self.play_ticks += 1;
            }
        }
        // Death / victory flow.
        if let Some(t) = self.dead_timer.as_mut() {
            *t += dt;
            if *t > 3.5 {
                self.dead_timer = None;
                match self.kind {
                    MissionKind::War { .. } => {
                        self.world.respawn_player(content, 0);
                        self.cur = self.world.fighters.iter().map(|f| (f.pos, f.yaw)).collect();
                        self.prev = self.cur.clone();
                        self.static_dirty = true;
                        self.note("YOUR REMAINS LIE WHERE YOU FELL", [200, 200, 200, 255]);
                    }
                    MissionKind::Arena => {
                        self.world.respawn_player(content, 0);
                    }
                    _ => self.end = Some(End::Defeat),
                }
            }
        }
        if self.world.victory && self.end.is_none() {
            self.end_timer += dt;
            if self.end_timer > 4.0 {
                self.end = Some(End::Victory);
            }
        }
        if versus {
            for p in 0..2u8 {
                if self.world.player(p).is_some_and(|f| matches!(f.act, Act::Dead)) && self.end.is_none() {
                    self.end_timer += dt;
                    if self.end_timer > 3.0 {
                        self.end = Some(End::Victory);
                    }
                }
            }
        }
        self.vfx.update(dt);
        // Ambient VFX from hazards and fires.
        if ((self.time * 20.0) as u32).is_multiple_of(3) {
            let hz: Vec<_> = self.world.hazards.iter().map(|h| (h.kind, h.pos, h.radius)).collect();
            for (k, p, r) in hz {
                match k {
                    HazardKind::Fire => self.vfx.fire(p, 1),
                    HazardKind::Spores => self.vfx.mist(p + Vec3::Y * 2.0, 1, [190, 210, 120, 110]),
                    HazardKind::Smoke => self.vfx.mist(p + Vec3::Y * 3.0, 1, [150, 150, 150, 140]),
                    HazardKind::Flood => self.vfx.splash(p + Vec3::new(r * 0.5, 0.0, 0.0), 1),
                }
            }
            for f in self.world.city.fires.clone().iter().take(6) {
                self.vfx.fire(*f, 1);
            }
        }
        // Audio listener.
        let c = &self.cams[0];
        audio.set_listener(c.pos, c.right());
        audio.set_reverb(self.reverb);
    }

    fn photo_update(&mut self, dt: f32, inp: &PlayerInput) {
        let (mut p, mut yaw, mut pitch) = self.photo_cam;
        yaw -= inp.cam_axis.x * dt * 1.5 + inp.mouse_delta.x * 0.004;
        pitch = (pitch + inp.cam_axis.y * dt * 1.2 - inp.mouse_delta.y * 0.004).clamp(-1.4, 1.4);
        let fwd = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        p += (fwd * inp.move_axis.y + right * -inp.move_axis.x) * dt * 25.0;
        self.photo_cam = (p, yaw, pitch);
    }

    pub fn enter_photo(&mut self) {
        let c = &self.cams[0];
        let d = (c.target - c.pos).normalize_or_zero();
        self.photo_cam = (c.pos, d.x.atan2(d.z), d.y.asin());
        self.photo = true;
    }

    fn handle_events(&mut self, content: &Content, events: &[SimEvent], settings: &Settings, audio: &mut Audio) {
        let me = self.world.player_id(0);
        let shake = settings.screen_shake;
        for e in events {
            match e {
                SimEvent::AttackStart { id, mv, cue, telegraph, pos, boss, .. } => {
                    let f = &self.world.fighters[*id as usize];
                    let pitch = (1.2 / f.size().max(0.4)).clamp(0.6, 1.6);
                    if *boss || f.team != Team::Player {
                        // Every telegraph has a distinct sound cue.
                        audio.play(cue_sfx(*cue), Some(*pos), if *boss { 1.0 } else { 0.6 }, pitch);
                        if *boss {
                            self.telegraphs.push((*id, 0.6));
                        }
                    } else {
                        audio.play(Sfx::Swing, Some(*pos), 0.5, pitch);
                    }
                    if let Some(m) = content.mv(mv)
                        && (matches!(*telegraph, Telegraph::Ring) || (m.kind == MoveKind::Burst && *boss))
                    {
                        let r = m.radius * f.size();
                        let secs = (m.windup + m.feint) as f32 / 60.0 + 0.1;
                        self.vfx.ring(*pos, r, r, secs, settings.danger(), true);
                    }
                }
                SimEvent::Hit { target, dmg, pos, blocked, heavy, attacker, .. } => {
                    let tf = &self.world.fighters[*target as usize];
                    let kaiju = tf.loadout.side == Side::Kaiju;
                    if *blocked {
                        audio.play(Sfx::Block, Some(*pos), 0.8, 1.0);
                        self.vfx.sparks(*pos, 8, [255, 230, 160, 255]);
                    } else if kaiju {
                        audio.play(Sfx::Flesh, Some(*pos), 0.9, 1.0);
                        self.vfx.mist(*pos, 5, [90, 140, 120, 160]);
                    } else {
                        audio.play(Sfx::Clank, Some(*pos), 0.9, 1.0);
                        self.vfx.sparks(*pos, 12, [255, 200, 120, 255]);
                        self.vfx.mist(*pos, 2, [40, 40, 40, 140]);
                    }
                    let involved = Some(*target) == me || Some(*attacker) == me;
                    if involved {
                        self.cams[0].add_trauma(if *heavy { 0.35 } else { 0.15 } * shake);
                        if *heavy && Some(*attacker) == me {
                            self.hit_stop = 0.05;
                        }
                        if Some(*target) == me {
                            self.rumble.push((0, if *heavy { 0.8 } else { 0.4 }, 160));
                        }
                    }
                    let _ = dmg;
                }
                SimEvent::Parried { pos, by, .. } => {
                    audio.play(Sfx::Parry, Some(*pos), 1.0, 1.0);
                    self.vfx.sparks(*pos + Vec3::Y * 6.0, 24, [255, 255, 255, 255]);
                    if Some(*by) == me {
                        self.hit_stop = 0.08;
                        self.note("PARRY", [255, 255, 255, 255]);
                    }
                }
                SimEvent::Dodge { id, .. } => {
                    let f = &self.world.fighters[*id as usize];
                    self.vfx.dust(f.pos, 3, f.size());
                }
                SimEvent::DodgeEvaded { id, .. } => {
                    if Some(*id) == me && settings.show_iframes {
                        self.note("EVADED", [140, 220, 255, 255]);
                    }
                }
                SimEvent::Footstep { pos, weight, id } => {
                    let f = &self.world.fighters[*id as usize];
                    let s = if f.loadout.side == Side::Kaiju { Sfx::StepKaiju } else { Sfx::StepMech };
                    audio.play(s, Some(*pos), (0.25 + weight * 0.25).min(0.9), 1.0 / f.size().max(0.5));
                    if f.in_water > 0.5 {
                        self.vfx.splash(*pos, 4);
                    } else if f.size() > 0.8 {
                        self.vfx.dust(*pos, 1, f.size() * 0.6);
                    }
                }
                SimEvent::Shake { amount, pos } => {
                    let d = self.cams[0].target.distance(*pos);
                    let k = (1.0 - d / 120.0).max(0.0);
                    self.cams[0].add_trauma(amount * k * 0.6 * shake);
                    if k > 0.3 {
                        self.rumble.push((0, amount * k, 120));
                    }
                }
                SimEvent::PartDestroyed { id, slot, pos } => {
                    audio.play(Sfx::Shatter, Some(*pos), 0.9, 1.0);
                    let f = &self.world.fighters[*id as usize];
                    self.vfx.debris(*pos, 10, [140, 130, 120], 2.0);
                    if Some(*id) == me {
                        self.note(format!("{} DESTROYED", slot.name_for(f.loadout.side)), settings.danger());
                    }
                }
                SimEvent::Severed { slot, pos, target, by } => {
                    audio.play(Sfx::Sever, Some(*pos), 1.0, 1.0);
                    self.vfx.debris(*pos, 18, [160, 150, 130], 2.5);
                    self.vfx.mist(*pos, 12, [120, 30, 30, 180]);
                    self.cams[0].add_trauma(0.5 * shake);
                    if Some(*by) == me {
                        let t = &self.world.fighters[*target as usize];
                        self.note(format!("SEVERED: {}", slot.name_for(t.loadout.side)), [255, 210, 110, 255]);
                    }
                }
                SimEvent::PostureBroken { id, pos } => {
                    audio.play(Sfx::Shatter, Some(*pos), 0.8, 0.7);
                    self.vfx.ring(*pos, 2.0, 14.0, 0.5, [255, 230, 120, 200], false);
                    if Some(*id) != me && self.world.player(0).is_some_and(|p| p.lock.is_some_and(|l| l.target == *id)) {
                        self.note("POSTURE BROKEN - HEAVY TO SEVER", [255, 230, 120, 255]);
                    }
                }
                SimEvent::Vent { id } => {
                    let p = self.world.fighters[*id as usize].center();
                    audio.play(Sfx::Vent, Some(p), 0.8, 1.0);
                    self.vfx.mist(p, 14, [230, 230, 230, 160]);
                    if Some(*id) == me {
                        self.note("OVERHEAT - VENTING", [255, 150, 50, 255]);
                    }
                }
                SimEvent::Heal { id, .. } => {
                    let p = self.world.fighters[*id as usize].center();
                    audio.play(Sfx::Heal, Some(p), 0.7, 1.0);
                    self.vfx.sparks(p, 10, [120, 255, 160, 255]);
                }
                SimEvent::Death { pos, id, .. } => {
                    let f = &self.world.fighters[*id as usize];
                    audio.play(
                        if f.loadout.side == Side::Kaiju { Sfx::Roar } else { Sfx::Explosion },
                        Some(*pos),
                        1.0,
                        0.7 / f.size().max(0.5),
                    );
                    self.vfx.debris(*pos + Vec3::Y * 3.0, 20, [120, 110, 100], 3.0 * f.size());
                    self.vfx.dust(*pos, 8, f.size());
                }
                SimEvent::PlayerDied { id, .. } => {
                    if Some(*id) == me || self.kind != MissionKind::Versus {
                        audio.play(Sfx::Death, None, 1.0, 1.0);
                        self.dead_timer = Some(0.0);
                        self.last_death_tick = Some(self.world.tick);
                    }
                }
                SimEvent::Loot { def, condition, pos } => {
                    audio.play(Sfx::Pickup, Some(*pos), 0.8, 1.0);
                    let name = content.part(def).map_or(def.clone(), |d| d.name.clone());
                    self.note(format!("{} [{}]", name, condition.name()), [255, 210, 110, 255]);
                }
                SimEvent::Currency { amount } => {
                    audio.play(Sfx::Coins, None, 0.4, 1.0);
                    let cur = self.world.cfg.side.currency();
                    self.note(format!("+{amount} {cur}"), [200, 200, 220, 255]);
                }
                SimEvent::ChunkDestroyed { pos, eaten, .. } => {
                    self.static_dirty = true;
                    self.vfx.debris(*pos, if *eaten { 4 } else { 8 }, [150, 140, 130], 3.0);
                    self.vfx.dust(*pos - Vec3::Y * 2.0, 2, 1.5);
                    audio.play(if *eaten { Sfx::Eat } else { Sfx::Collapse }, Some(*pos), 0.7, 1.0);
                }
                SimEvent::Explosion { pos, radius } => {
                    audio.play(Sfx::Explosion, Some(*pos), 0.8, 1.0);
                    self.vfx.ring(*pos, 1.0, *radius * 1.5, 0.45, [255, 220, 160, 220], false);
                    self.vfx.sparks(*pos + Vec3::Y, 10, [255, 160, 60, 255]);
                    self.vfx.dust(*pos, 4, radius / 3.0);
                }
                SimEvent::BossAwake { id } => {
                    let f = &self.world.fighters[*id as usize];
                    if let FighterKind::Boss(b) = &f.kind
                        && let Some(bd) = content.boss(b)
                    {
                        self.intro = Some(Intro { title: bd.name.clone(), sub: bd.title.clone(), card: bd.intro.clone(), ttl: 4.5 });
                    }
                    self.boss_seen = true;
                    audio.play(Sfx::Bell, None, 0.8, 0.5);
                }
                SimEvent::BossPhase { line, .. } => {
                    self.subtitle = Some((line.clone(), 4.0));
                    audio.play(Sfx::Roar, None, 0.7, 0.6);
                }
                SimEvent::Rest { .. } => {
                    self.rests += 1;
                    audio.play(Sfx::Rest, None, 0.9, 1.0);
                    self.note("RESTED. THE TIDE CLOCK ADVANCES.", [120, 230, 240, 255]);
                    self.static_dirty = true;
                }
                SimEvent::WreckRecovered { amount } => {
                    audio.play(Sfx::Coins, None, 0.9, 1.0);
                    self.note(format!("RECOVERED {amount}"), [255, 210, 110, 255]);
                }
                SimEvent::WreckHarvested { parts } => {
                    audio.play(Sfx::Pickup, None, 0.9, 0.8);
                    self.note(format!("STRIPPED YOUR REMAINS: {}", parts.join(", ")), [255, 210, 110, 255]);
                }
                SimEvent::Climb { .. } => self.note("CLIMBING - LIGHT/HEAVY TO RIP, DODGE TO DROP", [255, 255, 255, 255]),
                SimEvent::Rip { .. } | SimEvent::Thrown { .. } => {}
                SimEvent::Grabbed { target, .. } => {
                    if Some(*target) == me {
                        self.note("CONSTRICTED - MASH DODGE", settings.danger());
                    }
                }
                SimEvent::Hallucination { .. } => {
                    audio.play(Sfx::Drone, None, 0.3, 1.6);
                }
                SimEvent::Mimic { mv, .. } => {
                    let n = content.mv(mv).map_or(mv.clone(), |m| m.name.clone());
                    self.note(format!("NULL COPIES: {n}"), [230, 230, 255, 255]);
                }
                SimEvent::Lightning { .. } => {
                    if !settings.photosensitive {
                        self.flash = 0.6;
                    }
                    audio.play(Sfx::Lightning, None, 0.8, 1.0);
                }
                SimEvent::Eat { .. } => {}
                SimEvent::Hazard { pos, radius, kind } => {
                    if *kind == HazardKind::Spores {
                        self.vfx.spores(*pos, *radius);
                    }
                    if *kind == HazardKind::Flood {
                        self.vfx.ring(*pos, 1.0, *radius, 1.0, [120, 200, 255, 180], false);
                    }
                }
                SimEvent::LockOn { .. } => audio.play(Sfx::Lock, None, 0.4, 1.0),
                SimEvent::ProjectileFired { pos, .. } => audio.play(Sfx::Click, Some(*pos), 0.4, 0.7),
                _ => {}
            }
        }
    }

    pub fn interp(&self, i: usize, alpha: f32) -> (Vec3, f32) {
        let a = self.prev.get(i).copied().unwrap_or((Vec3::ZERO, 0.0));
        let b = self.cur.get(i).copied().unwrap_or(a);
        let mut dy = b.1 - a.1;
        if dy > std::f32::consts::PI {
            dy -= std::f32::consts::TAU;
        } else if dy < -std::f32::consts::PI {
            dy += std::f32::consts::TAU;
        }
        (a.0.lerp(b.0, alpha), a.1 + dy * alpha)
    }

    pub fn alpha(&self) -> f32 {
        (self.acc / DT).clamp(0.0, 1.0)
    }

    /// The view for player `p` with viewport.
    pub fn view(&self, p: usize, viewport: [u32; 4], settings: &Settings) -> View {
        let aspect = viewport[2] as f32 / viewport[3].max(1) as f32
            * (settings.aspect() / (settings.low_res().0 as f32 / settings.low_res().1 as f32));
        let (eye, vm) = if self.photo {
            let (pos, yaw, pitch) = self.photo_cam;
            let fwd = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
            (pos, isl_render::look_at(pos, pos + fwd, Vec3::Y))
        } else {
            (self.cams[p].pos, self.cams[p].view(settings.screen_shake))
        };
        let proj = isl_render::projection(self.cams[p].fov, aspect, self.fog_far + 30.0);
        View {
            view_proj: proj * vm,
            cam: eye,
            fog_color: self.fog,
            fog_near: self.fog_near,
            fog_far: self.fog_far,
            flash: if settings.photosensitive { 0.0 } else { self.flash * 0.35 },
            viewport,
            draw_static: true,
            clear: true,
        }
    }

    /// World-to-screen (viewport pixels). None when behind the camera.
    pub fn project(view: &View, p: Vec3) -> Option<Vec2> {
        let c = view.view_proj * Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.1 {
            return None;
        }
        let n = Vec2::new(c.x / c.w, c.y / c.w);
        if n.x.abs() > 1.2 || n.y.abs() > 1.2 {
            return None;
        }
        Some(Vec2::new(
            view.viewport[0] as f32 + (n.x * 0.5 + 0.5) * view.viewport[2] as f32,
            view.viewport[1] as f32 + (0.5 - n.y * 0.5) * view.viewport[3] as f32,
        ))
    }

    pub fn draw(&mut self, content: &Content, dl: &mut DrawList, view: &View, settings: &Settings) {
        let alpha = self.alpha();
        let t = self.time;
        let right = (view.view_proj.inverse() * Vec4::new(1.0, 0.0, 0.5, 1.0)).truncate();
        let _ = right;
        let cam = view.cam;
        let fwd = (self.cams[0].target - self.cams[0].pos).normalize_or(Vec3::Z);
        let (cr, cu) = {
            let r = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
            (r, r.cross(fwd))
        };
        // Fighters.
        let n = self.world.fighters.len();
        let me = self.world.player_id(0);
        for i in 0..n {
            let f = &self.world.fighters[i];
            if !f.alive {
                continue;
            }
            if f.pos.distance(cam) > self.fog_far + 40.0 {
                continue;
            }
            let (pos, yaw) = self.interp(i, alpha);
            // Burrowed ambushers are hidden except during lightning.
            if f.burrowed && self.world.reveal == 0 && f.build.rig != RigKind::Burrower && !f.kind.is_player() {
                continue;
            }
            let mut pose = pose_of(f, content, pos, yaw, t);
            if settings.show_iframes && f.iframes_active() {
                pose.tint = Vec3::new(0.6, 1.4, 2.0);
            }
            if self.world.reveal > 0 && f.loadout.side == Side::Kaiju && !f.kind.is_player() {
                pose.tint *= 1.6;
            }
            self.vis.draw(dl, content, &pose, &self.light);
            let g = self.world.city.ground(pos.x, pos.z).max(self.world.water.min(0.0));
            if !matches!(f.act, Act::Dead) {
                crate::vfx::Vfx::blob(dl, pos, f.radius() * 1.1, g);
            }
            // Weak points on huge bosses: revealed by head senses or while broken.
            if let FighterKind::Boss(b) = &f.kind
                && let Some(bd) = content.boss(b)
            {
                let reveal = matches!(f.act, Act::PostureBroken) || self.world.player(0).is_some_and(|p| p.build.reveal);
                if bd.huge && reveal {
                    for wp in bd.weak_points.iter().filter(|w| f.part_alive(w.slot)) {
                        let p = pos + Vec3::Y * f.height() * wp.height;
                        let pulse = if settings.photosensitive { 1.0 } else { 0.7 + 0.3 * (t * 6.0).sin() };
                        billboard(&mut dl.add, p, 3.0 * pulse, cr, cu, textures::layer("particle"), [255, 230, 120, 200]);
                    }
                }
            }
            let _ = me;
        }
        // Projectiles.
        for p in &self.world.projectiles {
            let c = if self.world.fighters.get(p.owner as usize).is_some_and(|f| f.loadout.side == Side::Kaiju) {
                [180, 255, 120, 255]
            } else {
                [255, 200, 120, 255]
            };
            billboard(&mut dl.add, p.pos, p.radius * 2.2 + 0.6, cr, cu, textures::layer("particle"), c);
        }
        // Pickups: glowing column.
        for pk in &self.world.pickups {
            let h = (t * 3.0).sin() * 0.5;
            billboard(&mut dl.add, pk.pos + Vec3::Y * (2.0 + h), 2.5, cr, cu, textures::layer("particle"), [255, 220, 120, 220]);
            for k in 0..4 {
                billboard(
                    &mut dl.add,
                    pk.pos + Vec3::Y * (4.0 + k as f32 * 3.0),
                    1.0,
                    cr,
                    cu,
                    textures::layer("spark"),
                    [255, 230, 160, 120],
                );
            }
        }
        // Wrecks / carcasses.
        for w in &self.world.wrecks {
            if w.district != self.world.cfg.district || w.pos.distance(cam) > self.fog_far {
                continue;
            }
            let rig = crate::visuals::rig_of(&w.loadout, content);
            let size = crate::visuals::size_of(&w.loadout, content);
            let destroyed = std::array::from_fn(|i| w.loadout.slots[i].is_none());
            let pose = crate::visuals::Pose {
                id: (w.uid & 0xFFFF_FFFF) as u32 | 0x8000_0000,
                loadout: &w.loadout,
                destroyed,
                rig,
                pos: w.pos,
                yaw: (w.uid % 628) as f32 / 100.0,
                size,
                lean: 0.0,
                altitude: 0.0,
                anim: crate::visuals::Anim::Dead(1.0),
                tint: Vec3::splat(0.45),
                alpha: 255,
                bodies: 6,
                burrowed: false,
                moving: 0.0,
                time: t,
            };
            self.vis.draw(dl, content, &pose, &self.light);
            let mine = w.side == self.world.cfg.side;
            if (mine && w.currency > 0) || (!mine && !w.harvested) {
                let c = if mine { [120, 230, 255, 220] } else { [255, 120, 220, 220] };
                billboard(&mut dl.add, w.pos + Vec3::Y * (6.0 + (t * 2.0).sin()), 3.0, cr, cu, textures::layer("particle"), c);
            }
        }
        // Water.
        let flash = self.flash > 0.1;
        district_mesh::water(&mut dl.trans, &self.world.city, self.world.water, t, cam, self.water_tint, flash);
        self.vfx.draw(dl, cr, cu);
    }
}
