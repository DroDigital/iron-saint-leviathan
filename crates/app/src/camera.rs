//! Third-person camera: free orbit, lock-on framing (with part targeting), scale-combat mode for
//! huge bosses, and trauma-based screen shake (scaled by the accessibility slider).

use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Debug)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub pos: Vec3,
    pub target: Vec3,
    pub trauma: f32,
    pub fov: f32,
    pub t: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            yaw: std::f32::consts::PI,
            pitch: 0.32,
            dist: 30.0,
            pos: Vec3::new(0.0, 20.0, -30.0),
            target: Vec3::ZERO,
            trauma: 0.0,
            fov: 1.05,
            t: 0.0,
        }
    }
}

pub struct Focus {
    pub subject: Vec3,
    pub subject_height: f32,
    pub lock: Option<(Vec3, f32)>,
    /// Huge boss in play: pull back and frame both.
    pub scale_mode: bool,
}

impl Camera {
    pub fn add_trauma(&mut self, x: f32) {
        self.trauma = (self.trauma + x).min(1.0);
    }

    /// Update from stick/mouse input and the focus. `dt` in seconds.
    pub fn update(&mut self, dt: f32, look: Vec2, f: &Focus, ground: impl Fn(f32, f32) -> f32) {
        self.t += dt;
        self.trauma = (self.trauma - dt * 1.4).max(0.0);
        let base = (f.subject_height * 2.2 + 10.0).max(18.0);
        let want_dist = if f.scale_mode { base * 2.0 } else { base };
        self.dist += (want_dist - self.dist) * (dt * 3.0).min(1.0);
        let head = f.subject + Vec3::Y * f.subject_height * 0.75;
        if let Some((lp, lh)) = f.lock {
            // Lock-on: swing behind the player, looking at a point between player and target.
            let d = lp - f.subject;
            let want_yaw = d.x.atan2(d.z) + std::f32::consts::PI;
            let diff = (want_yaw - self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            self.yaw += diff * (dt * 6.0).min(1.0);
            let span = Vec2::new(d.x, d.z).length();
            let want_pitch = (0.22 + (lh / span.max(1.0)) * 0.25).clamp(0.12, 0.6);
            self.pitch += (want_pitch - self.pitch) * (dt * 4.0).min(1.0);
            let mid = head.lerp(lp + Vec3::Y * lh * 0.5, 0.35);
            self.target = self.target.lerp(mid, (dt * 10.0).min(1.0));
        } else {
            self.yaw -= look.x * dt * 2.6;
            self.pitch = (self.pitch + look.y * dt * 1.6).clamp(-0.15, 1.2);
            self.target = self.target.lerp(head, (dt * 10.0).min(1.0));
        }
        let back = Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos());
        let mut p = self.target + back * self.dist;
        let g = ground(p.x, p.z).max(0.0) + 2.0;
        if p.y < g {
            p.y = g;
        }
        self.pos = p;
    }

    pub fn shake_offset(&self, scale: f32) -> Vec3 {
        let s = self.trauma * self.trauma * scale;
        let t = self.t * 40.0;
        Vec3::new((t * 1.3).sin(), (t * 1.7 + 1.0).sin(), (t * 1.1 + 2.0).sin()) * s * 1.2
    }

    pub fn view(&self, shake: f32) -> Mat4 {
        let o = self.shake_offset(shake);
        isl_render::look_at(self.pos + o, self.target + o * 0.5, Vec3::Y)
    }

    /// Camera-relative movement: stick up = away from camera.
    pub fn move_dir(&self, stick: Vec2) -> Vec2 {
        let fwd = Vec2::new(-self.yaw.sin(), -self.yaw.cos());
        let right = Vec2::new(-fwd.y, fwd.x);
        fwd * stick.y + right * stick.x
    }

    pub fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }
}
