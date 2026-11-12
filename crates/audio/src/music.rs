//! Tracker-style adaptive music, composed procedurally per district from its `MusicDef`
//! (tempo, root, scale, seed). Five layers share one loop length so they stay locked together:
//! EXPLORE, COMBAT, BOSS I, BOSS II, BOSS III. The game crossfades between them on the beat.

use crate::synth::*;
use isl_data::MusicDef;

pub const LAYERS: usize = 5;
pub const BARS: usize = 8;
pub const STEPS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Explore = 0,
    Combat = 1,
    Boss1 = 2,
    Boss2 = 3,
    Boss3 = 4,
}

pub struct Song {
    pub sr: u32,
    pub beat_secs: f32,
    /// Stereo interleaved per layer.
    pub layers: Vec<Vec<[f32; 2]>>,
}

struct Pattern {
    chords: Vec<u8>,
    melody: Vec<Option<u8>>,
    bassline: Vec<Option<u8>>,
    arp: Vec<u8>,
}

fn compose(m: &MusicDef) -> Pattern {
    let mut r = Rng::new(m.seed as u32 ^ 0xA5A5);
    // Minor-ish progressions on scale degrees.
    let progs: [[u8; 4]; 4] = [[0, 5, 3, 4], [0, 3, 5, 4], [0, 6, 5, 4], [0, 2, 3, 4]];
    let prog = progs[((r.next() + 1.0) * 2.0) as usize % 4];
    let chords: Vec<u8> = (0..BARS).map(|b| prog[(b / 2) % 4]).collect();
    let mut melody = vec![None; BARS * STEPS];
    let mut deg: i32 = 4;
    for (i, slot) in melody.iter_mut().enumerate() {
        let on_beat = i % 4 == 0;
        if r.next() > if on_beat { -0.2 } else { 0.55 } {
            deg = (deg + (r.next() * 2.6) as i32).clamp(0, 11);
            *slot = Some(deg as u8);
        }
    }
    let bassline = (0..BARS * STEPS).map(|i| if i % 4 == 0 || (i % 8 == 6 && r.next() > 0.0) { Some(0) } else { None }).collect();
    let arp = (0..STEPS).map(|i| [0u8, 2, 4, 2, 0, 4, 2, 6][i % 8]).collect();
    Pattern { chords, melody, bassline, arp }
}

fn note(m: &MusicDef, degree: i32, octave: i32) -> f32 {
    let n = m.scale.len() as i32;
    let o = degree.div_euclid(n);
    let d = degree.rem_euclid(n);
    midi_hz(m.root as f32 + m.scale[d as usize] as f32 + 12.0 * (o + octave) as f32)
}

