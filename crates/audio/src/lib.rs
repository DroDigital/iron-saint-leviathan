//! Audio for IRON SAINT // LEVIATHAN on top of kira.
//!
//! * **SPU mode** (default on): everything is synthesized at ~22 kHz with a gentle low-pass, like
//!   the PS1 SPU. Toggling re-renders the bank.
//! * **Reverb presets** modelled on PS1 SPU modes (Room, Studio, Hall, Space Echo), one mixer
//!   track each; districts pick one.
//! * **Adaptive music**: five synchronized layers crossfaded on the beat.
//! * **Positional audio**: distance attenuation and panning computed from the listener.
//!
//! If no audio device exists (CI, headless) every call is a silent no-op.

pub mod music;
pub mod synth;

use glam::Vec3;
use isl_data::{MusicDef, Reverb};
use kira::effect::delay::DelayBuilder;
use kira::effect::reverb::ReverbBuilder;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Mix, Panning, PlaybackRate, Tween};
use music::{Layer, LAYERS};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;
pub use synth::Sfx;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
    pub spu: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings { master: 0.8, sfx: 0.9, music: 0.6, spu: true }
    }
}

fn db(lin: f32) -> Decibels {
    if lin <= 0.001 { Decibels::SILENCE } else { Decibels(20.0 * lin.log10()) }
}

fn to_data(mono: &[f32], sr: u32) -> StaticSoundData {
    let frames: Arc<[Frame]> = mono.iter().map(|s| Frame::from_mono(*s)).collect();
    StaticSoundData { sample_rate: sr, frames, settings: StaticSoundSettings::default(), slice: None }
}

struct MusicState {
    handles: Vec<StaticSoundHandle>,
    beat: f32,
    elapsed: f32,
    current: usize,
    pending: Option<usize>,
    key: u64,
}

pub struct Audio {
    mgr: Option<AudioManager<DefaultBackend>>,
    reverb_tracks: Vec<TrackHandle>,
    music_track: Option<TrackHandle>,
    bank: HashMap<Sfx, StaticSoundData>,
    pub settings: AudioSettings,
    listener: (Vec3, Vec3),
    reverb: Reverb,
    music: Option<MusicState>,
    loading: Option<(u64, Receiver<music::Song>)>,
    /// Per-sfx cooldown to avoid machine-gunning identical sounds in one frame.
    recent: HashMap<Sfx, f32>,
    time: f32,
    pub muted_reason: Option<String>,
}

