//! "Fake FMV" cutscenes rendered in-engine through the PS1-video-compression post pass
//! (posterize, 8x8 macroblocks, chroma smear) - no video codec dependencies.

use crate::game::Screen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    City,
    Saint,
    Kaiju,
    Both,
    Chimera,
    Void,
}

pub struct Shot {
    pub dur: f32,
    pub lines: Vec<String>,
    pub scene: Scene,
}

pub struct Fmv {
    pub id: String,
    pub t: f32,
    pub shots: Vec<Shot>,
    pub next: Option<Box<Screen>>,
}

impl Fmv {
    pub fn total(&self) -> f32 {
        self.shots.iter().map(|s| s.dur).sum()
    }
    pub fn current(&self) -> Option<(&Shot, f32)> {
        let mut t = self.t;
        for s in &self.shots {
            if t < s.dur {
                return Some((s, t / s.dur));
            }
            t -= s.dur;
        }
        None
    }
    pub fn intro(lines: Vec<String>, next: Box<Screen>) -> Fmv {
        let scenes = [Scene::City, Scene::Kaiju, Scene::Saint, Scene::Both];
        let shots =
            lines.into_iter().enumerate().map(|(i, l)| Shot { dur: 3.2, lines: vec![l], scene: scenes[i % scenes.len()] }).collect();
        Fmv { id: "intro".into(), t: 0.0, shots, next: Some(next) }
    }
    pub fn ending(id: &str, lines: Vec<String>, next: Box<Screen>) -> Fmv {
        let scene = match id {
            "drown_the_city" => Scene::Kaiju,
            "the_city_stands" => Scene::Saint,
            "become_the_saint" => Scene::Saint,
            "tide_between" => Scene::Chimera,
            _ => Scene::Void,
        };
        let shots = lines.into_iter().map(|l| Shot { dur: 3.6, lines: vec![l], scene }).collect();
        Fmv { id: id.into(), t: 0.0, shots, next: Some(next) }
    }
}
