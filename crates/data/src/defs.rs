//! Content definition types. Every struct here maps 1:1 onto a RON file under `data/`.
//!
//! Definitions are *static* descriptions. Runtime/persistent state (a specific harvested part
//! with its condition, quirks, origin and memory) lives in `isl_sim::parts::PartInstance`.

use serde::{Deserialize, Serialize};

/// Which war faction an anatomy belongs to. Saints are machines grown on kaiju tissue,
/// Kaiju are monsters from the sea.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Side {
    Saint,
    Kaiju,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Saint => Side::Kaiju,
            Side::Kaiju => Side::Saint,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Side::Saint => "SAINT",
            Side::Kaiju => "KAIJU",
        }
    }
    pub fn disc(self) -> u8 {
        match self {
            Side::Saint => 1,
            Side::Kaiju => 2,
        }
    }
    pub fn currency(self) -> &'static str {
        match self {
            Side::Saint => "SALVAGE",
            Side::Kaiju => "BIOMASS",
        }
    }
}

/// Unified slot list. Saints and Kaiju share the same seven sockets under different names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Slot {
    Frame,
    LeftArm,
    RightArm,
    Legs,
    Core,
    Head,
    Back,
}

impl Slot {
    pub const ALL: [Slot; 7] = [
        Slot::Frame,
        Slot::LeftArm,
        Slot::RightArm,
        Slot::Legs,
        Slot::Core,
        Slot::Head,
        Slot::Back,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name_for(self, side: Side) -> &'static str {
        match (self, side) {
            (Slot::Frame, Side::Saint) => "FRAME",
            (Slot::Frame, Side::Kaiju) => "BODY",
            (Slot::LeftArm, Side::Saint) => "LEFT ARM",
            (Slot::LeftArm, Side::Kaiju) => "LEFT CLAW",
            (Slot::RightArm, Side::Saint) => "RIGHT ARM",
            (Slot::RightArm, Side::Kaiju) => "RIGHT CLAW",
            (Slot::Legs, Side::Saint) => "LEGS",
            (Slot::Legs, Side::Kaiju) => "LOCOMOTION",
            (Slot::Core, Side::Saint) => "REACTOR",
            (Slot::Core, Side::Kaiju) => "HEART",
            (Slot::Head, Side::Saint) => "HEAD",
            (Slot::Head, Side::Kaiju) => "SENSES",
            (Slot::Back, Side::Saint) => "BACKPACK",
            (Slot::Back, Side::Kaiju) => "MUTATION",
        }
    }
    /// Compact label (HUD lists).
    pub fn short_for(self, side: Side) -> &'static str {
        match (self, side) {
            (Slot::Frame, Side::Saint) => "FRAME",
            (Slot::Frame, Side::Kaiju) => "BODY",
            (Slot::LeftArm, Side::Saint) => "L.ARM",
            (Slot::LeftArm, Side::Kaiju) => "L.CLAW",
            (Slot::RightArm, Side::Saint) => "R.ARM",
            (Slot::RightArm, Side::Kaiju) => "R.CLAW",
            (Slot::Legs, Side::Saint) => "LEGS",
            (Slot::Legs, Side::Kaiju) => "LOCO",
            (Slot::Core, Side::Saint) => "REACTOR",
            (Slot::Core, Side::Kaiju) => "HEART",
            (Slot::Head, Side::Saint) => "HEAD",
            (Slot::Head, Side::Kaiju) => "SENSES",
            (Slot::Back, Side::Saint) => "PACK",
            (Slot::Back, Side::Kaiju) => "MUTATION",
        }
    }
    /// Lateral/longitudinal offset of the slot's mass from the frame centre, in metres
    /// (x = right, z = forward). Used for centre-of-mass and lean.
    pub fn mass_offset(self) -> (f32, f32) {
        match self {
            Slot::Frame => (0.0, 0.0),
            Slot::LeftArm => (-1.6, 0.2),
            Slot::RightArm => (1.6, 0.2),
            Slot::Legs => (0.0, 0.0),
            Slot::Core => (0.0, -0.2),
            Slot::Head => (0.0, 0.3),
            Slot::Back => (0.0, -1.2),
        }
    }
    /// Can this slot be torn off in a Sever? The frame is the body itself and the core is buried.
    pub fn severable(self) -> bool {
        !matches!(self, Slot::Frame | Slot::Core)
    }
    /// Fraction of damage to this part that also reaches core HP.
    pub fn core_bleed(self) -> f32 {
        match self {
            Slot::Frame | Slot::Core => 1.0,
            Slot::Head => 0.8,
            _ => 0.6,
        }
    }
}

