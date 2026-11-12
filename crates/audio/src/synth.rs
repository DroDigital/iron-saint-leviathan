//! A tiny deterministic synthesizer. All game audio is generated here (no sample packs), which is
//! both a uniqueness win and a licensing win: nothing to log in ASSETS.md.

use std::f32::consts::TAU;

pub struct Rng(u32);
impl Rng {
    pub fn new(s: u32) -> Rng {
        Rng(s | 1)
    }
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

pub fn env_ad(t: f32, a: f32, d: f32) -> f32 {
    if t < a {
        t / a.max(1e-4)
    } else {
        (-(t - a) / d.max(1e-4)).exp()
    }
}

pub fn saw(p: f32) -> f32 {
    2.0 * (p - (p + 0.5).floor())
}
pub fn square(p: f32, duty: f32) -> f32 {
    if p.fract() < duty { 1.0 } else { -1.0 }
}
pub fn tri(p: f32) -> f32 {
    4.0 * (p.fract() - 0.5).abs() - 1.0
}

/// One-pole low-pass filter in place.
pub fn lowpass(buf: &mut [f32], sr: f32, cutoff: f32) {
    let a = (TAU * cutoff / sr) / (TAU * cutoff / sr + 1.0);
    let mut y = 0.0;
    for s in buf.iter_mut() {
        y += a * (*s - y);
        *s = y;
    }
}
pub fn highpass(buf: &mut [f32], sr: f32, cutoff: f32) {
    let rc = 1.0 / (TAU * cutoff);
    let a = rc / (rc + 1.0 / sr);
    let (mut px, mut py) = (0.0, 0.0);
    for s in buf.iter_mut() {
        let y = a * (py + *s - px);
        px = *s;
        py = y;
        *s = y;
    }
}

pub fn normalize(buf: &mut [f32], peak: f32) {
    let m = buf.iter().fold(0.0f32, |a, b| a.max(b.abs()));
    if m > 1e-6 {
        for s in buf.iter_mut() {
            *s *= peak / m;
        }
    }
}

pub fn softclip(x: f32) -> f32 {
    x.tanh()
}

pub fn midi_hz(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// Sound effect recipes. `sr` is the output sample rate (22050 in SPU mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Swing,
    Clank,
    Flesh,
    Block,
    Parry,
    StepMech,
    StepKaiju,
    Roar,
    Hiss,
    Charge,
    Bell,
    Click,
    Drone,
    Gurgle,
    Shriek,
    Explosion,
    Sever,
    Death,
    Heal,
    Vent,
    Pickup,
    UiMove,
    UiOk,
    UiBack,
    DiscSwap,
    Lightning,
    Eat,
    Lock,
    Coins,
    Rest,
    Shatter,
    Splash,
    Collapse,
}

impl Sfx {
    pub const ALL: [Sfx; 33] = [
        Sfx::Swing, Sfx::Clank, Sfx::Flesh, Sfx::Block, Sfx::Parry, Sfx::StepMech, Sfx::StepKaiju, Sfx::Roar, Sfx::Hiss, Sfx::Charge, Sfx::Bell,
        Sfx::Click, Sfx::Drone, Sfx::Gurgle, Sfx::Shriek, Sfx::Explosion, Sfx::Sever, Sfx::Death, Sfx::Heal, Sfx::Vent, Sfx::Pickup, Sfx::UiMove,
        Sfx::UiOk, Sfx::UiBack, Sfx::DiscSwap, Sfx::Lightning, Sfx::Eat, Sfx::Lock, Sfx::Coins, Sfx::Rest, Sfx::Shatter, Sfx::Splash, Sfx::Collapse,
    ];
}

pub fn render_sfx(s: Sfx, sr: f32) -> Vec<f32> {
    let mut rng = Rng::new(s as u32 * 7919 + 13);
    let len = |secs: f32| (secs * sr) as usize;
    let mut out;
    match s {
        Sfx::Swing => {
            out = vec![0.0; len(0.28)];
            let mut lp = 0.0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let cut = 0.05 + 0.5 * (t / 0.28 * std::f32::consts::PI).sin();
                lp += cut * (rng.next() - lp);
                *o = lp * env_ad(t, 0.08, 0.1);
            }
        }
        Sfx::Clank | Sfx::Block | Sfx::Parry => {
            let (base, dur, bright) = match s {
                Sfx::Clank => (180.0, 0.5, 0.4),
                Sfx::Block => (320.0, 0.35, 0.6),
                _ => (880.0, 0.9, 1.0),
            };
            out = vec![0.0; len(dur)];
            let partials = [1.0, 2.76, 5.4, 8.93, 13.3];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let mut v = 0.0;
                for (k, p) in partials.iter().enumerate() {
                    v += (TAU * base * p * t).sin() * (-t * (6.0 + k as f32 * 6.0 / bright)).exp() / (k as f32 + 1.0);
                }
                v += rng.next() * (-t * 60.0).exp() * 0.6;
                *o = v;
            }
        }
        Sfx::Flesh => {
            out = vec![0.0; len(0.3)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                *o = (TAU * (90.0 - t * 120.0) * t).sin() * (-t * 14.0).exp() + rng.next() * (-t * 30.0).exp() * 0.5;
            }
            lowpass(&mut out, sr, 1600.0);
        }
        Sfx::StepMech | Sfx::StepKaiju => {
            let (f, d) = if s == Sfx::StepMech { (70.0, 0.35) } else { (38.0, 0.8) };
            out = vec![0.0; len(d)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let mut v = (TAU * f * (1.0 - t * 0.6) * t).sin() * (-t * 7.0).exp();
                v += rng.next() * (-t * 25.0).exp() * 0.4;
                if s == Sfx::StepMech {
                    v += (TAU * 410.0 * t).sin() * (-t * 30.0).exp() * 0.3;
                }
                *o = softclip(v * 1.5);
            }
            lowpass(&mut out, sr, if s == Sfx::StepMech { 2000.0 } else { 600.0 });
        }
        Sfx::Roar | Sfx::Shriek => {
            let (f0, d) = if s == Sfx::Roar { (85.0, 1.2) } else { (520.0, 0.8) };
            out = vec![0.0; len(d)];
            let mut ph = 0.0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let f = f0 * (1.0 + 0.3 * (t * 7.0).sin() * t) * if s == Sfx::Shriek { 1.0 + t } else { 1.0 - t * 0.3 };
                ph += f / sr;
                *o = softclip((saw(ph) + rng.next() * 0.4) * 2.0) * env_ad(t, 0.12, d * 0.4);
            }
            lowpass(&mut out, sr, if s == Sfx::Roar { 900.0 } else { 4000.0 });
        }
        Sfx::Hiss | Sfx::Vent | Sfx::Splash => {
            let d = match s {
                Sfx::Hiss => 0.7,
                Sfx::Vent => 1.2,
                _ => 0.6,
            };
            out = vec![0.0; len(d)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                *o = rng.next() * env_ad(t, 0.05, d * 0.35);
            }
            highpass(&mut out, sr, if s == Sfx::Splash { 400.0 } else { 2500.0 });
        }
        Sfx::Charge | Sfx::Heal => {
            let d = 0.7;
            out = vec![0.0; len(d)];
            let mut ph = 0.0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let f = if s == Sfx::Charge { 200.0 + t * 900.0 } else { 440.0 * [1.0, 1.25, 1.5, 2.0][((t / d * 4.0) as usize).min(3)] };
                ph += f / sr;
                *o = square(ph, 0.25) * 0.5 * env_ad(t, 0.02, d * 0.5);
            }
            lowpass(&mut out, sr, 3500.0);
        }
        Sfx::Bell | Sfx::Rest => {
            let d = if s == Sfx::Bell { 2.5 } else { 2.0 };
            out = vec![0.0; len(d)];
            let notes: &[f32] = if s == Sfx::Bell { &[1.0] } else { &[1.0, 1.26, 1.5] };
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let mut v = 0.0;
                for (k, n) in notes.iter().enumerate() {
                    let tt = (t - k as f32 * 0.15).max(0.0);
                    let f = 220.0 * n;
                    for (pk, p) in [0.5, 1.0, 1.19, 1.56, 2.0, 2.74].iter().enumerate() {
                        v += (TAU * f * p * tt).sin() * (-tt * (1.0 + pk as f32)).exp() * 0.3;
                    }
                }
                *o = v;
            }
        }
        Sfx::Click | Sfx::Lock | Sfx::UiMove => {
            out = vec![0.0; len(0.12)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let f = match s {
                    Sfx::Lock => 1500.0,
                    Sfx::UiMove => 900.0,
                    _ => 2400.0,
                };
                let click2 = if s == Sfx::Click && t > 0.05 { (-(t - 0.05) * 200.0).exp() } else { 0.0 };
                *o = square(f * t, 0.5) * ((-t * 120.0).exp() + click2) * 0.6;
            }
        }
        Sfx::UiOk | Sfx::UiBack | Sfx::Pickup | Sfx::Coins => {
            let seq: &[f32] = match s {
                Sfx::UiOk => &[72.0, 79.0],
                Sfx::UiBack => &[74.0, 67.0],
                Sfx::Pickup => &[72.0, 76.0, 79.0, 84.0],
                _ => &[88.0, 91.0, 96.0],
            };
            let step = 0.06;
            out = vec![0.0; len(step * seq.len() as f32 + 0.15)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let k = ((t / step) as usize).min(seq.len() - 1);
                let tt = t - k as f32 * step;
                *o = square(midi_hz(seq[k]) * t, 0.5) * (-tt * 18.0).exp() * 0.5;
            }
        }
        Sfx::Drone => {
            out = vec![0.0; len(1.2)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                *o = (saw(110.0 * t) + saw(111.3 * t) + saw(164.0 * t)) * 0.3 * env_ad(t, 0.2, 0.5);
            }
            lowpass(&mut out, sr, 1200.0);
        }
        Sfx::Gurgle | Sfx::Eat => {
            let d = if s == Sfx::Eat { 0.5 } else { 0.9 };
            out = vec![0.0; len(d)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let am = 0.5 + 0.5 * (TAU * if s == Sfx::Eat { 22.0 } else { 9.0 } * t).sin();
                *o = rng.next() * am * env_ad(t, 0.03, d * 0.5);
            }
            lowpass(&mut out, sr, if s == Sfx::Eat { 2500.0 } else { 700.0 });
        }
        Sfx::Explosion | Sfx::Lightning | Sfx::Collapse => {
            let d = match s {
                Sfx::Explosion => 1.2,
                Sfx::Lightning => 2.0,
                _ => 1.6,
            };
            out = vec![0.0; len(d)];
            let mut lp = 0.0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                lp += 0.08 * (rng.next() - lp);
                let crack = if s == Sfx::Lightning { rng.next() * (-t * 40.0).exp() } else { 0.0 };
                *o = softclip((lp * 4.0 + crack) * env_ad(t, 0.005, d * 0.35) * 2.0);
            }
        }
        Sfx::Sever | Sfx::Shatter => {
            out = vec![0.0; len(0.9)];
            let mut ph = 0.0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                ph += (900.0 - t * 800.0).max(60.0) / sr;
                *o = (rng.next() * 0.7 + saw(ph) * 0.5) * env_ad(t, 0.01, 0.3) + (TAU * 140.0 * t).sin() * (-t * 6.0).exp() * 0.6;
            }
            if s == Sfx::Shatter {
                highpass(&mut out, sr, 800.0);
            }
        }
        Sfx::Death => {
            out = vec![0.0; len(2.2)];
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                *o = (TAU * (60.0 - t * 15.0) * t).sin() * env_ad(t, 0.05, 0.9) + rng.next() * 0.2 * env_ad(t, 0.01, 0.6);
            }
            lowpass(&mut out, sr, 500.0);
        }
        Sfx::DiscSwap => {
            out = vec![0.0; len(1.6)];
            let mut ph = 0.0;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let f = 60.0 + (t * 3.0).min(1.0) * 120.0;
                ph += f / sr;
                *o = (square(ph, 0.5) * 0.3 + rng.next() * 0.1) * env_ad(t, 0.3, 0.8) + if (t - 1.2).abs() < 0.01 { 0.8 } else { 0.0 };
            }
            lowpass(&mut out, sr, 1800.0);
        }
    }
    normalize(&mut out, 0.9);
    out
}

/// Map a data telegraph cue to a sound.
pub fn cue_sfx(c: isl_data::Cue) -> Sfx {
    use isl_data::Cue::*;
    match c {
        Swing => Sfx::Swing,
        Clank => Sfx::Clank,
        Roar => Sfx::Roar,
        Hiss => Sfx::Hiss,
        Charge => Sfx::Charge,
        Bell => Sfx::Bell,
        Click => Sfx::Click,
        Drone => Sfx::Drone,
        Gurgle => Sfx::Gurgle,
        Shriek => Sfx::Shriek,
    }
}
