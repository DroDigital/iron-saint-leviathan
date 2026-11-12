//! Immediate-mode bitmap UI drawn at the PSX internal resolution.
//!
//! The font is a hand-made 5x7 bitmap (see `glyphs.rs`), packed into a tiny RGBA atlas at
//! startup. Everything the UI draws is a textured quad in low-res pixel coordinates; the renderer
//! uploads the vertex list once per frame.

mod glyphs;

pub use glyphs::GLYPHS;

pub const CELL: u32 = 8;
pub const ATLAS_W: u32 = 128;
pub const ATLAS_H: u32 = 64;
pub const ADVANCE: f32 = 6.0;
pub const LINE: f32 = 9.0;

/// Special glyph codes beyond ASCII.
pub mod icon {
    pub const SOLID: char = '\u{7f}';
    pub const CURSOR: char = '\u{80}';
    pub const BLOCK: char = '\u{81}';
    pub const HEART: char = '\u{82}';
    pub const STAR: char = '\u{83}';
    pub const UP: char = '\u{84}';
    pub const DOWN: char = '\u{85}';
    pub const LEFT: char = '\u{86}';
    pub const RIGHT: char = '\u{87}';
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

/// Build the font atlas (RGBA8, white glyphs on transparent).
pub fn atlas_rgba() -> Vec<u8> {
    let mut img = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
    for (gi, g) in GLYPHS.iter().enumerate() {
        let cx = (gi as u32 % 16) * CELL;
        let cy = (gi as u32 / 16) * CELL;
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    let x = cx + col;
                    let y = cy + row as u32;
                    let i = ((y * ATLAS_W + x) * 4) as usize;
                    img[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }
    img
}

fn glyph_index(c: char) -> usize {
    let u = c as u32;
    if (32..=135).contains(&u) { (u - 32) as usize } else { ('?' as u32 - 32) as usize }
}

fn glyph_uv(c: char) -> ([f32; 2], [f32; 2]) {
    let gi = glyph_index(c) as u32;
    let x = (gi % 16) * CELL;
    let y = (gi / 16) * CELL;
    ([x as f32 / ATLAS_W as f32, y as f32 / ATLAS_H as f32], [(x + 5) as f32 / ATLAS_W as f32, (y + 7) as f32 / ATLAS_H as f32])
}

fn solid_uv() -> [f32; 2] {
    let gi = glyph_index(icon::SOLID) as u32;
    [((gi % 16) * CELL + 2) as f32 / ATLAS_W as f32, ((gi / 16) * CELL + 3) as f32 / ATLAS_H as f32]
}

pub type Rgba = [u8; 4];

pub mod col {
    use super::Rgba;
    pub const WHITE: Rgba = [255, 255, 255, 255];
    pub const GREY: Rgba = [150, 150, 160, 255];
    pub const DARK: Rgba = [70, 70, 80, 255];
    pub const BLACK: Rgba = [0, 0, 0, 255];
    pub const SHADOW: Rgba = [0, 0, 0, 200];
    pub const PANEL: Rgba = [8, 12, 40, 210];
    pub const BORDER: Rgba = [180, 180, 200, 255];
    pub const GOLD: Rgba = [255, 210, 110, 255];
    pub const RED: Rgba = [230, 60, 50, 255];
    pub const BLOOD: Rgba = [150, 20, 30, 255];
    pub const GREEN: Rgba = [110, 220, 120, 255];
    pub const CYAN: Rgba = [120, 230, 240, 255];
    pub const BLUE: Rgba = [80, 120, 230, 255];
    pub const ORANGE: Rgba = [255, 150, 50, 255];
    pub const MAGENTA: Rgba = [230, 90, 220, 255];
    pub const YELLOW: Rgba = [250, 240, 90, 255];
}

pub struct Ui {
    pub w: f32,
    pub h: f32,
    pub verts: Vec<UiVertex>,
    /// Text scale (subtitle size accessibility option).
    pub text_scale: f32,
    /// Sync-loss glitch amount 0..1.
    pub glitch: f32,
    pub time: f32,
    seed: u32,
}

impl Ui {
    pub fn new(w: f32, h: f32) -> Ui {
        Ui { w, h, verts: Vec::with_capacity(8192), text_scale: 1.0, glitch: 0.0, time: 0.0, seed: 1 }
    }

    pub fn begin(&mut self, w: f32, h: f32, time: f32) {
        self.w = w;
        self.h = h;
        self.time = time;
        self.verts.clear();
        self.seed = (time * 60.0) as u32 | 1;
    }

    fn rnd(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10_000) as f32 / 10_000.0
    }

    fn quad(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, uv0: [f32; 2], uv1: [f32; 2], c: Rgba) {
        let v = |x: f32, y: f32, u: f32, w: f32| UiVertex { pos: [x, y], uv: [u, w], color: c };
        self.verts.extend_from_slice(&[
            v(x0, y0, uv0[0], uv0[1]),
            v(x1, y0, uv1[0], uv0[1]),
            v(x1, y1, uv1[0], uv1[1]),
            v(x0, y0, uv0[0], uv0[1]),
            v(x1, y1, uv1[0], uv1[1]),
            v(x0, y1, uv0[0], uv1[1]),
        ]);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        let uv = solid_uv();
        self.quad(x, y, x + w, y + h, uv, uv, c);
    }

    pub fn outline(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        self.rect(x, y, w, 1.0, c);
        self.rect(x, y + h - 1.0, w, 1.0, c);
        self.rect(x, y, 1.0, h, c);
        self.rect(x + w - 1.0, y, 1.0, h, c);
    }

    /// PS1-style window: translucent gradient-ish panel with a bevelled border.
    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.rect(x + 2.0, y + 2.0, w, h, [0, 0, 0, 120]);
        self.rect(x, y, w, h, col::PANEL);
        self.rect(x + 1.0, y + 1.0, w - 2.0, (h * 0.3).floor(), [30, 40, 90, 90]);
        self.outline(x, y, w, h, col::BORDER);
        self.outline(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [60, 60, 110, 255]);
    }

