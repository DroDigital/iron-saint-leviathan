//! Tiny collision kit: spheres, vertical capsules, AABBs and swept spheres.
//! PSX-scale needs do not justify a physics engine.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Aabb { min, max }
    }
    pub fn from_center(c: Vec3, half: Vec3) -> Self {
        Aabb { min: c - half, max: c + half }
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn half(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
    pub fn closest(&self, p: Vec3) -> Vec3 {
        p.clamp(self.min, self.max)
    }
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.cmple(o.max).all() && self.max.cmpge(o.min).all()
    }
    pub fn expand(&self, r: f32) -> Aabb {
        Aabb { min: self.min - Vec3::splat(r), max: self.max + Vec3::splat(r) }
    }
    pub fn sphere_hit(&self, c: Vec3, r: f32) -> bool {
        self.closest(c).distance_squared(c) <= r * r
    }
}

/// Vertical capsule: segment from `base` to `base + up*height`, radius `r`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub base: Vec3,
    pub height: f32,
    pub r: f32,
}

impl Capsule {
    pub fn closest_on_axis(&self, p: Vec3) -> Vec3 {
        let y = (p.y).clamp(self.base.y + self.r, self.base.y + (self.height - self.r).max(self.r));
        Vec3::new(self.base.x, y, self.base.z)
    }
    pub fn sphere_hit(&self, c: Vec3, r: f32) -> bool {
        let a = self.closest_on_axis(c);
        a.distance_squared(c) <= (r + self.r) * (r + self.r)
    }
    /// Horizontal push-out against an AABB (fighters vs building chunks). Returns XZ correction.
    pub fn push_out_aabb(&self, b: &Aabb) -> Option<Vec2> {
        if self.base.y + self.height < b.min.y || self.base.y > b.max.y {
            return None;
        }
        let p = Vec2::new(self.base.x, self.base.z);
        let mn = Vec2::new(b.min.x, b.min.z);
        let mx = Vec2::new(b.max.x, b.max.z);
        let c = p.clamp(mn, mx);
        let d = p - c;
        let dist2 = d.length_squared();
        if dist2 > self.r * self.r {
            return None;
        }
        if dist2 > 1e-6 {
            let dist = dist2.sqrt();
            Some(d / dist * (self.r - dist))
        } else {
            // Centre inside the box: push along the shallowest axis.
            let left = p.x - mn.x;
            let right = mx.x - p.x;
            let down = p.y - mn.y;
            let up = mx.y - p.y;
            let m = left.min(right).min(down).min(up);
            Some(if m == left {
                Vec2::new(-(left + self.r), 0.0)
            } else if m == right {
                Vec2::new(right + self.r, 0.0)
            } else if m == down {
                Vec2::new(0.0, -(down + self.r))
            } else {
                Vec2::new(0.0, up + self.r)
            })
        }
    }
}

/// Swept sphere vs sphere: does a sphere moving from a to b (radius ra) touch sphere (c, rb)?
pub fn swept_sphere_hit(a: Vec3, b: Vec3, ra: f32, c: Vec3, rb: f32) -> bool {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 < 1e-8 { 0.0 } else { ((c - a).dot(ab) / len2).clamp(0.0, 1.0) };
    let p = a + ab * t;
    p.distance_squared(c) <= (ra + rb) * (ra + rb)
}

pub fn yaw_dir(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin(), 0.0, yaw.cos())
}

pub fn dir_yaw(d: Vec3) -> f32 {
    d.x.atan2(d.z)
}

/// Shortest signed angle from a to b, in (-PI, PI].
pub fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = (b - a) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

pub fn approach_angle(cur: f32, target: f32, max_step: f32) -> f32 {
    let d = angle_diff(cur, target);
    cur + d.clamp(-max_step, max_step)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capsule_pushes_out_of_box() {
        let cap = Capsule { base: Vec3::new(0.9, 0.0, 0.0), height: 10.0, r: 1.0 };
        let b = Aabb::new(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(0.5, 5.0, 1.0));
        let push = cap.push_out_aabb(&b).unwrap();
        assert!(push.x > 0.0);
        let moved = Capsule { base: cap.base + Vec3::new(push.x, 0.0, push.y), ..cap };
        assert!(moved.push_out_aabb(&b).is_none_or(|p| p.length() < 1e-3));
    }
    #[test]
    fn swept_sphere_catches_tunnelling() {
        // A fast projectile passes completely through a target between ticks.
        assert!(swept_sphere_hit(Vec3::new(-10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), 0.2, Vec3::ZERO, 1.0));
        assert!(!swept_sphere_hit(Vec3::new(-10.0, 5.0, 0.0), Vec3::new(10.0, 5.0, 0.0), 0.2, Vec3::ZERO, 1.0));
    }
    #[test]
    fn angles_wrap() {
        assert!((angle_diff(3.0, -3.0) - (std::f32::consts::TAU - 6.0)).abs() < 1e-4);
        assert!((approach_angle(0.0, 1.0, 0.1) - 0.1).abs() < 1e-6);
    }
}
