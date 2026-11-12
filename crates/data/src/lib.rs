//! Data-driven content for IRON SAINT // LEVIATHAN.
//!
//! All gameplay content (section 6 and 8 of the design) is described in RON files and loaded into
//! a [`Content`] database. Loading always runs [`Content::validate`], which enforces the design
//! rules that can be checked statically (cross-harvest sides, telegraph minimums, avoidability,
//! reference integrity, quirk count...).

pub mod defs;
mod validate;

pub use defs::*;
pub use validate::ValidationError;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io error reading {path}: {err}")]
    Io { path: String, err: std::io::Error },
    #[error("parse error in {path}: {err}")]
    Parse { path: String, err: String },
    #[error("missing content file {0}")]
    Missing(String),
    #[error("content validation failed:\n{0}")]
    Invalid(String),
}

/// A virtual file set: path (relative to the data root, `/` separators) -> UTF-8 contents.
/// Produced either from a directory (dev, hot reload) or from a `.pak` archive (release).
pub type FileSet = BTreeMap<String, String>;

/// The loaded, validated content database.
#[derive(Clone, Debug, Default)]
pub struct Content {
    pub parts: Vec<PartDef>,
    pub moves: Vec<MoveDef>,
    pub quirks: Vec<QuirkDef>,
    pub chassis: Vec<ChassisDef>,
    pub enemies: Vec<EnemyDef>,
    pub bosses: Vec<BossDef>,
    pub districts: Vec<DistrictDef>,
    pub omens: Vec<OmenDef>,
    pub disc_rot: Vec<DiscRotTier>,
    pub contracts: Vec<ContractDef>,
    pub events: Vec<EventDef>,
    pub conditions: Vec<ConditionDef>,
    pub endings: Vec<EndingDef>,
    pub lore: LoreDef,
    pub tuning: Option<TuningDef>,
    part_ix: HashMap<String, usize>,
    move_ix: HashMap<String, usize>,
    quirk_ix: HashMap<String, usize>,
    chassis_ix: HashMap<String, usize>,
    enemy_ix: HashMap<String, usize>,
    boss_ix: HashMap<String, usize>,
    district_ix: HashMap<String, usize>,
}

fn parse<T: serde::de::DeserializeOwned>(files: &FileSet, path: &str) -> Result<T, LoadError> {
    let src = files.get(path).ok_or_else(|| LoadError::Missing(path.to_string()))?;
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(src)
        .map_err(|e| LoadError::Parse { path: path.to_string(), err: e.to_string() })
}

