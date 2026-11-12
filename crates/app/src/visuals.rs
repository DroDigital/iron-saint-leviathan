//! Procedural rigs and animation. Saints use a humanoid socket rig with keyed swing poses;
//! kaiju are fully procedural (trail-following spine segments, flapping wings, swarm bodies,
//! burrowers) - no humanoid animation library needed, and nothing looks like a stock pack.

use glam::{Mat4, Quat, Vec3};
use isl_data::*;
use isl_render::meshgen::{self, Mesh};
use isl_render::{DrawList, Lighting, push_mesh};
use isl_sim::build::Loadout;
use isl_sim::fighter::{Act, DodgeKind, Fighter};
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anim {
    Idle,
    Walk(f32),
    /// telegraph, windup progress 0..1, active progress 0..1, recovery progress 0..1, arc (rad), slot
    Attack {
        tele: Telegraph,
        w: f32,
        a: f32,
        r: f32,
        arc: f32,
        slot: Slot,
        kind: MoveKind,
    },
    Guard,
    Parry(f32),
    Dodge {
        kind: DodgeKind,
        k: f32,
    },
    Stagger(f32),
    Broken(f32),
    Dead(f32),
    Heal(f32),
    Vent(f32),
    Climb,
    Grabbed,
}

pub struct Pose<'a> {
    pub id: u32,
    pub loadout: &'a Loadout,
    pub destroyed: [bool; 7],
    pub rig: RigKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub size: f32,
    pub lean: f32,
    pub altitude: f32,
    pub anim: Anim,
    pub tint: Vec3,
    pub alpha: u8,
    pub bodies: u8,
    pub burrowed: bool,
    pub moving: f32,
    pub time: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Group {
    All,
    Left,
    Right,
}

#[derive(Default)]
pub struct VisState {
    pub trail: VecDeque<Vec3>,
}

#[derive(Default)]
pub struct Visuals {
    meshes: HashMap<(String, Group), Mesh>,
    pub states: HashMap<u32, VisState>,
}

