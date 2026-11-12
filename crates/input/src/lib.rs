//! Input: every control is an [`Action`] with any number of [`Binding`]s, fully rebindable and
//! serialized in the settings file. Controller-first, with complete keyboard and mouse support.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    CamUp,
    CamDown,
    CamLeft,
    CamRight,
    Light,
    Heavy,
    Guard,
    Parry,
    Dodge,
    Special,
    Heal,
    Interact,
    LockOn,
    LockPrev,
    LockNext,
    LockPart,
    Heretic,
    Pause,
    Confirm,
    Back,
    MenuUp,
    MenuDown,
    MenuLeft,
    MenuRight,
    TabPrev,
    TabNext,
    Photo,
}

impl Action {
    pub const ALL: [Action; 31] = [
        Action::MoveUp, Action::MoveDown, Action::MoveLeft, Action::MoveRight, Action::CamUp, Action::CamDown, Action::CamLeft, Action::CamRight,
        Action::Light, Action::Heavy, Action::Guard, Action::Parry, Action::Dodge, Action::Special, Action::Heal, Action::Interact, Action::LockOn,
        Action::LockPrev, Action::LockNext, Action::LockPart, Action::Heretic, Action::Pause, Action::Confirm, Action::Back, Action::MenuUp,
        Action::MenuDown, Action::MenuLeft, Action::MenuRight, Action::TabPrev, Action::TabNext, Action::Photo,
    ];
    /// Actions shown on the rebinding screen.
    pub const REBINDABLE: [Action; 21] = [
        Action::MoveUp, Action::MoveDown, Action::MoveLeft, Action::MoveRight, Action::Light, Action::Heavy, Action::Guard, Action::Parry,
        Action::Dodge, Action::Special, Action::Heal, Action::Interact, Action::LockOn, Action::LockPrev, Action::LockNext, Action::LockPart,
        Action::Heretic, Action::Pause, Action::Confirm, Action::Back, Action::Photo,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Action::MoveUp => "MOVE FORWARD",
            Action::MoveDown => "MOVE BACK",
            Action::MoveLeft => "MOVE LEFT",
            Action::MoveRight => "MOVE RIGHT",
            Action::CamUp => "CAMERA UP",
            Action::CamDown => "CAMERA DOWN",
            Action::CamLeft => "CAMERA LEFT",
            Action::CamRight => "CAMERA RIGHT",
            Action::Light => "LIGHT ATTACK",
            Action::Heavy => "HEAVY / SEVER",
            Action::Guard => "GUARD",
            Action::Parry => "PARRY",
            Action::Dodge => "DODGE",
            Action::Special => "SPECIAL",
            Action::Heal => "HEAL",
            Action::Interact => "INTERACT / CLIMB",
            Action::LockOn => "LOCK ON",
            Action::LockPrev => "TARGET PREV",
            Action::LockNext => "TARGET NEXT",
            Action::LockPart => "TARGET PART",
            Action::Heretic => "HERETIC FORM",
            Action::Pause => "PAUSE",
            Action::Confirm => "CONFIRM",
            Action::Back => "BACK",
            Action::MenuUp => "MENU UP",
            Action::MenuDown => "MENU DOWN",
            Action::MenuLeft => "MENU LEFT",
            Action::MenuRight => "MENU RIGHT",
            Action::TabPrev => "TAB PREV",
            Action::TabNext => "TAB NEXT",
            Action::Photo => "PHOTO MODE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PadButton {
    South,
    East,
    North,
    West,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Binding {
    /// winit KeyCode name, e.g. "KeyW".
    Key(String),
    Mouse(u8),
    Pad(PadButton),
    /// Axis with direction sign (+1 / -1).
    Axis(PadAxis, i8),
}

impl Binding {
    pub fn label(&self) -> String {
        match self {
            Binding::Key(k) => k.trim_start_matches("Key").trim_start_matches("Digit").to_uppercase(),
            Binding::Mouse(0) => "LMB".into(),
            Binding::Mouse(1) => "RMB".into(),
            Binding::Mouse(2) => "MMB".into(),
            Binding::Mouse(n) => format!("MB{n}"),
            Binding::Pad(b) => format!("{b:?}").to_uppercase(),
            Binding::Axis(a, s) => format!("{a:?}{}", if *s > 0 { "+" } else { "-" }).to_uppercase(),
        }
    }
}

fn key(k: &str) -> Binding {
    Binding::Key(k.to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bindings {
    pub map: BTreeMap<Action, Vec<Binding>>,
    /// Guard: hold (false) or toggle (true).
    pub guard_toggle: bool,
    pub invert_y: bool,
    pub mouse_sens: f32,
    pub pad_sens: f32,
}

impl Default for Bindings {
    fn default() -> Self {
        use Action as A;
        use PadButton as P;
        let mut m: BTreeMap<Action, Vec<Binding>> = BTreeMap::new();
        let mut b = |a: Action, v: Vec<Binding>| {
            m.insert(a, v);
        };
        b(A::MoveUp, vec![key("KeyW"), Binding::Axis(PadAxis::LeftY, 1)]);
        b(A::MoveDown, vec![key("KeyS"), Binding::Axis(PadAxis::LeftY, -1)]);
        b(A::MoveLeft, vec![key("KeyA"), Binding::Axis(PadAxis::LeftX, -1)]);
        b(A::MoveRight, vec![key("KeyD"), Binding::Axis(PadAxis::LeftX, 1)]);
        b(A::CamUp, vec![key("ArrowUp"), Binding::Axis(PadAxis::RightY, 1)]);
        b(A::CamDown, vec![key("ArrowDown"), Binding::Axis(PadAxis::RightY, -1)]);
        b(A::CamLeft, vec![key("ArrowLeft"), Binding::Axis(PadAxis::RightX, -1)]);
        b(A::CamRight, vec![key("ArrowRight"), Binding::Axis(PadAxis::RightX, 1)]);
        b(A::Light, vec![key("KeyJ"), Binding::Mouse(0), Binding::Pad(P::R1)]);
        b(A::Heavy, vec![key("KeyK"), Binding::Mouse(1), Binding::Pad(P::R2)]);
        b(A::Guard, vec![key("ShiftLeft"), key("KeyL"), Binding::Pad(P::L1)]);
        b(A::Parry, vec![key("KeyI"), Binding::Mouse(3), Binding::Pad(P::L2)]);
        b(A::Dodge, vec![key("Space"), Binding::Pad(P::East)]);
        b(A::Special, vec![key("KeyU"), key("KeyR"), Binding::Pad(P::North)]);
        b(A::Heal, vec![key("KeyH"), key("Digit1"), Binding::Pad(P::West)]);
        b(A::Interact, vec![key("KeyE"), Binding::Pad(P::South)]);
        b(A::LockOn, vec![key("KeyQ"), Binding::Mouse(2), Binding::Pad(P::R3)]);
        b(A::LockPrev, vec![key("KeyZ"), Binding::Pad(P::Left)]);
        b(A::LockNext, vec![key("KeyC"), Binding::Pad(P::Right)]);
        b(A::LockPart, vec![key("Tab"), Binding::Pad(P::Up)]);
        b(A::Heretic, vec![key("KeyF"), Binding::Pad(P::L3)]);
        b(A::Pause, vec![key("Escape"), Binding::Pad(P::Start)]);
        b(A::Confirm, vec![key("Enter"), key("KeyE"), Binding::Pad(P::South)]);
        b(A::Back, vec![key("Escape"), key("Backspace"), Binding::Pad(P::East)]);
        b(A::MenuUp, vec![key("ArrowUp"), key("KeyW"), Binding::Pad(P::Up), Binding::Axis(PadAxis::LeftY, 1)]);
        b(A::MenuDown, vec![key("ArrowDown"), key("KeyS"), Binding::Pad(P::Down), Binding::Axis(PadAxis::LeftY, -1)]);
        b(A::MenuLeft, vec![key("ArrowLeft"), key("KeyA"), Binding::Pad(P::Left), Binding::Axis(PadAxis::LeftX, -1)]);
        b(A::MenuRight, vec![key("ArrowRight"), key("KeyD"), Binding::Pad(P::Right), Binding::Axis(PadAxis::LeftX, 1)]);
        b(A::TabPrev, vec![key("KeyQ"), key("PageUp"), Binding::Pad(P::L1)]);
        b(A::TabNext, vec![key("KeyE"), key("PageDown"), Binding::Pad(P::R1)]);
        b(A::Photo, vec![key("KeyP"), Binding::Pad(P::Select)]);
        Bindings { map: m, guard_toggle: false, invert_y: false, mouse_sens: 1.0, pad_sens: 1.0 }
    }
}

/// Per-player input snapshot for one frame.
#[derive(Clone, Debug, Default)]
pub struct PlayerInput {
    pub held: HashSet<Action>,
    pub pressed: HashSet<Action>,
    pub move_axis: Vec2,
    pub cam_axis: Vec2,
    pub mouse_delta: Vec2,
    pub guard_latched: bool,
    pub using_pad: bool,
}

impl PlayerInput {
    pub fn down(&self, a: Action) -> bool {
        self.held.contains(&a)
    }
    pub fn hit(&self, a: Action) -> bool {
        self.pressed.contains(&a)
    }
    pub fn guard(&self, toggle: bool) -> bool {
        if toggle { self.guard_latched } else { self.down(Action::Guard) }
    }
}

struct Pad {
    id: gilrs::GamepadId,
    buttons: HashSet<PadButton>,
    axes: [f32; 4],
    ff: Option<gilrs::ff::Effect>,
}

pub struct Input {
    pub bindings: Bindings,
    keys: HashSet<String>,
    mouse: HashSet<u8>,
    mouse_delta: Vec2,
    gilrs: Option<gilrs::Gilrs>,
    pads: Vec<Pad>,
    prev: [HashSet<Action>; 2],
    pub players: [PlayerInput; 2],
    /// When set, the next pressed binding is assigned to this action (rebinding screen).
    pub capture: Option<Action>,
    pub captured: Option<(Action, Binding)>,
    pub any_pressed: bool,
    pub rumble_enabled: bool,
}

fn map_button(b: gilrs::Button) -> Option<PadButton> {
    use gilrs::Button as G;
    Some(match b {
        G::South => PadButton::South,
        G::East => PadButton::East,
        G::North => PadButton::North,
        G::West => PadButton::West,
        G::LeftTrigger => PadButton::L1,
        G::RightTrigger => PadButton::R1,
        G::LeftTrigger2 => PadButton::L2,
        G::RightTrigger2 => PadButton::R2,
        G::LeftThumb => PadButton::L3,
        G::RightThumb => PadButton::R3,
        G::Select => PadButton::Select,
        G::Start => PadButton::Start,
        G::DPadUp => PadButton::Up,
        G::DPadDown => PadButton::Down,
        G::DPadLeft => PadButton::Left,
        G::DPadRight => PadButton::Right,
        _ => return None,
    })
}

impl Input {
    pub fn new(bindings: Bindings) -> Input {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(g) => Some(g),
            Err(e) => {
                tracing::warn!("gamepads unavailable: {e}");
                None
            }
        };
        Input {
            bindings,
            keys: HashSet::new(),
            mouse: HashSet::new(),
            mouse_delta: Vec2::ZERO,
            gilrs,
            pads: vec![],
            prev: [HashSet::new(), HashSet::new()],
            players: [PlayerInput::default(), PlayerInput::default()],
            capture: None,
            captured: None,
            any_pressed: false,
            rumble_enabled: true,
        }
    }

    pub fn pad_count(&self) -> usize {
        self.pads.len()
    }

    pub fn on_key(&mut self, code: KeyCode, state: ElementState) {
        let name = format!("{code:?}");
        match state {
            ElementState::Pressed => {
                if let Some(a) = self.capture.take() {
                    self.captured = Some((a, Binding::Key(name.clone())));
                }
                self.keys.insert(name);
            }
            ElementState::Released => {
                self.keys.remove(&name);
            }
        }
    }

    pub fn on_mouse_button(&mut self, b: MouseButton, state: ElementState) {
        let n = match b {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(x) => (x as u8).saturating_add(5),
        };
        match state {
            ElementState::Pressed => {
                if let Some(a) = self.capture.take() {
                    self.captured = Some((a, Binding::Mouse(n)));
                }
                self.mouse.insert(n);
            }
            ElementState::Released => {
                self.mouse.remove(&n);
            }
        }
    }

    pub fn on_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse_delta += Vec2::new(dx as f32, dy as f32);
    }

    /// Drop all held state (window lost focus).
    pub fn clear(&mut self) {
        self.keys.clear();
        self.mouse.clear();
    }

    fn poll_pads(&mut self) {
        let Some(g) = self.gilrs.as_mut() else { return };
        while let Some(ev) = g.next_event() {
            let id = ev.id;
            if !self.pads.iter().any(|p| p.id == id) {
                self.pads.push(Pad { id, buttons: HashSet::new(), axes: [0.0; 4], ff: None });
            }
            let pad = self.pads.iter_mut().find(|p| p.id == id).unwrap();
            match ev.event {
                gilrs::EventType::ButtonPressed(b, _) => {
                    if let Some(pb) = map_button(b) {
                        if let Some(a) = self.capture.take() {
                            self.captured = Some((a, Binding::Pad(pb)));
                        }
                        pad.buttons.insert(pb);
                    }
                }
                gilrs::EventType::ButtonReleased(b, _) => {
                    if let Some(pb) = map_button(b) {
                        pad.buttons.remove(&pb);
                    }
                }
                gilrs::EventType::ButtonChanged(b, v, _) => {
                    // Analog triggers.
                    if let Some(pb) = map_button(b) {
                        if v > 0.5 {
                            pad.buttons.insert(pb);
                        } else {
                            pad.buttons.remove(&pb);
                        }
                    }
                }
                gilrs::EventType::AxisChanged(a, v, _) => {
                    let i = match a {
                        gilrs::Axis::LeftStickX => 0,
                        gilrs::Axis::LeftStickY => 1,
                        gilrs::Axis::RightStickX => 2,
                        gilrs::Axis::RightStickY => 3,
                        _ => continue,
                    };
                    pad.axes[i] = v;
                }
                gilrs::EventType::Disconnected => {
                    self.pads.retain(|p| p.id != id);
                }
                _ => {}
            }
        }
    }

    fn binding_value(&self, b: &Binding, pad: Option<&Pad>) -> f32 {
        let dead = 0.22;
        match b {
            Binding::Key(k) => self.keys.contains(k) as u8 as f32,
            Binding::Mouse(m) => self.mouse.contains(m) as u8 as f32,
            Binding::Pad(p) => pad.is_some_and(|pd| pd.buttons.contains(p)) as u8 as f32,
            Binding::Axis(a, s) => pad.map_or(0.0, |pd| {
                let v = pd.axes[*a as usize] * *s as f32;
                if v > dead { (v - dead) / (1.0 - dead) } else { 0.0 }
            }),
        }
    }

    /// Call once per frame after feeding window events.
    pub fn update(&mut self, versus: bool) {
        self.poll_pads();
        self.any_pressed = false;
        for pl in 0..2 {
            // P1: keyboard/mouse + first pad (or only kb in versus with 1 pad -> P2 gets pad 0).
            let pad_idx = if versus { if self.pads.len() >= 2 { Some(pl) } else if pl == 1 { Some(0) } else { None } } else if pl == 0 { Some(0) } else { None };
            let pad = pad_idx.and_then(|i| self.pads.get(i));
            let use_kb = pl == 0 && !(versus && self.pads.len() < 2 && pl == 1);
            let mut held = HashSet::new();
            let mut vals: BTreeMap<Action, f32> = BTreeMap::new();
            let mut using_pad = false;
            for (a, bs) in &self.bindings.map {
                let mut v: f32 = 0.0;
                for b in bs {
                    let is_kb = matches!(b, Binding::Key(_) | Binding::Mouse(_));
                    if is_kb && !use_kb {
                        continue;
                    }
                    let x = self.binding_value(b, pad);
                    if x > 0.0 && !is_kb {
                        using_pad = true;
                    }
                    v = v.max(x);
                }
                if v > 0.5 {
                    held.insert(*a);
                }
                vals.insert(*a, v);
            }
            let g = |a: Action| vals.get(&a).copied().unwrap_or(0.0);
            let mut mv = Vec2::new(g(Action::MoveRight) - g(Action::MoveLeft), g(Action::MoveUp) - g(Action::MoveDown));
            if mv.length() > 1.0 {
                mv = mv.normalize();
            }
            let inv = if self.bindings.invert_y { -1.0 } else { 1.0 };
            let cam = Vec2::new(g(Action::CamRight) - g(Action::CamLeft), (g(Action::CamUp) - g(Action::CamDown)) * inv) * self.bindings.pad_sens;
            let pressed: HashSet<Action> = held.difference(&self.prev[pl]).copied().collect();
            if !pressed.is_empty() {
                self.any_pressed = true;
            }
            let p = &mut self.players[pl];
            if pressed.contains(&Action::Guard) {
                p.guard_latched = !p.guard_latched;
            }
            p.pressed = pressed;
            p.held = held.clone();
            p.move_axis = mv;
            p.cam_axis = cam;
            p.mouse_delta = if pl == 0 { self.mouse_delta * self.bindings.mouse_sens } else { Vec2::ZERO };
            p.using_pad = using_pad;
            self.prev[pl] = held;
        }
        self.mouse_delta = Vec2::ZERO;
        if let Some((a, b)) = self.captured.take() {
            self.rebind(a, b);
        }
    }

    /// Replace the first binding of the same device class for an action.
    pub fn rebind(&mut self, a: Action, b: Binding) {
        let list = self.bindings.map.entry(a).or_default();
        let same = |x: &Binding| std::mem::discriminant(x) == std::mem::discriminant(&b)
            || (matches!(x, Binding::Key(_) | Binding::Mouse(_)) && matches!(b, Binding::Key(_) | Binding::Mouse(_)))
            || (matches!(x, Binding::Pad(_) | Binding::Axis(..)) && matches!(b, Binding::Pad(_) | Binding::Axis(..)));
        if let Some(i) = list.iter().position(same) {
            list[i] = b;
        } else {
            list.push(b);
        }
    }

    /// Controller rumble (kaiju footsteps, heavy hits). Unsupported platforms ignore it.
    pub fn rumble(&mut self, player: usize, strength: f32, ms: u32) {
        if !self.rumble_enabled || strength <= 0.01 {
            return;
        }
        let Some(g) = self.gilrs.as_mut() else { return };
        let Some(pad) = self.pads.get_mut(player) else { return };
        if !g.gamepad(pad.id).is_ff_supported() {
            return;
        }
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
        let mag = (strength.clamp(0.0, 1.0) * u16::MAX as f32) as u16;
        let eff = EffectBuilder::new()
            .add_effect(BaseEffect { kind: BaseEffectType::Strong { magnitude: mag }, scheduling: Replay { play_for: Ticks::from_ms(ms), ..Default::default() }, ..Default::default() })
            .add_gamepad(&g.gamepad(pad.id))
            .finish(g);
        if let Ok(e) = eff {
            let _ = e.play();
            pad.ff = Some(e);
        }
    }

    /// Short label for the first binding of an action on the active device (for prompts).
    pub fn prompt(&self, a: Action, pad: bool) -> String {
        self.bindings
            .map
            .get(&a)
            .and_then(|v| v.iter().find(|b| matches!(b, Binding::Pad(_) | Binding::Axis(..)) == pad))
            .map_or("-".into(), |b| b.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebinding_replaces_same_device_class() {
        let mut i = Input { gilrs: None, ..Input::new(Bindings::default()) };
        i.rebind(Action::Light, Binding::Key("KeyZ".into()));
        let l = &i.bindings.map[&Action::Light];
        assert_eq!(l[0], Binding::Key("KeyZ".into()));
        assert!(l.contains(&Binding::Pad(PadButton::R1)), "pad binding untouched");
    }
    #[test]
    fn keyboard_press_edges_and_toggle_guard() {
        let mut i = Input { gilrs: None, ..Input::new(Bindings::default()) };
        i.on_key(KeyCode::KeyJ, ElementState::Pressed);
        i.on_key(KeyCode::KeyW, ElementState::Pressed);
        i.update(false);
        assert!(i.players[0].hit(Action::Light));
        assert_eq!(i.players[0].move_axis, Vec2::new(0.0, 1.0));
        i.update(false);
        assert!(!i.players[0].hit(Action::Light) && i.players[0].down(Action::Light));
        i.on_key(KeyCode::ShiftLeft, ElementState::Pressed);
        i.update(false);
        i.on_key(KeyCode::ShiftLeft, ElementState::Released);
        i.update(false);
        assert!(i.players[0].guard(true), "toggle guard latched");
        assert!(!i.players[0].guard(false));
    }
    #[test]
    fn bindings_serialize() {
        let b = Bindings::default();
        assert!(Action::ALL.iter().all(|a| b.map.contains_key(a)));
    }
}
