//! Persistence for IRON SAINT // LEVIATHAN.
//!
//! * One virtual memory card per profile, with **15 blocks** like the PS1 card. Builds cost 1-3
//!   blocks; campaign saves, tide runs and corrupted blocks cost blocks too. The Archive and
//!   settings live in a separate *system area* that never consumes blocks (improvement: a full card
//!   can never lock you out of your permanent progress).
//! * Every file is `MAGIC | version | crc32 | RON payload`, written atomically with a `.bak` copy.
//! * Older versions migrate forward (see [`migrate`]).
//! * Builds and parts export as short base32 codes (see [`codes`]).

pub mod codes;
pub mod migrate;

use isl_ghost::HabitProfile;
use isl_sim::archive::Archive;
use isl_sim::build::Loadout;
use isl_sim::parts::PartInstance;
use isl_sim::run::TideRun;
use isl_sim::war::Campaign;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 6] = b"ISLMC\x1a";
pub const CARD_VERSION: u32 = 2;
pub const BLOCKS: u8 = 15;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a memory card file")]
    BadMagic,
    #[error("checksum mismatch (file damaged)")]
    Checksum,
    #[error("unsupported version {0}")]
    Version(u32),
    #[error("parse: {0}")]
    Parse(String),
    #[error("memory card full: need {need} blocks, {free} free")]
    Full { need: u8, free: u8 },
    #[error("bad code: {0}")]
    Code(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlockData {
    Campaign(Box<Campaign>),
    Build { loadout: Loadout, profile: HabitProfile, war: u32, tide: u32, usage: u64 },
    /// NG+ carry-over: one part or one trait, permanently corrupted.
    Corrupted { item: CorruptItem, war: u32, chain: u32 },
    Run(Box<TideRun>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CorruptItem {
    Part(PartInstance),
    Trait(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub title: String,
    pub icon: u8,
    pub blocks: u8,
    pub data: BlockData,
}

impl Entry {
    pub fn kind_name(&self) -> &'static str {
        match self.data {
            BlockData::Campaign(_) => "WAR",
            BlockData::Build { .. } => "BUILD",
            BlockData::Corrupted { .. } => "CORRUPT",
            BlockData::Run(_) => "RUN",
        }
    }
}

/// The system area: never consumes blocks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemArea {
    pub archive: Archive,
    pub profile: HabitProfile,
    /// Set when the player quit soon after a death (THE GRUDGE detection).
    pub rage_quit_flag: bool,
    pub last_death_tick: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryCard {
    pub version: u32,
    pub system: SystemArea,
    pub entries: Vec<Entry>,
}

impl MemoryCard {
    pub fn new() -> Self {
        MemoryCard { version: CARD_VERSION, ..Default::default() }
    }
    pub fn used(&self) -> u8 {
        self.entries.iter().map(|e| e.blocks).sum()
    }
    pub fn free(&self) -> u8 {
        BLOCKS.saturating_sub(self.used())
    }
    pub fn can_fit(&self, blocks: u8) -> bool {
        self.free() >= blocks
    }
    /// Add an entry; fails if the card is full (the UI then asks what to delete).
    pub fn add(&mut self, e: Entry) -> Result<usize, SaveError> {
        if !self.can_fit(e.blocks) {
            return Err(SaveError::Full { need: e.blocks, free: self.free() });
        }
        self.entries.push(e);
        Ok(self.entries.len() - 1)
    }
    /// Replace the entry at `i` (e.g. overwrite a campaign save); size may change.
    pub fn replace(&mut self, i: usize, e: Entry) -> Result<(), SaveError> {
        let old = self.entries.get(i).map_or(0, |x| x.blocks);
        if self.free() + old < e.blocks {
            return Err(SaveError::Full { need: e.blocks, free: self.free() + old });
        }
        self.entries[i] = e;
        Ok(())
    }
    pub fn delete(&mut self, i: usize) -> Option<Entry> {
        (i < self.entries.len()).then(|| self.entries.remove(i))
    }
    pub fn campaign_index(&self) -> Option<usize> {
        self.entries.iter().position(|e| matches!(e.data, BlockData::Campaign(_)))
    }
    pub fn builds(&self) -> impl Iterator<Item = (usize, &Loadout, &HabitProfile, u32, u32)> {
        self.entries.iter().enumerate().filter_map(|(i, e)| match &e.data {
            BlockData::Build { loadout, profile, war, tide, .. } => Some((i, loadout, profile, *war, *tide)),
            _ => None,
        })
    }
    pub fn corrupted(&self) -> Option<(usize, &CorruptItem, u32, u32)> {
        self.entries.iter().enumerate().find_map(|(i, e)| match &e.data {
            BlockData::Corrupted { item, war, chain } => Some((i, item, *war, *chain)),
            _ => None,
        })
    }
}

/// Campaign saves cost blocks by size of the war (more wrecks/parts = more blocks).
pub fn campaign_blocks(c: &Campaign) -> u8 {
    let parts = c.inventory[0].parts.len() + c.inventory[1].parts.len();
    (1 + parts / 40 + c.wrecks.len() / 8).clamp(1, 3) as u8
}

// ---------------------------------------------------------------------- framed files

pub fn encode_framed<T: Serialize>(v: &T, version: u32) -> Result<Vec<u8>, SaveError> {
    let payload = ron::ser::to_string(v).map_err(|e| SaveError::Parse(e.to_string()))?;
    let crc = crc32fast::hash(payload.as_bytes());
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(payload.as_bytes());
    Ok(out)
}

/// Returns (version, payload) after verifying magic and checksum.
pub fn decode_frame(bytes: &[u8]) -> Result<(u32, &str), SaveError> {
    if bytes.len() < 14 || &bytes[..6] != MAGIC {
        return Err(SaveError::BadMagic);
    }
    let version = u32::from_le_bytes(bytes[6..10].try_into().unwrap());
    let crc = u32::from_le_bytes(bytes[10..14].try_into().unwrap());
    let payload = &bytes[14..];
    if crc32fast::hash(payload) != crc {
        return Err(SaveError::Checksum);
    }
    let s = std::str::from_utf8(payload).map_err(|e| SaveError::Parse(e.to_string()))?;
    Ok((version, s))
}

pub fn parse_ron<T: DeserializeOwned>(s: &str) -> Result<T, SaveError> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(s)
        .map_err(|e| SaveError::Parse(e.to_string()))
}

pub fn decode_card(bytes: &[u8]) -> Result<MemoryCard, SaveError> {
    let (v, s) = decode_frame(bytes)?;
    migrate::migrate_card(v, s)
}

/// Atomic write: tmp file, fsync, keep previous as `.bak`, rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("bak"));
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn save_card(path: &Path, card: &MemoryCard) -> Result<(), SaveError> {
    write_atomic(path, &encode_framed(card, CARD_VERSION)?)
}

/// Load the card; falls back to the `.bak` copy if the main file is damaged.
pub fn load_card(path: &Path) -> Result<MemoryCard, SaveError> {
    match std::fs::read(path).map_err(SaveError::from).and_then(|b| decode_card(&b)) {
        Ok(c) => Ok(c),
        Err(e) => {
            let bak = path.with_extension("bak");
            if bak.exists() {
                decode_card(&std::fs::read(bak)?)
            } else {
                Err(e)
            }
        }
    }
}

/// Generic framed RON load/save (settings, keybindings).
pub fn save_ron<T: Serialize>(path: &Path, v: &T, version: u32) -> Result<(), SaveError> {
    write_atomic(path, &encode_framed(v, version)?)
}
pub fn load_ron<T: DeserializeOwned>(path: &Path) -> Result<(u32, T), SaveError> {
    let b = std::fs::read(path)?;
    let (v, s) = decode_frame(&b)?;
    Ok((v, parse_ron(s)?))
}

/// OS-correct user data directories.
pub struct Paths {
    pub data: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    pub fn discover() -> Paths {
        if let Ok(p) = std::env::var("ISL_DATA_DIR") {
            let p = PathBuf::from(p);
            return Paths { config: p.join("config"), data: p };
        }
        match directories::ProjectDirs::from("", "IronSaint", "Leviathan") {
            Some(d) => Paths { data: d.data_dir().to_path_buf(), config: d.config_dir().to_path_buf() },
            None => Paths { data: PathBuf::from("save"), config: PathBuf::from("save") },
        }
    }
    pub fn card(&self) -> PathBuf {
        self.data.join("memory_card.mcd")
    }
    pub fn settings(&self) -> PathBuf {
        self.config.join("settings.ron")
    }
    pub fn photos(&self) -> PathBuf {
        self.data.join("photos")
    }
    pub fn exports(&self) -> PathBuf {
        self.data.join("exports")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_roundtrip_and_corruption_detected() {
        let card = MemoryCard::new();
        let mut b = encode_framed(&card, CARD_VERSION).unwrap();
        assert_eq!(decode_card(&b).unwrap(), card);
        let n = b.len() - 3;
        b[n] ^= 0x20;
        assert!(matches!(decode_card(&b), Err(SaveError::Checksum)));
        assert!(matches!(decode_card(b"nope"), Err(SaveError::BadMagic)));
    }
    #[test]
    fn atomic_write_keeps_backup() {
        let dir = std::env::temp_dir().join(format!("isl-save-test-{}", std::process::id()));
        let p = dir.join("card.mcd");
        let mut c = MemoryCard::new();
        save_card(&p, &c).unwrap();
        c.system.archive.cross_harvests = 5;
        save_card(&p, &c).unwrap();
        // Damage the main file: load falls back to .bak (previous good version).
        std::fs::write(&p, b"garbage").unwrap();
        let back = load_card(&p).unwrap();
        assert_eq!(back.system.archive.cross_harvests, 0);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn block_accounting() {
        let mut c = MemoryCard::new();
        let tr = |n| Entry { title: "x".into(), icon: 0, blocks: n, data: BlockData::Corrupted { item: CorruptItem::Trait("t".into()), war: 1, chain: 1 } };
        for _ in 0..5 {
            c.add(tr(3)).unwrap();
        }
        assert_eq!(c.free(), 0);
        assert!(matches!(c.add(tr(1)), Err(SaveError::Full { need: 1, free: 0 })));
        c.delete(0);
        assert_eq!(c.free(), 3);
        c.replace(0, tr(1)).unwrap();
        assert_eq!(c.free(), 5);
    }
}