impl Audio {
    pub fn new(settings: AudioSettings) -> Audio {
        let mut a = Audio {
            mgr: None,
            reverb_tracks: vec![],
            music_track: None,
            bank: HashMap::new(),
            settings,
            listener: (Vec3::ZERO, Vec3::X),
            reverb: Reverb::Hall,
            music: None,
            loading: None,
            recent: HashMap::new(),
            time: 0.0,
            muted_reason: None,
        };
        if std::env::var("ISL_NO_AUDIO").is_ok() {
            a.muted_reason = Some("ISL_NO_AUDIO set".into());
            return a;
        }
        match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(mut m) => {
                for r in [Reverb::Room, Reverb::Studio, Reverb::Hall, Reverb::SpaceEcho] {
                    let (fb, damp, mix) = match r {
                        Reverb::Room => (0.55, 0.6, 0.14),
                        Reverb::Studio => (0.68, 0.45, 0.2),
                        Reverb::Hall => (0.86, 0.3, 0.28),
                        Reverb::SpaceEcho => (0.8, 0.5, 0.25),
                    };
                    let mut tb = TrackBuilder::new().with_effect(ReverbBuilder::new().feedback(fb).damping(damp).mix(Mix(mix)));
                    if r == Reverb::SpaceEcho {
                        tb = tb.with_effect(DelayBuilder::new().delay_time(Duration::from_millis(330)).feedback(Decibels(-7.0)).mix(Mix(0.3)));
                    }
                    if let Ok(t) = m.add_sub_track(tb) {
                        a.reverb_tracks.push(t);
                    }
                }
                a.music_track = m.add_sub_track(TrackBuilder::new().with_effect(ReverbBuilder::new().feedback(0.6).damping(0.5).mix(Mix(0.12)))).ok();
                a.mgr = Some(m);
            }
            Err(e) => {
                tracing::warn!("audio disabled: {e}");
                a.muted_reason = Some(e.to_string());
            }
        }
        a.rebuild_bank();
        a
    }

    pub fn enabled(&self) -> bool {
        self.mgr.is_some()
    }

    fn sample_rate(&self) -> u32 {
        if self.settings.spu { 22_050 } else { 44_100 }
    }

    /// (Re)synthesize every sound effect at the current SPU setting.
    pub fn rebuild_bank(&mut self) {
        if self.mgr.is_none() {
            return;
        }
        let sr = self.sample_rate();
        self.bank.clear();
        for s in Sfx::ALL {
            let mut buf = synth::render_sfx(s, sr as f32);
            if self.settings.spu {
                synth::lowpass(&mut buf, sr as f32, 7_000.0);
            }
            self.bank.insert(s, to_data(&buf, sr));
        }
    }

    pub fn set_spu(&mut self, on: bool) {
        if self.settings.spu != on {
            self.settings.spu = on;
            self.rebuild_bank();
            if let Some(k) = self.music.as_ref().map(|m| m.key) {
                self.music = None;
                self.loading = None;
                let _ = k;
            }
        }
    }

    pub fn set_listener(&mut self, pos: Vec3, right: Vec3) {
        self.listener = (pos, right.normalize_or(Vec3::X));
    }

    pub fn set_reverb(&mut self, r: Reverb) {
        self.reverb = r;
    }

    /// Play a sound; `pos` = None for UI/2D sounds.
    pub fn play(&mut self, s: Sfx, pos: Option<Vec3>, volume: f32, pitch: f32) {
        let Some(m) = self.mgr.as_mut() else { return };
        if self.recent.get(&s).is_some_and(|t| self.time - t < 0.03) {
            return;
        }
        self.recent.insert(s, self.time);
        let Some(data) = self.bank.get(&s).cloned() else { return };
        let (mut vol, mut pan) = (volume, 0.0);
        if let Some(p) = pos {
            let d = p - self.listener.0;
            let dist = d.length();
            vol *= (1.0 / (1.0 + dist / 25.0)).powf(1.3);
            pan = (d.normalize_or_zero().dot(self.listener.1) * 0.8).clamp(-1.0, 1.0);
        }
        vol *= self.settings.sfx * self.settings.master;
        if vol < 0.01 {
            return;
        }
        let settings = StaticSoundSettings::new().volume(db(vol)).panning(Panning(pan)).playback_rate(PlaybackRate(pitch as f64));
        let data = data.with_settings(settings);
        let idx = match self.reverb {
            Reverb::Room => 0,
            Reverb::Studio => 1,
            Reverb::Hall => 2,
            Reverb::SpaceEcho => 3,
        };
        let res = match self.reverb_tracks.get_mut(idx) {
            Some(t) => t.play(data).map(|_| ()).map_err(|e| e.to_string()),
            None => m.play(data).map(|_| ()).map_err(|e| e.to_string()),
        };
        if let Err(e) = res {
            tracing::debug!("sfx dropped: {e}");
        }
    }

    /// Start composing (in the background) and then playing a district's music.
    pub fn load_music(&mut self, key: u64, m: &MusicDef) {
        if self.mgr.is_none() || self.music.as_ref().is_some_and(|s| s.key == key) || self.loading.as_ref().is_some_and(|l| l.0 == key) {
            return;
        }
        let (tx, rx) = channel();
        let m = m.clone();
        let sr = self.sample_rate();
        std::thread::spawn(move || {
            let _ = tx.send(music::render(&m, sr));
        });
        self.loading = Some((key, rx));
    }

    pub fn stop_music(&mut self) {
        if let Some(ms) = self.music.as_mut() {
            for h in ms.handles.iter_mut() {
                h.stop(Tween { duration: Duration::from_millis(800), ..Default::default() });
            }
        }
        self.music = None;
        self.loading = None;
    }

    /// Request a layer; the switch happens on the next beat.
    pub fn set_layer(&mut self, l: Layer) {
        if let Some(ms) = self.music.as_mut() {
            if ms.current != l as usize && ms.pending != Some(l as usize) {
                ms.pending = Some(l as usize);
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some((key, rx)) = &self.loading {
            if let Ok(song) = rx.try_recv() {
                let key = *key;
                self.loading = None;
                self.start_song(key, song);
            }
        }
        let mv = self.settings.music * self.settings.master;
        if let Some(ms) = self.music.as_mut() {
            let before = (ms.elapsed / ms.beat).floor();
            ms.elapsed += dt;
            let after = (ms.elapsed / ms.beat).floor();
            if after != before {
                if let Some(p) = ms.pending.take() {
                    let tw = Tween { duration: Duration::from_secs_f32(ms.beat), ..Default::default() };
                    ms.handles[ms.current].set_volume(Decibels::SILENCE, tw);
                    ms.handles[p].set_volume(db(mv), tw);
                    ms.current = p;
                }
            }
        }
    }

    pub fn apply_volumes(&mut self) {
        let mv = self.settings.music * self.settings.master;
        if let Some(ms) = self.music.as_mut() {
            let c = ms.current;
            ms.handles[c].set_volume(db(mv), Tween::default());
        }
    }

    fn start_song(&mut self, key: u64, song: music::Song) {
        self.stop_music();
        let (Some(_), Some(track)) = (self.mgr.as_mut(), self.music_track.as_mut()) else { return };
        let mut handles = vec![];
        let mv = self.settings.music * self.settings.master;
        for (i, l) in song.layers.iter().enumerate().take(LAYERS) {
            let frames: Arc<[Frame]> = l.iter().map(|s| Frame { left: s[0], right: s[1] }).collect();
            let data = StaticSoundData {
                sample_rate: song.sr,
                frames,
                settings: StaticSoundSettings::new().loop_region(..).volume(if i == 0 { db(mv) } else { Decibels::SILENCE }),
                slice: None,
            };
            match track.play(data) {
                Ok(h) => handles.push(h),
                Err(e) => {
                    tracing::warn!("music: {e}");
                    return;
                }
            }
        }
        self.music = Some(MusicState { handles, beat: song.beat_secs, elapsed: 0.0, current: 0, pending: None, key });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_sfx_render() {
        for s in Sfx::ALL {
            let b = synth::render_sfx(s, 22_050.0);
            assert!(!b.is_empty(), "{s:?}");
            assert!(b.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }
    #[test]
    fn silent_without_device() {
        // SAFETY: test-only env mutation before constructing audio.
        unsafe { std::env::set_var("ISL_NO_AUDIO", "1") };
        let mut a = Audio::new(AudioSettings::default());
        assert!(!a.enabled());
        a.play(Sfx::Clank, Some(Vec3::ONE), 1.0, 1.0);
        a.update(0.016);
    }
}
