//! Save migrations. Every historical card schema is kept here as a frozen struct; loading an old
//! file deserializes the frozen schema and converts it forward one version at a time, so saves from
//! earlier versions migrate without data loss.

use crate::*;
use isl_sim::archive::Archive;

/// Version 1 (pre-release builds): a flat array of 15 optional single-block slots and the archive
/// stored inline, no habit profile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardV1 {
    pub archive: Archive,
    pub slots: Vec<Option<SlotV1>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlotV1 {
    pub title: String,
    pub data: BlockData,
}

fn v1_to_v2(c: CardV1) -> MemoryCard {
    let mut entries = vec![];
    for s in c.slots.into_iter().flatten() {
        let blocks = match &s.data {
            BlockData::Campaign(c) => campaign_blocks(c),
            BlockData::Build { .. } => 1,
            _ => 1,
        };
        entries.push(Entry { title: s.title, icon: 0, blocks, data: s.data });
    }
    MemoryCard { version: 2, system: SystemArea { archive: c.archive, ..Default::default() }, entries }
}

pub fn migrate_card(version: u32, payload: &str) -> Result<MemoryCard, SaveError> {
    match version {
        1 => Ok(v1_to_v2(parse_ron::<CardV1>(payload)?)),
        2 => parse_ron::<MemoryCard>(payload),
        v => Err(SaveError::Version(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v1_cards_migrate_without_loss() {
        let mut arch = Archive::default();
        arch.codex.insert("iron_saint_blade".into());
        arch.cross_harvests = 17;
        let v1 = CardV1 {
            archive: arch.clone(),
            slots: vec![
                None,
                Some(SlotV1 {
                    title: "TRAIT".into(),
                    data: BlockData::Corrupted { item: CorruptItem::Trait("old_grudge".into()), war: 2, chain: 1 },
                }),
                None,
            ],
        };
        let bytes = encode_framed(&v1, 1).unwrap();
        let card = decode_card(&bytes).unwrap();
        assert_eq!(card.version, 2);
        assert_eq!(card.system.archive, arch);
        assert_eq!(card.entries.len(), 1);
        assert_eq!(card.entries[0].title, "TRAIT");
        assert!(matches!(decode_card(&encode_framed(&v1, 99).unwrap()), Err(SaveError::Version(99))));
    }
}
