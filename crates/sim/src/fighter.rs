//! Fighters (player, enemies, bosses, nemeses, allies, hallucinations), their commands,
//! action states and the events the simulation emits.

use crate::ai::Brain;
use crate::build::{Loadout, MoveRef, ResolvedBuild};
use crate::parts::Condition;
use crate::stamina::Stamina;
use glam::{Vec2, Vec3};
use isl_data::*;
use serde::{Deserialize, Serialize};

/// One tick of intent. The player's controller and every AI produce the same thing, so AI can
/// never do anything the player can't, and nemeses replay habits through the same interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    /// Desired movement in world XZ (x, z), length <= 1.
    pub move_dir: Vec2,
    pub light: bool,
    pub heavy: bool,
    /// Held.
    pub guard: bool,
    pub parry: bool,
    pub dodge: bool,
    pub special: bool,
    pub heal: bool,
    pub interact: bool,
    pub lock: bool,
    pub lock_next: bool,
    pub lock_prev: bool,
    pub lock_part: bool,
    pub heretic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    Player,
    Player2,
    Enemy,
    Ally,
}

impl Team {
    pub fn hostile(self, o: Team) -> bool {
        use Team::*;
        match (self, o) {
            (Player, Enemy) | (Enemy, Player) | (Ally, Enemy) | (Enemy, Ally) | (Player, Player2) | (Player2, Player) => true,
            (Player2, Enemy) | (Enemy, Player2) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FighterKind {
    Player(u8),
    Enemy(String),
    Boss(String),
    Nemesis { name: String, rival: Option<String> },
    Ally(String),
    Hallucination,
    Verse,
}

impl FighterKind {
    pub fn is_player(&self) -> bool {
        matches!(self, FighterKind::Player(_))
    }
    pub fn is_bossish(&self) -> bool {
        matches!(self, FighterKind::Boss(_) | FighterKind::Nemesis { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DodgeKind {
    Dash,
    Hop,
    Slide,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Act {
    Idle,
    Attack { mv: MoveRef, hit: Vec<u32>, skip: u16, chunks_hit: bool },
    Guard,
    Parry,
    Dodge { dir: Vec2, kind: DodgeKind, iframes: u16, dist: f32, ticks: u16, label: DodgeDir },
    Stagger { dur: u16 },
    PostureBroken,
    Vent,
    Heal,
    Severing { target: u32, slot: Slot },
    Climbing { target: u32, slot: Slot, height: f32 },
    Grabbed { by: u32 },
    Dead,
}

impl Act {
    pub fn name(&self) -> &'static str {
        match self {
            Act::Idle => "idle",
            Act::Attack { .. } => "attack",
            Act::Guard => "guard",
            Act::Parry => "parry",
            Act::Dodge { .. } => "dodge",
            Act::Stagger { .. } => "stagger",
            Act::PostureBroken => "broken",
            Act::Vent => "vent",
            Act::Heal => "heal",
            Act::Severing { .. } => "sever",
            Act::Climbing { .. } => "climb",
            Act::Grabbed { .. } => "grabbed",
            Act::Dead => "dead",
        }
    }
    pub fn busy(&self) -> bool {
        !matches!(self, Act::Idle | Act::Guard)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub burn: u16,
    pub spores: u16,
    pub tether: u16,
    pub static_: u16,
    pub hit_flash: u8,
    /// Grab: ticks remaining.
    pub grabbed: u16,
    pub flood_heal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockOn {
    pub target: u32,
    pub slot: Slot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fighter {
    pub id: u32,
    pub team: Team,
    pub kind: FighterKind,
    /// Identity used by Old Grudge and licence win tracking (chassis/enemy/boss id).
    pub tag: String,
    pub name: String,
    pub loadout: Loadout,
    pub build: ResolvedBuild,
    pub destroyed: [bool; 7],
    pub part_hp: [f32; 7],
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub hp: f32,
    pub posture: f32,
    pub posture_idle: u16,
    pub stamina: Stamina,
    pub act: Act,
    pub act_t: u32,
    pub lock: Option<LockOn>,
    pub heal_charges: u8,
    pub heal_max: u8,
    pub currency: u32,
    pub status: Status,
    pub scale: f32,
    pub dmg_mul: f32,
    pub speed_mul: f32,
    pub ai: Option<Brain>,
    pub boss_phase: usize,
    pub last_move: Option<String>,
    pub combo: u8,
    pub buffered: Option<(Buffered, u8)>,
    pub alive: bool,
    pub despawn: u16,
    pub hallucination: bool,
    pub mimic: Option<MoveRef>,
    pub in_water: f32,
    pub altitude: f32,
    pub burrowed: bool,
    pub spawn_pos: Vec3,
    pub step_phase: f32,
    pub last_attacker: Option<u32>,
    /// Swarm body count (Pale Choir); renders N bodies, HP scales with it.
    pub bodies: u8,
    pub owner: Option<u32>,
    /// Animation speed (AttackSpeed quirks, Old Bones assist).
    pub anim_speed: f32,
    pub anim_acc: f32,
    /// Effects applied on top of the loadout (omens, traits, meta bonuses).
    pub extra_fx: Vec<Effect>,
    /// Tick the current act started (identifies a swing for AI reactions).
    pub act_start: u64,
    pub currency_value: u32,
    pub lifetime: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Buffered {
    Light,
    Heavy,
    Special,
    Dodge,
    Parry,
    Heal,
    Bash,
    Heretic,
}

impl Fighter {
    pub fn new(
        id: u32,
        team: Team,
        kind: FighterKind,
        tag: String,
        name: String,
        loadout: Loadout,
        build: ResolvedBuild,
        pos: Vec3,
        scale: f32,
    ) -> Fighter {
        let part_hp = std::array::from_fn(|i| build.parts[i].as_ref().map_or(0.0, |p| p.hp));
        let destroyed = std::array::from_fn(|i| build.parts[i].is_none());
        let heal_max = match build.heal {
            HealKind::RepairCharges(n) => n,
            HealKind::Regrowth => 3,
        };
        let stamina = Stamina::new(build.stamina_kind, build.stamina_max, build.stamina_regen);
        let bodies = if build.rig == RigKind::Swarm { 12 } else { 1 };
        Fighter {
            id,
            team,
            kind,
            tag,
            name,
            hp: build.hp_max,
            posture: 0.0,
            posture_idle: 0,
            stamina,
            act: Act::Idle,
            act_t: 0,
            lock: None,
            heal_charges: heal_max,
            heal_max,
            currency: 0,
            status: Status::default(),
            scale,
            dmg_mul: 1.0,
            speed_mul: 1.0,
            ai: None,
            boss_phase: 0,
            last_move: None,
            combo: 0,
            buffered: None,
            alive: true,
            despawn: 0,
            hallucination: false,
            mimic: None,
            in_water: 0.0,
            altitude: 0.0,
            burrowed: false,
            spawn_pos: pos,
            step_phase: 0.0,
            last_attacker: None,
            bodies,
            owner: None,
            anim_speed: 1.0,
            anim_acc: 0.0,
            extra_fx: vec![],
            act_start: 0,
            currency_value: 0,
            lifetime: 0,
            loadout,
            build,
            destroyed,
            part_hp,
            pos,
            vel: Vec3::ZERO,
            yaw: 0.0,
        }
    }
    pub fn radius(&self) -> f32 {
        self.build.radius * self.scale
    }
    pub fn height(&self) -> f32 {
        self.build.height * self.scale
    }
    pub fn size(&self) -> f32 {
        self.build.size * self.scale
    }
    pub fn hp_frac(&self) -> f32 {
        (self.hp / self.build.hp_max).clamp(0.0, 1.0)
    }
    pub fn posture_frac(&self) -> f32 {
        (self.posture / self.build.posture_max).clamp(0.0, 1.0)
    }
    pub fn center(&self) -> Vec3 {
        self.pos + Vec3::Y * (self.height() * 0.45 + self.altitude)
    }
    pub fn forward(&self) -> Vec3 {
        crate::geom::yaw_dir(self.yaw)
    }
    pub fn invulnerable(&self) -> bool {
        match &self.act {
            Act::Dodge { iframes, .. } => self.act_t < *iframes as u32,
            Act::Severing { .. } | Act::Dead => true,
            _ => self.burrowed,
        }
    }
    /// Is the dodge i-frame window currently active (for the training-arena visualiser)?
    pub fn iframes_active(&self) -> bool {
        matches!(&self.act, Act::Dodge { iframes, .. } if self.act_t < *iframes as u32)
    }
    pub fn part_alive(&self, s: Slot) -> bool {
        !self.destroyed[s.index()]
    }
    pub fn alive_parts(&self) -> Vec<Slot> {
        Slot::ALL.into_iter().filter(|s| self.part_alive(*s)).collect()
    }
}

/// Everything the outside world (audio, renderer, ghost recorder, campaign) needs to hear about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SimEvent {
    AttackStart { id: u32, mv: String, cue: Cue, telegraph: Telegraph, feint: u16, pos: Vec3, boss: bool },
    Hit { attacker: u32, target: u32, slot: Slot, dmg: f32, pos: Vec3, blocked: bool, heavy: bool },
    Parried { by: u32, attacker: u32, pos: Vec3 },
    ParryAttempt { id: u32 },
    Dodge { id: u32, dir: DodgeDir },
    DodgeEvaded { id: u32, attacker: u32 },
    Guard { id: u32 },
    Footstep { id: u32, pos: Vec3, weight: f32 },
    PartDestroyed { id: u32, slot: Slot, pos: Vec3 },
    Severed { by: u32, target: u32, slot: Slot, pos: Vec3 },
    PostureBroken { id: u32, pos: Vec3 },
    Vent { id: u32 },
    Heal { id: u32, hp_frac: f32 },
    Death { id: u32, pos: Vec3, tag: String, by_player: bool, side: Side },
    PlayerDied { id: u32, pos: Vec3 },
    Loot { def: String, condition: Condition, pos: Vec3 },
    Currency { amount: u32 },
    ChunkDestroyed { building: u32, chunk: u32, pos: Vec3, eaten: bool },
    ProjectileFired { id: u32, pos: Vec3 },
    Explosion { pos: Vec3, radius: f32 },
    BossPhase { id: u32, phase: usize, line: String },
    BossAwake { id: u32 },
    Rest { checkpoint: usize },
    WreckRecovered { amount: u32 },
    WreckHarvested { parts: Vec<String> },
    LockOn { id: u32, target: Option<u32> },
    Shake { amount: f32, pos: Vec3 },
    Grabbed { by: u32, target: u32 },
    Climb { id: u32, target: u32 },
    Rip { id: u32, target: u32, slot: Slot },
    Thrown { id: u32 },
    Hallucination { id: u32 },
    Mimic { id: u32, mv: String },
    Lightning { pos: Vec3 },
    Eat { id: u32, food: f32 },
    Hazard { pos: Vec3, radius: f32, kind: HazardKind },
    StatusApplied { id: u32, effect: MoveEffect },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HazardKind {
    Fire,
    Spores,
    Flood,
    Smoke,
}
