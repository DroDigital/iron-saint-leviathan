//! Player settings (saved to the OS config dir) including accessibility options.

use isl_input::Bindings;
use serde::{Deserialize, Serialize};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Palette {
    Standard,
    Deuteranopia,
    Protanopia,
    Tritanopia,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Index into RESOLUTIONS.
    pub resolution: usize,
    pub widescreen: bool,
    pub integer_scale: bool,
    pub vertex_jitter: f32,
    pub dither: f32,
    pub crt: bool,
    pub composite: bool,
    pub scanlines: f32,
    pub vsync: bool,
    pub fullscreen: bool,
    pub pause_unfocused: bool,
    // Accessibility.
    pub screen_shake: f32,
    pub photosensitive: bool,
    pub subtitle_size: f32,
    pub palette: Palette,
    pub steady_hands: bool,
    pub old_bones: bool,
    pub insurance: bool,
    pub show_iframes: bool,
    pub rumble: bool,
    // Audio.
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
    pub spu: bool,
    pub bindings: Bindings,
    pub dev_overlay: bool,
}

pub const RESOLUTIONS: [(u32, u32); 5] = [(320, 240), (256, 224), (368, 240), (512, 448), (640, 480)];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            resolution: 0,
            widescreen: false,
            integer_scale: true,
            vertex_jitter: 1.0,
            dither: 1.0,
            crt: false,
            composite: false,
            scanlines: 0.35,
            vsync: true,
            fullscreen: false,
            pause_unfocused: true,
            screen_shake: 1.0,
            photosensitive: false,
            subtitle_size: 1.0,
            palette: Palette::Standard,
            steady_hands: false,
            old_bones: false,
            insurance: false,
            show_iframes: false,
            rumble: true,
            master: 0.8,
            sfx: 0.9,
            music: 0.6,
            spu: true,
            bindings: Bindings::default(),
            dev_overlay: false,
        }
    }
}

impl Settings {
    /// Internal resolution, widened to 16:9 when widescreen is on.
    pub fn low_res(&self) -> (u32, u32) {
        let (w, h) = RESOLUTIONS[self.resolution.min(RESOLUTIONS.len() - 1)];
        if self.widescreen {
            (((h as f32 * 16.0 / 9.0) as u32 + 1) & !1, h)
        } else {
            (w, h)
        }
    }
    pub fn aspect(&self) -> f32 {
        if self.widescreen { 16.0 / 9.0 } else { 4.0 / 3.0 }
    }
    pub fn assists(&self) -> isl_sim::world::Assists {
        isl_sim::world::Assists { steady_hands: self.steady_hands, old_bones: self.old_bones, insurance: self.insurance }
    }

    /// Colour used for "danger" telegraph accents, remapped for colour-blind palettes. Telegraphs
    /// never rely on colour alone (shape + sound always accompany it).
    pub fn danger(&self) -> [u8; 4] {
        match self.palette {
            Palette::Standard => [230, 60, 50, 255],
            Palette::Deuteranopia | Palette::Protanopia => [250, 160, 0, 255],
            Palette::Tritanopia => [230, 40, 120, 255],
        }
    }
    pub fn safe(&self) -> [u8; 4] {
        match self.palette {
            Palette::Standard => [110, 220, 120, 255],
            Palette::Deuteranopia | Palette::Protanopia => [80, 160, 255, 255],
            Palette::Tritanopia => [60, 200, 200, 255],
        }
    }

    pub fn load(path: &std::path::Path) -> Settings {
        match isl_save::load_ron::<Settings>(path) {
            Ok((_, s)) => s,
            Err(_) => Settings::default(),
        }
    }
    pub fn save(&self, path: &std::path::Path) {
        if let Err(e) = isl_save::save_ron(path, self, SETTINGS_VERSION) {
            tracing::warn!("settings not saved: {e}");
        }
    }
}