impl Content {
    /// Read every `.ron` file under `root` into a [`FileSet`].
    pub fn read_dir(root: &Path) -> Result<FileSet, LoadError> {
        let mut out = FileSet::new();
        fn walk(base: &Path, dir: &Path, out: &mut FileSet) -> Result<(), LoadError> {
            let rd = std::fs::read_dir(dir).map_err(|err| LoadError::Io { path: dir.display().to_string(), err })?;
            let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    walk(base, &p, out)?;
                } else if p.extension().is_some_and(|e| e == "ron") {
                    let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                    let s = std::fs::read_to_string(&p).map_err(|err| LoadError::Io { path: p.display().to_string(), err })?;
                    out.insert(rel, s);
                }
            }
            Ok(())
        }
        walk(root, root, &mut out)?;
        Ok(out)
    }

    /// Load from a directory (dev builds).
    pub fn load_dir(root: &Path) -> Result<Content, LoadError> {
        Self::from_files(&Self::read_dir(root)?)
    }

    /// Parse a file set, build indices and validate.
    pub fn from_files(files: &FileSet) -> Result<Content, LoadError> {
        let mut c = Content {
            moves: parse(files, "moves.ron")?,
            quirks: parse(files, "quirks.ron")?,
            chassis: parse(files, "chassis.ron")?,
            enemies: parse(files, "enemies.ron")?,
            bosses: parse(files, "bosses.ron")?,
            districts: parse(files, "districts.ron")?,
            omens: parse(files, "omens.ron")?,
            disc_rot: parse(files, "disc_rot.ron")?,
            contracts: parse(files, "contracts.ron")?,
            events: parse(files, "events.ron")?,
            conditions: parse(files, "conditions.ron")?,
            endings: parse(files, "endings.ron")?,
            lore: parse(files, "lore.ron")?,
            tuning: Some(parse(files, "tuning.ron")?),
            ..Default::default()
        };
        for (path, _) in files.range("parts/".to_string()..) {
            if !path.starts_with("parts/") {
                break;
            }
            c.parts.push(parse(files, path)?);
        }
        c.reindex();
        let errs = c.validate();
        if !errs.is_empty() {
            let msg = errs.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n");
            return Err(LoadError::Invalid(msg));
        }
        Ok(c)
    }

    pub fn reindex(&mut self) {
        fn ix<T>(v: &[T], id: impl Fn(&T) -> &str) -> HashMap<String, usize> {
            v.iter().enumerate().map(|(i, x)| (id(x).to_string(), i)).collect()
        }
        self.part_ix = ix(&self.parts, |p| &p.id);
        self.move_ix = ix(&self.moves, |p| &p.id);
        self.quirk_ix = ix(&self.quirks, |p| &p.id);
        self.chassis_ix = ix(&self.chassis, |p| &p.id);
        self.enemy_ix = ix(&self.enemies, |p| &p.id);
        self.boss_ix = ix(&self.bosses, |p| &p.id);
        self.district_ix = ix(&self.districts, |p| &p.id);
    }

    pub fn tuning(&self) -> &TuningDef {
        self.tuning.as_ref().expect("tuning loaded")
    }
    pub fn part(&self, id: &str) -> Option<&PartDef> {
        self.part_ix.get(id).map(|&i| &self.parts[i])
    }
    pub fn mv(&self, id: &str) -> Option<&MoveDef> {
        self.move_ix.get(id).map(|&i| &self.moves[i])
    }
    pub fn quirk(&self, id: &str) -> Option<&QuirkDef> {
        self.quirk_ix.get(id).map(|&i| &self.quirks[i])
    }
    pub fn chassis_def(&self, id: &str) -> Option<&ChassisDef> {
        self.chassis_ix.get(id).map(|&i| &self.chassis[i])
    }
    pub fn enemy(&self, id: &str) -> Option<&EnemyDef> {
        self.enemy_ix.get(id).map(|&i| &self.enemies[i])
    }
    pub fn boss(&self, id: &str) -> Option<&BossDef> {
        self.boss_ix.get(id).map(|&i| &self.bosses[i])
    }
    pub fn district(&self, id: &str) -> Option<&DistrictDef> {
        self.district_ix.get(id).map(|&i| &self.districts[i])
    }
    pub fn district_index(&self, id: &str) -> Option<usize> {
        self.district_ix.get(id).copied()
    }
    pub fn omen(&self, id: &str) -> Option<&OmenDef> {
        self.omens.iter().find(|o| o.id == id)
    }
    pub fn parts_of(&self, side: Side, slot: Slot) -> impl Iterator<Item = &PartDef> {
        self.parts.iter().filter(move |p| p.side == side && p.slot == slot)
    }
    pub fn starting_chassis(&self, side: Side) -> impl Iterator<Item = &ChassisDef> {
        self.chassis.iter().filter(move |c| c.side == side && matches!(c.unlock, Unlock::Always))
    }
}

/// Locate the `data/` directory in dev: walk up from the executable and the CWD.
pub fn find_data_dir() -> Option<std::path::PathBuf> {
    let mut cands = vec![];
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            cands.push(p.to_path_buf());
        }
    }
    for c in cands {
        let mut d: Option<&Path> = Some(&c);
        while let Some(dir) = d {
            let p = dir.join("data");
            if p.join("moves.ron").exists() {
                return Some(p);
            }
            d = dir.parent();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_content_loads_and_validates() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let c = match Content::load_dir(&dir) {
            Ok(c) => c,
            Err(e) => panic!("{e}"),
        };
        assert!(c.quirks.len() >= 30, "need at least 30 quirks");
        assert!(c.parts.len() >= 100, "parts: {}", c.parts.len());
        assert_eq!(c.districts.iter().filter(|d| !d.finale).count(), 6);
        assert_eq!(c.starting_chassis(Side::Saint).count(), 4);
        assert_eq!(c.starting_chassis(Side::Kaiju).count(), 4);
        assert_eq!(c.chassis.len(), 10);
        assert!(c.bosses.iter().filter(|b| b.side == Side::Saint).count() >= 6);
        assert!(c.bosses.iter().filter(|b| b.side == Side::Kaiju).count() >= 6);
    }
}