/// Render all five layers. Deterministic for a given MusicDef.
pub fn render(m: &MusicDef, sr: u32) -> Song {
    let srf = sr as f32;
    let beat = 60.0 / m.tempo;
    let step = beat / 4.0;
    let total_steps = BARS * STEPS;
    let n = (total_steps as f32 * step * srf) as usize;
    let p = compose(m);
    let mut rng = Rng::new(m.seed as u32);
    // Stems.
    let mut pad = vec![[0.0f32; 2]; n];
    let mut bass = vec![0.0f32; n];
    let mut bass8 = vec![0.0f32; n];
    let mut kick = vec![0.0f32; n];
    let mut snare = vec![0.0f32; n];
    let mut hat = vec![0.0f32; n];
    let mut arp = vec![0.0f32; n];
    let mut lead = vec![0.0f32; n];
    let mut bell = vec![0.0f32; n];
    let mut choir = vec![[0.0f32; 2]; n];
    let at = |s: usize| (s as f32 * step * srf) as usize;
    for s in 0..total_steps {
        let bar = s / STEPS;
        let root = p.chords[bar] as i32;
        let i0 = at(s);
        let len = at(1);
        // Pad: chord each bar.
        if s % STEPS == 0 {
            let dur = at(STEPS);
            for (k, d) in [0, 2, 4].iter().enumerate() {
                let f = note(m, root + d, -1);
                for j in 0..dur.min(n - i0) {
                    let t = j as f32 / srf;
                    let e = env_ad(t, 0.4, 3.0);
                    let l = saw(f * t) + saw(f * 1.004 * t);
                    let r = saw(f * t * 0.997) + saw(f * 1.002 * t + 0.3);
                    pad[i0 + j][0] += l * e * 0.08 / (k as f32 + 1.0).sqrt();
                    pad[i0 + j][1] += r * e * 0.08 / (k as f32 + 1.0).sqrt();
                    let cf = note(m, root + d, 0);
                    let vib = (t * 5.0).sin() * 0.004;
                    choir[i0 + j][0] += tri(cf * (1.0 + vib) * t) * e * 0.05;
                    choir[i0 + j][1] += tri(cf * (1.0 - vib) * t + 0.25) * e * 0.05;
                }
            }
        }
        // Sub bass on half notes (explore) and driving 8ths (combat+).
        if s % 8 == 0 {
            let f = note(m, root, -2);
            for j in 0..at(8).min(n - i0) {
                let t = j as f32 / srf;
                bass[i0 + j] += (TAU_F * f * t).sin() * env_ad(t, 0.01, 1.2) * 0.35;
            }
        }
        if p.bassline[s].is_some() || s % 2 == 0 {
            let f = note(m, root, -2);
            for j in 0..len.min(n - i0) {
                let t = j as f32 / srf;
                bass8[i0 + j] += square(f * t, 0.3) * env_ad(t, 0.005, 0.12) * 0.22;
            }
        }
        // Drums.
        if s % 4 == 0 {
            for j in 0..at(2).min(n - i0) {
                let t = j as f32 / srf;
                kick[i0 + j] += (TAU_F * (120.0 * (-t * 25.0).exp() + 45.0) * t).sin() * (-t * 9.0).exp() * 0.6;
            }
        }
        if s % 8 == 4 {
            for j in 0..at(2).min(n - i0) {
                let t = j as f32 / srf;
                snare[i0 + j] += (rng.next() * 0.7 + (TAU_F * 190.0 * t).sin() * 0.3) * (-t * 18.0).exp() * 0.4;
            }
        }
        if s % 2 == 0 || s % 16 > 12 {
            for j in 0..len.min(n - i0) {
                let t = j as f32 / srf;
                hat[i0 + j] += rng.next() * (-t * 70.0).exp() * 0.15;
            }
        }
        // Arp 16ths.
        let ad = p.arp[s % STEPS] as i32;
        let fa = note(m, root + ad, 1);
        for j in 0..len.min(n - i0) {
            let t = j as f32 / srf;
            arp[i0 + j] += square(fa * t, 0.125) * env_ad(t, 0.003, 0.07) * 0.09;
        }
        // Lead melody.
        if let Some(d) = p.melody[s] {
            let f = note(m, root + d as i32, 0);
            let dur = at(3);
            for j in 0..dur.min(n - i0) {
                let t = j as f32 / srf;
                let vib = 1.0 + (t * 6.0).sin() * 0.006 * (t * 3.0).min(1.0);
                lead[i0 + j] += saw(f * vib * t) * env_ad(t, 0.01, 0.35) * 0.12;
            }
        }
        // Bells on downbeats (explore colour).
        if s % 16 == 0 || s % 16 == 10 {
            let f = note(m, root + 4, 1);
            for j in 0..at(12).min(n - i0) {
                let t = j as f32 / srf;
                bell[i0 + j] += ((TAU_F * f * t).sin() + (TAU_F * f * 2.76 * t).sin() * 0.4) * (-t * 2.0).exp() * 0.07;
            }
        }
    }
    lowpass(&mut bass8, srf, 900.0);
    lowpass(&mut lead, srf, 3000.0);
    highpass(&mut hat, srf, 5000.0);
    let mut layers = vec![vec![[0.0f32; 2]; n]; LAYERS];
    for i in 0..n {
        let explore = [pad[i][0] + bell[i] + bass[i], pad[i][1] + bell[i] + bass[i]];
        let combat_mono = bass8[i] + kick[i] + snare[i] + hat[i] * 0.6 + arp[i];
        let combat = [explore[0] * 0.7 + combat_mono, explore[1] * 0.7 + combat_mono];
        let b1 = [combat[0] + lead[i], combat[1] + lead[i]];
        let b2 = [b1[0] + choir[i][0] + hat[i] * 0.6, b1[1] + choir[i][1] + hat[i] * 0.6];
        let dist = softclip((bass8[i] + kick[i]) * 2.5) * 0.25;
        let b3 = [b2[0] + dist + snare[i] * 0.5, b2[1] + dist + snare[i] * 0.5];
        for (l, v) in [explore, combat, b1, b2, b3].iter().enumerate() {
            layers[l][i] = [softclip(v[0] * 0.9), softclip(v[1] * 0.9)];
        }
    }
    Song { sr, beat_secs: beat, layers }
}

const TAU_F: f32 = std::f32::consts::TAU;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn song_is_deterministic_and_layered() {
        let m = MusicDef { tempo: 120.0, root: 45, scale: vec![0, 2, 3, 5, 7, 8, 10], seed: 3 };
        let a = render(&m, 8000);
        let b = render(&m, 8000);
        assert_eq!(a.layers.len(), LAYERS);
        assert_eq!(a.layers[4].len(), b.layers[4].len());
        assert!(a.layers[4].iter().zip(&b.layers[4]).all(|(x, y)| x == y));
        let energy = |l: &Vec<[f32; 2]>| l.iter().map(|s| s[0] * s[0]).sum::<f32>();
        assert!(energy(&a.layers[4]) > energy(&a.layers[0]), "boss layer is denser than explore");
        // 8 bars of 4/4 at 120 bpm = 16 s.
        assert!((a.layers[0].len() as f32 / 8000.0 - 16.0).abs() < 0.1);
    }
}