fn rot(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_quat(Quat::from_euler(glam::EulerRot::YXZ, y, x, z))
}
fn tr(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

pub fn anim_of(f: &Fighter, content: &Content) -> Anim {
    let t = f.act_t as f32;
    match &f.act {
        Act::Idle | Act::Guard if matches!(f.act, Act::Guard) => Anim::Guard,
        Act::Idle => {
            let sp = glam::Vec2::new(f.vel.x, f.vel.z).length();
            if sp > 0.6 { Anim::Walk(f.step_phase) } else { Anim::Idle }
        }
        Act::Attack { mv, skip, .. } => match content.mv(&mv.id) {
            Some(m) => {
                let w = (m.windup + m.feint).saturating_sub(*skip) as f32;
                let a = m.active as f32;
                let r = m.recovery as f32;
                Anim::Attack {
                    tele: m.telegraph,
                    w: (t / w.max(1.0)).min(1.0),
                    a: ((t - w) / a.max(1.0)).clamp(0.0, 1.0),
                    r: ((t - w - a) / r.max(1.0)).clamp(0.0, 1.0),
                    arc: m.arc.to_radians(),
                    slot: mv.slot,
                    kind: m.kind,
                }
            }
            None => Anim::Idle,
        },
        Act::Guard => Anim::Guard,
        Act::Parry => Anim::Parry((t / 12.0).min(1.0)),
        Act::Dodge { kind, ticks, .. } => Anim::Dodge { kind: *kind, k: t / *ticks as f32 },
        Act::Stagger { dur } => Anim::Stagger(t / *dur as f32),
        Act::PostureBroken => Anim::Broken((t / 20.0).min(1.0)),
        Act::Vent => Anim::Vent(t),
        Act::Heal => Anim::Heal(t),
        Act::Severing { .. } => Anim::Attack {
            tele: Telegraph::Raise,
            w: 1.0,
            a: (t / 40.0).min(1.0),
            r: 0.0,
            arc: 0.0,
            slot: Slot::RightArm,
            kind: MoveKind::Heavy,
        },
        Act::Climbing { .. } => Anim::Climb,
        Act::Grabbed { .. } => Anim::Grabbed,
        Act::Dead => Anim::Dead((t / 50.0).min(1.0)),
    }
}

impl Visuals {
    fn mesh(&mut self, content: &Content, def: &str, g: Group) -> Option<&Mesh> {
        let key = (def.to_string(), g);
        if !self.meshes.contains_key(&key) {
            let d = content.part(def)?;
            let prims: Vec<Prim> = d
                .visual
                .prims
                .iter()
                .filter(|p| match g {
                    Group::All => true,
                    Group::Left => p.at[0] < -0.01,
                    Group::Right => p.at[0] >= -0.01,
                })
                .cloned()
                .collect();
            self.meshes.insert(key.clone(), meshgen::part_mesh(&prims));
        }
        self.meshes.get(&key)
    }

    #[allow(clippy::too_many_arguments)]
    fn put(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, slot: Slot, g: Group, m: Mat4, light: &Lighting) {
        if p.destroyed[slot.index()] {
            return;
        }
        let Some(inst) = p.loadout.get(slot) else { return };
        let def = inst.def.clone();
        if let Some(mesh) = self.mesh(content, &def, g) {
            let mesh = mesh.clone();
            push_mesh(dl, &mesh, m, light, p.tint, p.alpha);
        }
    }

    /// Count of triangles a pose would emit (for budgets / tests).
    pub fn draw(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, light: &Lighting) {
        let root =
            Mat4::from_translation(p.pos + Vec3::Y * p.altitude) * Mat4::from_rotation_y(p.yaw) * Mat4::from_scale(Vec3::splat(p.size));
        match p.rig {
            RigKind::Humanoid => self.humanoid(dl, content, p, root, light),
            RigKind::Serpent => self.serpent(dl, content, p, light),
            RigKind::Moth => self.moth(dl, content, p, root, light),
            RigKind::Swarm => self.swarm(dl, content, p, root, light),
            RigKind::Burrower => self.burrower(dl, content, p, root, light),
            RigKind::Quadruped => self.quadruped(dl, content, p, root, light),
            RigKind::Vehicle => self.vehicle(dl, content, p, root, light),
            RigKind::Drone => self.drone(dl, content, p, root, light),
            RigKind::Turret => self.turret(dl, content, p, root, light),
        }
    }

    fn humanoid(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        // Pose parameters.
        let (mut hip_y, mut pitch, mut twist, mut roll) = (4.6, 0.0f32, 0.0f32, p.lean);
        let (mut ra, mut la) = ((0.0f32, 0.0f32, 0.0f32), (0.0f32, 0.0f32, 0.0f32)); // (pitch, yaw, roll)
        let (mut legl, mut legr) = (0.0f32, 0.0f32);
        let mut fall = 0.0;
        let breathe = (p.time * 1.6 + p.id as f32).sin() * 0.04;
        match p.anim {
            Anim::Idle => {
                hip_y += breathe;
                ra.0 = 0.1;
                la.0 = 0.1;
            }
            Anim::Walk(ph) => {
                let s = (ph * std::f32::consts::PI).sin();
                legl = s * 0.55;
                legr = -s * 0.55;
                ra.0 = -s * 0.35;
                la.0 = s * 0.35;
                hip_y += (ph * std::f32::consts::TAU).cos().abs() * 0.25;
                pitch = 0.08;
            }
            Anim::Attack { tele, w, a, r, arc, slot, kind } => {
                let back = 1.0 - r;
                let arm = if slot == Slot::LeftArm { &mut la } else { &mut ra };
                match kind {
                    MoveKind::Bash => {
                        arm.0 = -0.4 * w - 1.2 * a * back;
                        pitch = 0.25 * w * back;
                    }
                    MoveKind::Burst | MoveKind::Summon | MoveKind::Heretic => {
                        la.2 = -1.3 * w * back;
                        ra.2 = 1.3 * w * back;
                        la.0 = -1.6 * a * back;
                        ra.0 = -1.6 * a * back;
                        hip_y -= 1.0 * w * back;
                    }
                    _ => {
                        if arc > 0.5 {
                            // Horizontal sweep.
                            arm.0 = -1.5 * w.max(a) * back;
                            arm.1 = (arc * 0.5) * w - arc * a;
                            twist = (0.5 * w - 1.0 * a) * back;
                        } else {
                            // Overhead chop / thrust.
                            arm.0 = (-2.5 * w + 3.0 * a) * back;
                            pitch = (-0.15 * w + 0.35 * a) * back;
                        }
                    }
                }
                // Telegraph silhouettes (shape language, not colour).
                let tw = w * (1.0 - a);
                match tele {
                    Telegraph::Coil => {
                        hip_y -= 1.2 * tw;
                        pitch += 0.45 * tw;
                    }
                    Telegraph::Rear => {
                        pitch -= 0.35 * tw;
                        hip_y += 0.6 * tw;
                    }
                    Telegraph::Spread => {
                        la.2 = la.2.min(-1.2 * tw);
                        ra.2 = ra.2.max(1.2 * tw);
                    }
                    Telegraph::Wind => twist += 0.9 * tw,
                    Telegraph::Ring => hip_y -= 0.8 * tw,
                    Telegraph::Raise => {}
                }
            }
            Anim::Guard => {
                la = (-1.4, 0.4, -0.2);
                pitch = 0.12;
                hip_y -= 0.3;
            }
            Anim::Parry(k) => {
                la = (-1.6 + k * 0.6, 0.8 - k, 0.0);
                twist = -0.3 * (1.0 - k);
            }
            Anim::Dodge { kind, k } => match kind {
                DodgeKind::Dash => pitch = 0.4 * (1.0 - k),
                DodgeKind::Hop => hip_y += (k * std::f32::consts::PI).sin() * 2.5,
                DodgeKind::Slide => {
                    hip_y -= 1.3 * (1.0 - k);
                    roll += 0.35 * (1.0 - k);
                    la.0 = -1.0;
                }
            },
            Anim::Stagger(k) => {
                pitch = -0.4 * (1.0 - k);
                ra.0 = 0.6;
                la.0 = 0.6;
            }
            Anim::Broken(k) => {
                hip_y -= 2.0 * k;
                pitch = 0.6 * k;
                ra.0 = 0.4;
                la.0 = 0.4;
                legl = -0.9 * k;
                legr = 0.4 * k;
            }
            Anim::Vent(t) => {
                pitch = 0.3 + (t * 0.3).sin() * 0.05;
                hip_y -= 0.6;
            }
            Anim::Heal(t) => {
                la.0 = -0.6;
                ra.0 = -0.6;
                hip_y -= 0.4 + (t * 0.2).sin() * 0.1;
            }
            Anim::Dead(k) => {
                fall = k;
            }
            Anim::Climb => {
                ra.0 = -2.6;
                la.0 = -2.6;
            }
            Anim::Grabbed => {
                ra.2 = 0.6;
                la.2 = -0.6;
                pitch = -0.2;
            }
        }
        let root = if fall > 0.0 { root * tr(0.0, -2.0 * fall, 0.0) * rot(-1.45 * fall, 0.0, 0.2 * fall) } else { root };
        let hip = root * tr(0.0, hip_y, 0.0);
        let torso = hip * rot(pitch, twist, -roll);
        self.put(dl, content, p, Slot::Frame, Group::All, torso, light);
        self.put(dl, content, p, Slot::Legs, Group::Left, hip * rot(legl, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::Legs, Group::Right, hip * rot(legr, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::Head, Group::All, torso * tr(0.0, 3.8, 0.0), light);
        self.put(dl, content, p, Slot::Core, Group::All, torso * tr(0.0, 2.3, 1.05), light);
        self.put(dl, content, p, Slot::Back, Group::All, torso * tr(0.0, 2.4, -1.0), light);
        self.put(dl, content, p, Slot::LeftArm, Group::All, torso * tr(-2.1, 3.2, 0.0) * rot(la.0, la.1, la.2), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, torso * tr(2.1, 3.2, 0.0) * rot(ra.0, ra.1, ra.2), light);
    }

    fn serpent(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, light: &Lighting) {
        let n = 9usize;
        let spacing = 2.3 * p.size;
        let head = p.pos + Vec3::Y * (1.6 * p.size + p.altitude);
        let st = self.states.entry(p.id).or_default();
        let fwd = Vec3::new(p.yaw.sin(), 0.0, p.yaw.cos());
        // A reused fighter slot (or a teleport) leaves a stale trail: rebuild it.
        if st.trail.front().is_some_and(|f| f.distance(head) > spacing * 8.0) {
            st.trail.clear();
        }
        if st.trail.is_empty() {
            for i in 0..(n * 4) {
                st.trail.push_back(head - fwd * (i as f32 * spacing * 0.25));
            }
        }
        if st.trail.front().is_some_and(|f| f.distance(head) > spacing * 0.25) {
            st.trail.push_front(head);
            while st.trail.len() > n * 6 {
                st.trail.pop_back();
            }
        } else if let Some(f) = st.trail.front_mut() {
            *f = head;
        }
        // Sample the trail at fixed arc length.
        let trail: Vec<Vec3> = st.trail.iter().copied().collect();
        let mut segs = vec![head];
        let mut acc = 0.0;
        let mut want = spacing;
        for w in trail.windows(2) {
            let l = w[0].distance(w[1]);
            while acc + l >= want && segs.len() < n {
                let k = (want - acc) / l.max(1e-4);
                segs.push(w[0].lerp(w[1], k));
                want += spacing;
            }
            acc += l;
        }
        while segs.len() < n {
            let last = *segs.last().unwrap();
            segs.push(last - fwd * spacing);
        }
        // Rear up, undulate.
        let rear = match p.anim {
            Anim::Attack { tele: Telegraph::Rear | Telegraph::Raise, w, a, .. } => w * (1.0 - a) * 5.0 * p.size,
            Anim::Attack { tele: Telegraph::Coil, w, a, .. } => -w * (1.0 - a) * 0.8 * p.size,
            Anim::Dead(k) => -k * 1.2 * p.size,
            Anim::Broken(_) => -0.8 * p.size,
            _ => 0.0,
        };
        let swim = p.moving.min(1.0);
        for (i, s) in segs.iter_mut().enumerate() {
            let k = i as f32 / n as f32;
            s.y += rear * (1.0 - k * 2.5).max(0.0);
            let side = Vec3::new(fwd.z, 0.0, -fwd.x);
            *s += side * ((p.time * 4.0 - i as f32 * 0.9).sin() * 0.6 * p.size * swim * k);
        }
        let orient = |i: usize, segs: &Vec<Vec3>| -> Mat4 {
            let a = segs[i];
            let b = if i == 0 { a + fwd } else { segs[i - 1] };
            let d = (b - a).normalize_or(fwd);
            let yaw = d.x.atan2(d.z);
            let pitch = -d.y.asin();
            Mat4::from_translation(a) * Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(pitch)
        };
        for i in 0..n {
            let k = 1.0 - i as f32 / n as f32 * 0.55;
            let m = orient(i, &segs) * Mat4::from_scale(Vec3::splat(p.size * k));
            self.put(dl, content, p, Slot::Frame, Group::All, m, light);
            if i == 2 || i == 5 {
                self.put(dl, content, p, Slot::Back, Group::All, m * tr(0.0, 0.2, 0.0), light);
            }
            if i == 3 {
                self.put(dl, content, p, Slot::Core, Group::All, m, light);
            }
        }
        let jaw = match p.anim {
            Anim::Attack { w, a, .. } => w * (1.0 - a),
            _ => 0.0,
        };
        let hm = orient(0, &segs) * Mat4::from_scale(Vec3::splat(p.size)) * tr(0.0, 0.3, 0.8) * rot(-0.4 * jaw, 0.0, 0.0);
        self.put(dl, content, p, Slot::Head, Group::All, hm, light);
        let m1 = orient(1, &segs) * Mat4::from_scale(Vec3::splat(p.size));
        let claw = match p.anim {
            Anim::Attack { a, .. } => a,
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::LeftArm, Group::All, m1 * tr(-1.0, -0.3, 0.0) * rot(0.0, 0.6 * claw, 0.0), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, m1 * tr(1.0, -0.3, 0.0) * rot(0.0, -0.6 * claw, 0.0), light);
        let ml = orient(n - 1, &segs) * Mat4::from_scale(Vec3::splat(p.size * 0.6));
        self.put(dl, content, p, Slot::Legs, Group::All, ml * rot(0.0, (p.time * 3.0).sin() * 0.3, 0.0), light);
    }

    fn moth(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        let flap_speed = if matches!(p.anim, Anim::Attack { .. }) { 14.0 } else { 7.0 };
        let flap = (p.time * flap_speed + p.id as f32).sin() * if matches!(p.anim, Anim::Dead(_)) { 0.0 } else { 0.6 };
        let tilt = match p.anim {
            Anim::Attack { tele: Telegraph::Rear, w, a, .. } => -0.5 * w * (1.0 - a),
            Anim::Attack { a, .. } => 0.5 * a,
            Anim::Dead(k) => 1.3 * k,
            Anim::Broken(k) => 0.8 * k,
            _ => 0.1,
        };
        let body = root * tr(0.0, 6.5, 0.0) * rot(tilt, 0.0, p.lean);
        self.put(dl, content, p, Slot::Frame, Group::All, body, light);
        self.put(dl, content, p, Slot::Head, Group::All, body * tr(0.0, 1.4, 1.6), light);
        self.put(dl, content, p, Slot::Core, Group::All, body * tr(0.0, 0.0, 0.8), light);
        self.put(dl, content, p, Slot::Back, Group::All, body * tr(0.0, -0.4, -2.6), light);
        let spread = match p.anim {
            Anim::Attack { tele: Telegraph::Spread, w, a, .. } => w * (1.0 - a) * 0.8,
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::Legs, Group::Left, body * tr(-0.6, 0.8, -0.2) * rot(0.0, 0.0, flap + spread), light);
        self.put(dl, content, p, Slot::Legs, Group::Right, body * tr(0.6, 0.8, -0.2) * rot(0.0, 0.0, -flap - spread), light);
        let reach = match p.anim {
            Anim::Attack { a, .. } => -a,
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::LeftArm, Group::All, body * tr(-0.9, -0.6, 1.0) * rot(reach, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, body * tr(0.9, -0.6, 1.0) * rot(reach, 0.0, 0.0), light);
    }

    fn swarm(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        // Draw at most 8 bodies (poly budget); HP is still the full body count in the sim.
        let n = p.bodies.clamp(1, 8) as usize;
        let sink = if let Anim::Dead(k) = p.anim { -3.0 * k } else { 0.0 };
        let gather = match p.anim {
            Anim::Attack { w, a, .. } => 1.0 - 0.5 * w * (1.0 - a),
            _ => 1.0,
        };
        for k in 0..n {
            let a = k as f32 * 2.399;
            let r = if k == 0 { 0.0 } else { (1.6 + (k as f32).sqrt() * 1.8) * gather };
            let bob = (p.time * 3.0 + k as f32 * 1.7).sin() * 0.25 + sink;
            let sway = (p.time * 2.0 + k as f32).sin() * 0.15;
            let m = root
                * tr(a.sin() * r, bob, a.cos() * r)
                * rot(0.0, sway, sway * 0.5)
                * Mat4::from_scale(Vec3::splat(if k == 0 { 1.3 } else { 0.9 }));
            self.put(dl, content, p, Slot::Frame, Group::All, m, light);
            self.put(dl, content, p, Slot::Head, Group::All, m * tr(0.0, 3.0, 0.0) * Mat4::from_scale(Vec3::splat(0.55)), light);
            if k == 0 {
                let reach = match p.anim {
                    Anim::Attack { a, w, .. } => -1.2 * a.max(w * 0.4),
                    _ => 0.0,
                };
                self.put(dl, content, p, Slot::LeftArm, Group::All, m * tr(-0.5, 2.3, 0.0) * rot(reach, 0.0, 0.0), light);
                self.put(dl, content, p, Slot::RightArm, Group::All, m * tr(0.5, 2.3, 0.0) * rot(reach, 0.0, 0.0), light);
                self.put(dl, content, p, Slot::Back, Group::All, m * tr(0.0, 2.2, -0.3), light);
                self.put(dl, content, p, Slot::Core, Group::All, m * tr(0.0, 1.6, 0.4), light);
            }
            if k % 3 == 1 {
                self.put(dl, content, p, Slot::Legs, Group::All, m * tr(0.0, 1.0, 0.0) * Mat4::from_scale(Vec3::splat(0.6)), light);
            }
        }
    }

    fn burrower(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        if p.burrowed {
            // Only the dorsal ridge cuts through the street.
            self.put(dl, content, p, Slot::Back, Group::All, root * tr(0.0, -0.6, 0.0), light);
            return;
        }
        let rise = match p.anim {
            Anim::Attack { tele: Telegraph::Ring, w, a, .. } => 1.0 + 1.5 * w * (1.0 - a),
            Anim::Dead(k) => 1.0 - k,
            _ => 1.0,
        };
        let body = root * tr(0.0, 2.0 * rise, 0.0) * rot(0.0, 0.0, p.lean);
        self.put(dl, content, p, Slot::Frame, Group::All, body, light);
        self.put(dl, content, p, Slot::Legs, Group::All, body * tr(0.0, -1.6, 0.0), light);
        self.put(dl, content, p, Slot::Core, Group::All, body, light);
        self.put(dl, content, p, Slot::Back, Group::All, body * tr(0.0, 1.8, -1.0), light);
        let open = match p.anim {
            Anim::Attack { w, a, .. } => w * (1.0 - a),
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::Head, Group::All, body * tr(0.0, 0.6, 3.6) * rot(-0.5 * open, 0.0, 0.0), light);
        let sw = match p.anim {
            Anim::Walk(ph) => (ph * std::f32::consts::PI).sin() * 0.4,
            Anim::Attack { a, .. } => 0.8 * a,
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::LeftArm, Group::All, body * tr(-2.6, -0.4, 2.0) * rot(sw, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, body * tr(2.6, -0.4, 2.0) * rot(-sw, 0.0, 0.0), light);
    }

    fn quadruped(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        let (mut s, mut pitch, mut drop) = (0.0, 0.0, 0.0);
        match p.anim {
            Anim::Walk(ph) => s = (ph * std::f32::consts::PI).sin() * 0.5,
            Anim::Attack { tele, w, a, .. } => {
                let tw = w * (1.0 - a);
                pitch = match tele {
                    Telegraph::Rear => -0.5 * tw,
                    Telegraph::Coil => 0.3 * tw,
                    _ => -0.2 * tw,
                } + 0.4 * a;
            }
            Anim::Dead(k) => drop = 2.5 * k,
            Anim::Broken(k) => drop = 1.2 * k,
            _ => {}
        }
        let body = root * tr(0.0, 3.5 - drop, 0.0) * rot(pitch, 0.0, p.lean);
        self.put(dl, content, p, Slot::Frame, Group::All, body, light);
        self.put(dl, content, p, Slot::Head, Group::All, body * tr(0.0, 0.7, 2.8), light);
        self.put(dl, content, p, Slot::Core, Group::All, body * tr(0.0, 0.0, 0.8), light);
        self.put(dl, content, p, Slot::Back, Group::All, body * tr(0.0, 1.2, -0.4), light);
        let reach = match p.anim {
            Anim::Attack { a, .. } => -a,
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::LeftArm, Group::All, body * tr(-1.4, -0.4, 1.6) * rot(s + reach, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, body * tr(1.4, -0.4, 1.6) * rot(-s + reach, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::Legs, Group::Left, body * tr(0.0, -0.5, -1.6) * rot(-s, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::Legs, Group::Right, body * tr(0.0, -0.5, -1.6) * rot(s, 0.0, 0.0), light);
    }

    fn vehicle(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        let recoil = match p.anim {
            Anim::Attack { a, r, .. } => (a - r).max(0.0) * -0.5,
            _ => 0.0,
        };
        let hull = root * tr(0.0, 1.2, 0.0);
        self.put(dl, content, p, Slot::Legs, Group::All, root * tr(0.0, 0.6, 0.0), light);
        self.put(dl, content, p, Slot::Frame, Group::All, hull, light);
        self.put(dl, content, p, Slot::Core, Group::All, hull * tr(0.0, 0.0, -1.5), light);
        self.put(dl, content, p, Slot::Back, Group::All, hull * tr(0.0, 0.8, -2.0), light);
        let turret = hull * tr(0.0, 1.1, 0.0);
        self.put(dl, content, p, Slot::Head, Group::All, turret, light);
        self.put(dl, content, p, Slot::RightArm, Group::All, turret * tr(0.0, 0.1, recoil), light);
        self.put(dl, content, p, Slot::LeftArm, Group::All, turret * tr(-0.6, 0.2, 0.4), light);
    }

    fn drone(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        let tilt = match p.anim {
            Anim::Attack { tele: Telegraph::Rear, w, a, .. } => -0.6 * w + 1.0 * a,
            Anim::Dead(k) => 1.4 * k,
            _ => 0.1,
        };
        let body = root * tr(0.0, 2.0, 0.0) * rot(tilt, 0.0, 0.0);
        self.put(dl, content, p, Slot::Frame, Group::All, body, light);
        self.put(dl, content, p, Slot::Legs, Group::All, body * tr(0.0, 0.5, 0.0) * rot(0.0, p.time * 25.0, 0.0), light);
        self.put(dl, content, p, Slot::Head, Group::All, body * tr(0.0, 0.3, 0.7), light);
        self.put(dl, content, p, Slot::Core, Group::All, body, light);
        self.put(dl, content, p, Slot::Back, Group::All, body * tr(0.0, 0.5, -0.6), light);
        self.put(dl, content, p, Slot::LeftArm, Group::All, body * tr(-0.8, -0.3, 0.4), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, body * tr(0.8, -0.3, 0.4), light);
    }

    fn turret(&mut self, dl: &mut DrawList, content: &Content, p: &Pose, root: Mat4, light: &Lighting) {
        self.put(dl, content, p, Slot::Legs, Group::All, root, light);
        self.put(dl, content, p, Slot::Frame, Group::All, root, light);
        self.put(dl, content, p, Slot::Core, Group::All, root * tr(0.0, 1.0, 0.0), light);
        let head = root * tr(0.0, 2.8, 0.0);
        self.put(dl, content, p, Slot::Back, Group::All, head * tr(0.0, 0.6, -1.0), light);
        self.put(dl, content, p, Slot::Head, Group::All, head * tr(0.0, 1.0, 0.0) * rot(0.0, p.time * 2.0, 0.0), light);
        let el = match p.anim {
            Anim::Attack { w, .. } => -0.3 * w,
            _ => 0.0,
        };
        self.put(dl, content, p, Slot::LeftArm, Group::All, head * tr(-0.9, 0.2, 0.4) * rot(el, 0.0, 0.0), light);
        self.put(dl, content, p, Slot::RightArm, Group::All, head * tr(0.9, 0.2, 0.4) * rot(el, 0.0, 0.0), light);
    }
}

/// Build a pose for a live fighter (with interpolated position/yaw).
pub fn pose_of<'a>(f: &'a Fighter, content: &Content, pos: Vec3, yaw: f32, time: f32) -> Pose<'a> {
    let mut tint = Vec3::ONE;
    if f.status.hit_flash > 0 {
        tint = Vec3::splat(1.9);
    }
    if f.loadout.chassis.is_none() && f.kind.is_player() {
        let p = f.loadout.paint[0];
        tint *= Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 200.0 * 0.6 + Vec3::splat(0.4);
    }
    if f.status.burn > 0 {
        tint *= Vec3::new(1.3, 0.8, 0.6);
    }
    let mut alpha = 255;
    if f.hallucination {
        alpha = if (time * 9.0).sin() > 0.0 { 150 } else { 90 };
        tint *= Vec3::new(0.7, 0.9, 1.4);
    }
    if let Act::Dead = f.act
        && f.despawn > 150
    {
        alpha = 120;
    }
    Pose {
        id: f.id,
        loadout: &f.loadout,
        destroyed: f.destroyed,
        rig: f.build.rig,
        pos,
        yaw,
        size: f.size(),
        lean: f.build.lean,
        altitude: f.altitude,
        anim: anim_of(f, content),
        tint,
        alpha,
        bodies: f.bodies,
        burrowed: f.burrowed,
        moving: glam::Vec2::new(f.vel.x, f.vel.z).length() / f.build.speed.max(1.0),
        time,
    }
}

pub fn rig_of(lo: &Loadout, content: &Content) -> RigKind {
    lo.get(Slot::Frame).and_then(|p| content.part(&p.def)).and_then(|d| d.rig).unwrap_or(RigKind::Humanoid)
}

pub fn size_of(lo: &Loadout, content: &Content) -> f32 {
    lo.get(Slot::Frame).and_then(|p| content.part(&p.def)).map_or(1.0, |d| if d.stats.size > 0.0 { d.stats.size } else { 1.0 })
}