/// Stamina model chosen by the Reactor (Saint) or Heart (Kaiju).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StaminaKind {
    /// Actions add heat; overheating forces a vent stagger.
    Heat,
    /// Recharges only after a short pause.
    Battery,
    /// Finite reserve per sortie feeds a fast working tank; refills at a hangar.
    Fuel,
    /// Eat buildings or units to refill.
    Hunger,
    /// Damage dealt refills it.
    Blood,
    /// Rhythmic cycle; exhale-timed attacks cost less.
    Breath,
}

impl StaminaKind {
    pub fn natural_side(self) -> Side {
        match self {
            StaminaKind::Heat | StaminaKind::Battery | StaminaKind::Fuel => Side::Saint,
            _ => Side::Kaiju,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            StaminaKind::Heat => "HEAT",
            StaminaKind::Battery => "BATTERY",
            StaminaKind::Fuel => "FUEL",
            StaminaKind::Hunger => "HUNGER",
            StaminaKind::Blood => "BLOOD",
            StaminaKind::Breath => "BREATH",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealKind {
    /// Limited charges, refilled at a Hangar Bay.
    RepairCharges(u8),
    /// Spend banked biomass; slower, but regrows severed parts.
    Regrowth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocoKind {
    Biped,
    Treads,
    Serpent,
    Wings,
    Fins,
    Burrow,
    Swarm,
    Hover,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Locomotion {
    pub kind: LocoKind,
    /// Ground speed in m/s.
    pub speed: f32,
    /// Multiplier while in water.
    #[serde(default = "one")]
    pub swim: f32,
    /// Can cross deep water.
    #[serde(default)]
    pub deep_water: bool,
    /// Can climb rubble / building chunks.
    #[serde(default)]
    pub climb: bool,
}

fn one() -> f32 {
    1.0
}

/// Procedural rig used to animate a body. Kaiju rigs are fully procedural (spines, IK).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RigKind {
    Humanoid,
    Serpent,
    Moth,
    Swarm,
    Burrower,
    Quadruped,
    Vehicle,
    Drone,
    Turret,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PartStats {
    pub hp: f32,
    pub armor: f32,
    /// Weight capacity (frames/bodies only).
    pub capacity: f32,
    /// Damage multiplier contributed by this part (arms mostly).
    pub power: f32,
    /// Posture damage multiplier contribution.
    pub impact: f32,
    /// Max posture contribution.
    pub poise: f32,
    /// Lock-on range in metres (head/senses).
    pub lock_range: f32,
    /// Reveals weak points / part HP on lock.
    pub reveal: bool,
    /// Fog vision bonus in metres.
    pub fog_vision: f32,
    /// Stamina max contribution (core).
    pub stamina: f32,
    /// Stamina regen per second contribution (core).
    pub regen: f32,
    /// Guard efficiency 0..1: fraction of damage blocked when guarding (left arms mostly).
    pub guard: f32,
    /// Body size multiplier (frames/bodies only); 1.0 = 10 m tall.
    pub size: f32,
}

/// Primitive shapes for procedural greybox meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Spike,
    Wedge,
    /// A tapered tube along +Y; used for limbs, tentacles and spines.
    Tube,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prim {
    pub shape: Shape,
    /// Offset from socket, metres.
    #[serde(default)]
    pub at: [f32; 3],
    /// Extents (full size), metres.
    pub size: [f32; 3],
    /// Euler rotation, degrees (XYZ).
    #[serde(default)]
    pub rot: [f32; 3],
    /// Procedural texture name (see `render::textures`).
    #[serde(default = "default_tex")]
    pub tex: String,
    /// Base vertex colour, 0..255.
    #[serde(default = "default_col")]
    pub col: [u8; 3],
    /// Glows (unlit, ignores fog partially) - eyes, reactors, bioluminescence.
    #[serde(default)]
    pub glow: bool,
}

fn default_tex() -> String {
    "plate".into()
}
fn default_col() -> [u8; 3] {
    [160, 160, 160]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Visual {
    pub prims: Vec<Prim>,
}

/// Where a part definition originates. (Instances carry their own, more specific origin.)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Chassis(String),
    Boss(String),
    Enemy(String),
    Secret,
}

/// Behavioural bias inherited from a part's previous owner (see `memory` in the design).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PartMemory {
    /// -1 = always dodges left, +1 = right.
    pub dodge_bias: f32,
    /// Extra aggression (closing distance) 0..1.
    pub aggression: f32,
    /// Tendency to fire attacks early 0..1 (twitch).
    pub twitch: f32,
}

/// One harvestable part. One RON file per part under `data/parts/`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartDef {
    pub id: String,
    pub name: String,
    pub slot: Slot,
    pub side: Side,
    pub weight: f32,
    #[serde(default)]
    pub stats: PartStats,
    /// Move ids this part contributes.
    #[serde(default)]
    pub moves: Vec<String>,
    /// Innate quirks (rare; most quirks come from condition).
    #[serde(default)]
    pub quirks: Vec<String>,
    pub source: Source,
    /// Default memory bias for instances harvested from this part's owner.
    #[serde(default)]
    pub memory: PartMemory,
    #[serde(default)]
    pub locomotion: Option<Locomotion>,
    #[serde(default)]
    pub stamina: Option<StaminaKind>,
    #[serde(default)]
    pub heal: Option<HealKind>,
    #[serde(default)]
    pub rig: Option<RigKind>,
    /// Can parry with this part (left arms, light right arms, some claws).
    #[serde(default)]
    pub parry: bool,
    #[serde(default)]
    pub visual: Visual,
    /// Memory-card complexity points (feeds block cost).
    #[serde(default = "one_u8")]
    pub complexity: u8,
    #[serde(default)]
    pub lore: String,
}

fn one_u8() -> u8 {
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MoveKind {
    Light,
    Heavy,
    Special,
    Bash,
    Grab,
    Projectile,
    /// Area burst centred on the attacker.
    Burst,
    /// Consume a building chunk or downed unit (Hunger).
    Eat,
    /// Spawns units (swarms, hallucinations).
    Summon,
    /// Heretic Form super move.
    Heretic,
}

/// Distinct audio cue per telegraph, so telegraphs never rely on colour alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cue {
    Swing,
    Clank,
    Roar,
    Hiss,
    Charge,
    Bell,
    Click,
    Drone,
    Gurgle,
    Shriek,
}

/// Silhouette change that accompanies a telegraph (shape language, not colour).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Telegraph {
    /// Weapon raised high, glint at tip.
    Raise,
    /// Body coils / crouches.
    Coil,
    /// Rears up, increasing height.
    Rear,
    /// Expanding ground ring.
    Ring,
    /// Spreads limbs/wings wide.
    Spread,
    /// Turns away (backswing).
    Wind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MoveEffect {
    Burn,
    Spores,
    Constrict,
    Tether,
    /// Copies the struck part's move into the attacker's free slot (ST. NULL).
    Mimic,
    /// Leaves a healing flood pool (TIDE-WIDOW).
    Flood,
    /// Splits off swarm bodies (PALE CHOIR).
    Split,
    /// Recombine swarm bodies to heal.
    Recombine,
    /// Burrow underground then erupt.
    Burrow,
    /// Pull self toward target (grapple hook).
    Grapple,
    /// Drain Machine-blood/Sync stability.
    Static,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileDef {
    pub speed: f32,
    pub radius: f32,
    /// Lifetime in ticks.
    pub life: u16,
    #[serde(default)]
    pub gravity: f32,
    #[serde(default = "one_u8")]
    pub count: u8,
    /// Spread in degrees.
    #[serde(default)]
    pub spread: f32,
    #[serde(default)]
    pub homing: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Avoid {
    /// Can be dodged through with i-frames.
    Dodge,
    /// Can be outrun.
    Distance,
    /// Both.
    Any,
}

/// A single attack or action. Timings are in 60 Hz ticks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveDef {
    pub id: String,
    pub name: String,
    pub kind: MoveKind,
    pub windup: u16,
    pub active: u16,
    pub recovery: u16,
    #[serde(default)]
    pub damage: f32,
    #[serde(default)]
    pub posture: f32,
    #[serde(default)]
    pub stamina: f32,
    /// Hitbox centre distance in front of the attacker at mid-swing, metres (scaled by size).
    #[serde(default = "default_reach")]
    pub reach: f32,
    #[serde(default = "default_radius")]
    pub radius: f32,
    /// Horizontal sweep arc in degrees (0 = thrust).
    #[serde(default)]
    pub arc: f32,
    /// Forward lunge distance over windup+active, metres.
    #[serde(default)]
    pub lunge: f32,
    #[serde(default)]
    pub hyper_armor: bool,
    #[serde(default)]
    pub projectile: Option<ProjectileDef>,
    #[serde(default)]
    pub effect: Option<MoveEffect>,
    /// Extra hold before release; delayed feint. Always the same length so it is learnable.
    #[serde(default)]
    pub feint: u16,
    #[serde(default = "default_cue")]
    pub cue: Cue,
    #[serde(default = "default_tele")]
    pub telegraph: Telegraph,
    #[serde(default = "default_avoid")]
    pub avoid: Avoid,
    /// Can be guarded (unblockables must be Dodge/Distance avoidable).
    #[serde(default = "yes")]
    pub blockable: bool,
    #[serde(default)]
    pub lore: String,
}

fn default_reach() -> f32 {
    3.0
}
fn default_radius() -> f32 {
    2.0
}
fn default_cue() -> Cue {
    Cue::Swing
}
fn default_tele() -> Telegraph {
    Telegraph::Raise
}
fn default_avoid() -> Avoid {
    Avoid::Any
}
fn yes() -> bool {
    true
}

impl MoveDef {
    pub fn total(&self) -> u16 {
        self.windup + self.feint + self.active + self.recovery
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stat {
    Damage,
    Hp,
    Weight,
    Posture,
    PostureDamage,
    StaminaMax,
    StaminaRegen,
    Speed,
    TurnSpeed,
    Armor,
    LockRange,
    SwimSpeed,
    LandSpeed,
    HealPower,
    Guard,
    AttackSpeed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DodgeDir {
    Left,
    Right,
    Back,
    Forward,
    Any,
}

/// The atomic effects quirks, omens, traits and conditions are made of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// Multiply a stat (applies to the whole build).
    StatMul(Stat, f32),
    /// Chance per attack to skip part of the windup.
    EarlyFire { chance: f32, skip: f32 },
    /// Damage multiplier against fighters built from the part's former owner.
    Grudge(f32),
    /// Stamina drains while idle (per second).
    IdleDrain(f32),
    /// Extra i-frame ticks for dodges in a direction.
    DodgeIframes(DodgeDir, i8),
    /// Extra parry window ticks.
    ParryWindow(i8),
    /// Damage multiplier while HP below threshold.
    Desperate { below: f32, mul: f32 },
    /// Heal a fraction of damage dealt.
    Leech(f32),
    /// Attacks inflict burn.
    Ignite,
    /// Lose this many HP per second.
    Bleed(f32),
    /// Posture recovers faster.
    Steady(f32),
    /// The part cannot regrow (machine-blood).
    NoRegrow,
    /// Low-sync hallucinations (enemy decoys).
    Hallucinate(f32),
    /// Control drift magnitude.
    Drift(f32),
    /// Currency gain multiplier.
    Greed(f32),
    /// More harvest drops.
    Harvester(f32),
    /// Lock-on disabled.
    NoLock,
    /// HUD hidden.
    NoHud,
    /// One-hit death.
    GlassHull,
    /// Hunger drain multiplier.
    Famine(f32),
    /// Enemies copy the player's last move.
    Echo,
    /// All districts partly flooded.
    Riptide,
    /// Enemy damage multiplier.
    EnemyDamage(f32),
    /// Enemy HP multiplier.
    EnemyHp(f32),
    /// Enemy speed multiplier.
    EnemySpeed(f32),
    /// Heals restore less / more.
    HealMul(f32),
    /// Death drop is lost immediately.
    NoRecovery,
    /// Flavour/marker only (Rusted, Crumbling, Corrupted chain markers).
    Marker(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Polarity {
    Positive,
    Mixed,
    Negative,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuirkDef {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub polarity: Polarity,
    pub effects: Vec<Effect>,
    /// Restricted to one anatomy (e.g. Remembers the Sea only on Saint parts).
    #[serde(default)]
    pub side: Option<Side>,
    /// Only drops from imported (memory card code) parts.
    #[serde(default)]
    pub import_only: bool,
    /// Never rolled randomly (conditions, endings, corruption).
    #[serde(default)]
    pub special: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unlock {
    Always,
    /// Sever the heads of all listed chassis while playing `side`.
    SeverHeads { of: Vec<String>, while_playing: Side },
    /// Harvest the hearts of all listed chassis while playing `side`.
    HarvestCores { of: Vec<String>, while_playing: Side },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LicenseRank {
    pub rank: u8,
    pub reward: String,
    /// Optional permanent bonus (counts toward the +10% meta cap).
    #[serde(default)]
    pub bonus: Option<Effect>,
    #[serde(default)]
    pub palette: Option<[[u8; 3]; 2]>,
    /// Requires N wins against this chassis from the other side.
    #[serde(default)]
    pub wins_against: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChassisDef {
    pub id: String,
    pub name: String,
    pub side: Side,
    pub playstyle: String,
    /// Natural rival on the other side.
    pub rival: String,
    /// part ids in slot order: Frame, LeftArm, RightArm, Legs, Core, Head, Back.
    pub parts: [String; 7],
    #[serde(default)]
    pub alt_parts: Vec<[String; 7]>,
    pub palette: [[u8; 3]; 2],
    pub unlock: Unlock,
    pub signature: String,
    #[serde(default)]
    pub signature_effect: Option<Effect>,
    pub ranks: Vec<LicenseRank>,
    pub lore: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Archetype {
    /// Closes distance, swings.
    Brute,
    /// Keeps distance, shoots.
    Gunner,
    /// Fast, weak, attacks in packs.
    Swarmer,
    /// Flies, strafes.
    Flyer,
    /// Hides underground and ambushes.
    Ambusher,
    /// Static, long-range, high damage.
    Battery,
    /// Bosses use scripted phases.
    Boss,
    /// Inert training dummy.
    Dummy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyDef {
    pub id: String,
    pub name: String,
    pub side: Side,
    pub archetype: Archetype,
    pub parts: [String; 7],
    #[serde(default = "onef")]
    pub hp_mul: f32,
    #[serde(default = "onef")]
    pub scale: f32,
    pub currency: u32,
    /// Ticks of reaction delay (AI perceives a delayed snapshot - no input reading).
    #[serde(default = "default_react")]
    pub reaction: u16,
    #[serde(default)]
    pub color: [u8; 3],
    pub bestiary: String,
}

fn onef() -> f32 {
    1.0
}
fn default_react() -> u16 {
    14
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BossPhase {
    /// Phase begins when core HP fraction <= this.
    pub at_hp: f32,
    /// (move id, weight). Move ids must exist in moves.ron.
    pub moves: Vec<(String, u32)>,
    #[serde(default = "onef")]
    pub speed: f32,
    #[serde(default)]
    pub line: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeakPoint {
    pub slot: Slot,
    /// Height on the body 0..1.
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BossDef {
    pub id: String,
    pub name: String,
    pub title: String,
    pub side: Side,
    /// If this boss is a starting chassis (rival-licence wins count against it).
    #[serde(default)]
    pub chassis: Option<String>,
    pub parts: [String; 7],
    pub scale: f32,
    #[serde(default = "onef")]
    pub hp_mul: f32,
    /// Huge bosses get the scale-combat camera and climbable weak points.
    #[serde(default)]
    pub huge: bool,
    #[serde(default)]
    pub weak_points: Vec<WeakPoint>,
    pub phases: Vec<BossPhase>,
    pub currency: u32,
    #[serde(default = "default_react")]
    pub reaction: u16,
    pub intro: String,
    pub bestiary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum DistrictState {
    Fortified,
    Contested,
    Flooded,
    Ruined,
}

impl DistrictState {
    pub const ALL: [DistrictState; 4] = [
        DistrictState::Fortified,
        DistrictState::Contested,
        DistrictState::Flooded,
        DistrictState::Ruined,
    ];
    pub fn name(self) -> &'static str {
        match self {
            DistrictState::Fortified => "FORTIFIED",
            DistrictState::Contested => "CONTESTED",
            DistrictState::Flooded => "FLOODED",
            DistrictState::Ruined => "RUINED",
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Reverb {
    Room,
    Studio,
    Hall,
    SpaceEcho,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Landmark {
    Docks,
    Cathedral,
    Smokestacks,
    Railyard,
    Lighthouse,
    Market,
    Seawall,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateVariant {
    pub fog: [u8; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub ambient: [u8; 3],
    pub sun: [u8; 3],
    /// Water level, metres.
    pub water: f32,
    /// Fraction of buildings pre-destroyed.
    pub ruin: f32,
    /// Enemy ids faced when playing Saint (kaiju-side enemies).
    pub saint_enemies: Vec<String>,
    /// Enemy ids faced when playing Kaiju (saint-side enemies).
    pub kaiju_enemies: Vec<String>,
    /// Music layer intensity bias.
    #[serde(default)]
    pub mood: f32,
    /// Fortifications (barricades, AA) present.
    #[serde(default)]
    pub fortifications: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MusicDef {
    pub tempo: f32,
    /// MIDI root note.
    pub root: u8,
    /// Scale intervals (semitones).
    pub scale: Vec<u8>,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DistrictDef {
    pub id: String,
    pub name: String,
    pub landmark: Landmark,
    /// War-map position, 0..1.
    pub map_pos: [f32; 2],
    pub size: f32,
    pub block: f32,
    pub seed: u64,
    /// Fraction of map along -Z that is sea.
    pub sea: f32,
    pub palette: Vec<[u8; 3]>,
    pub reverb: Reverb,
    pub music: MusicDef,
    /// Indexed by DistrictState order.
    pub states: [StateVariant; 4],
    /// Boss faced when playing Saint (a kaiju boss).
    pub saint_boss: String,
    /// Boss faced when playing Kaiju (a saint boss).
    pub kaiju_boss: String,
    #[serde(default)]
    pub finale: bool,
    /// Initial state at the start of a war.
    pub start: DistrictState,
    pub lore: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OmenDef {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub reward: f32,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscRotTier {
    pub tier: u8,
    pub rule: String,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContractGoal {
    SeverSlot(Slot, u8),
    NoHeal,
    NoDamage,
    KillCount(u8),
    Parries(u8),
    UnderTime(u32),
    EatBuildings(u8),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContractDef {
    pub id: String,
    pub name: String,
    pub side: Option<Side>,
    pub goal: ContractGoal,
    /// Quirk id granted (applied to a fresh random part harvested on completion).
    pub reward_quirk: String,
    pub reward_currency: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    DemoDisc,
    Evacuation,
    CathedralBell,
    NightOfStatic,
    Blackout,
    SupplyDrop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventDef {
    pub id: String,
    pub name: String,
    pub kind: EventKind,
    pub chance: f32,
    pub desc: String,
    /// Optional (enemy faced when playing Saint, enemy faced when playing Kaiju).
    #[serde(default)]
    pub spawn: Option<(String, String)>,
    /// Optional cursed quirk attached to the spawn's drops.
    #[serde(default)]
    pub quirk: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConditionKind {
    HarborFlooded,
    OneSaintPerSide,
    AllContested,
    Blackout,
    EternalNight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionDef {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub kind: ConditionKind,
    /// War Record milestone required (wars finished).
    pub wars: u32,
    #[serde(default)]
    pub endings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EndingDef {
    pub id: String,
    pub name: String,
    pub card: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct LoreDef {
    pub loading: Vec<String>,
    pub intro: Vec<String>,
    pub sync_glitches: Vec<String>,
    pub disc3: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TuningDef {
    /// Seconds per in-game hour of tide clock when resting is the only driver.
    pub parry_window: u16,
    pub steady_hands_parry: u16,
    pub dodge_iframes: [u16; 3],
    pub dodge_distance: [f32; 3],
    pub dodge_ticks: [u16; 3],
    pub sever_window: u16,
    pub posture_break_ticks: u16,
    pub vent_ticks: u16,
    pub heal_ticks: u16,
    pub repair_heal: f32,
    pub regrowth_heal: f32,
    pub regrowth_cost: u32,
    pub meta_cap: f32,
    pub memory_blocks: u8,
    pub min_boss_windup: u16,
    pub old_bones_speed: f32,
}