    pub fn text_width(&self, s: &str) -> f32 {
        s.chars().count() as f32 * ADVANCE * self.text_scale.max(1.0).floor()
    }

    /// Draw text; returns the width drawn.
    pub fn text_raw(&mut self, x: f32, y: f32, s: &str, c: Rgba, scale: f32) -> f32 {
        let mut cx = x.floor();
        let y = y.floor();
        for ch in s.chars() {
            if ch == ' ' {
                cx += ADVANCE * scale;
                continue;
            }
            let (uv0, uv1) = glyph_uv(ch);
            let mut gx = cx;
            let mut gy = y;
            if self.glitch > 0.0 && self.rnd() < self.glitch * 0.25 {
                gx += (self.rnd() - 0.5) * 6.0 * self.glitch;
                gy += (self.rnd() - 0.5) * 3.0 * self.glitch;
            }
            self.quad(gx, gy, gx + 5.0 * scale, gy + 7.0 * scale, uv0, uv1, c);
            cx += ADVANCE * scale;
        }
        cx - x
    }

    /// Text with a 1px drop shadow (the classic PSX look).
    pub fn text(&mut self, x: f32, y: f32, s: &str, c: Rgba) -> f32 {
        let sc = self.text_scale.max(1.0).floor();
        self.text_raw(x + 1.0, y + 1.0, s, col::SHADOW, sc);
        self.text_raw(x, y, s, c, sc)
    }

    pub fn text_big(&mut self, x: f32, y: f32, s: &str, c: Rgba, scale: f32) -> f32 {
        self.text_raw(x + scale, y + scale, s, col::SHADOW, scale);
        self.text_raw(x, y, s, c, scale)
    }

    pub fn text_center(&mut self, cx: f32, y: f32, s: &str, c: Rgba) {
        let w = self.text_width(s);
        self.text(cx - w * 0.5, y, s, c);
    }

    pub fn text_center_big(&mut self, cx: f32, y: f32, s: &str, c: Rgba, scale: f32) {
        let w = s.chars().count() as f32 * ADVANCE * scale;
        self.text_big(cx - w * 0.5, y, s, c, scale);
    }

    pub fn text_right(&mut self, rx: f32, y: f32, s: &str, c: Rgba) {
        let w = self.text_width(s);
        self.text(rx - w, y, s, c);
    }

    /// Word-wrap text into a box; returns the number of lines drawn.
    pub fn text_wrap(&mut self, x: f32, y: f32, w: f32, s: &str, c: Rgba) -> usize {
        let lines = wrap(s, (w / (ADVANCE * self.text_scale.max(1.0).floor())) as usize);
        let lh = LINE * self.text_scale.max(1.0).floor();
        for (i, l) in lines.iter().enumerate() {
            self.text(x, y + i as f32 * lh, l, c);
        }
        lines.len()
    }

    /// A chunky segmented bar.
    pub fn bar(&mut self, x: f32, y: f32, w: f32, h: f32, frac: f32, fg: Rgba, bg: Rgba) {
        self.rect(x - 1.0, y - 1.0, w + 2.0, h + 2.0, col::BLACK);
        self.rect(x, y, w, h, bg);
        let fw = (w * frac.clamp(0.0, 1.0)).floor();
        self.rect(x, y, fw, h, fg);
        self.rect(x, y, fw, 1.0, [255, 255, 255, 70]);
        let mut sx = x + 8.0;
        while sx < x + w {
            self.rect(sx, y, 1.0, h, [0, 0, 0, 60]);
            sx += 8.0;
        }
    }

    /// Menu list with cursor; returns nothing, highlighting `sel`.
    pub fn menu(&mut self, x: f32, y: f32, items: &[String], sel: usize, enabled: &[bool]) {
        let lh = LINE * self.text_scale.max(1.0).floor() + 2.0;
        for (i, it) in items.iter().enumerate() {
            let on = enabled.get(i).copied().unwrap_or(true);
            let c = if i == sel {
                col::GOLD
            } else if on {
                col::WHITE
            } else {
                col::DARK
            };
            if i == sel {
                let blink = ((self.time * 4.0).sin() * 0.5 + 0.5) * 2.0;
                self.text(x - 9.0 + blink, y + i as f32 * lh, &icon::CURSOR.to_string(), col::GOLD);
            }
            self.text(x, y + i as f32 * lh, it, c);
        }
    }
}

pub fn wrap(s: &str, cols: usize) -> Vec<String> {
    let mut out = vec![];
    for para in s.split('\n') {
        let mut line = String::new();
        for w in para.split_whitespace() {
            if !line.is_empty() && line.len() + 1 + w.len() > cols.max(4) {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(w);
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_has_glyphs() {
        let a = atlas_rgba();
        assert_eq!(a.len(), (ATLAS_W * ATLAS_H * 4) as usize);
        let lit = a.chunks(4).filter(|p| p[3] == 255).count();
        assert!(lit > 1000);
    }
    #[test]
    fn wrapping() {
        assert_eq!(wrap("the tide comes in", 8), vec!["the tide", "comes in"]);
    }
    #[test]
    fn text_emits_quads() {
        let mut u = Ui::new(320.0, 240.0);
        u.begin(320.0, 240.0, 0.0);
        let w = u.text(0.0, 0.0, "AB C", col::WHITE);
        assert_eq!(w, 24.0);
        assert_eq!(u.verts.len(), 6 * 3 * 2); // 3 glyphs, shadow + fg
    }
}
