//! `.pak` archive: `b"ISLPAK1\0" | count:u32 | entries (name_len:u16 name offset:u32 len:u32 crc:u32) | blobs`.

use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum PakError {
    #[error("not a pak file")]
    Magic,
    #[error("truncated pak")]
    Truncated,
    #[error("crc mismatch in {0}")]
    Crc(String),
}

const MAGIC: &[u8; 8] = b"ISLPAK1\0";

pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&(files.len() as u32).to_le_bytes());
    let table_len: usize = files.keys().map(|k| 2 + k.len() + 12).sum();
    let mut offset = (header.len() + table_len) as u32;
    let mut table = Vec::new();
    let mut blobs = Vec::new();
    for (name, data) in files {
        table.extend_from_slice(&(name.len() as u16).to_le_bytes());
        table.extend_from_slice(name.as_bytes());
        table.extend_from_slice(&offset.to_le_bytes());
        table.extend_from_slice(&(data.len() as u32).to_le_bytes());
        table.extend_from_slice(&crc32fast::hash(data).to_le_bytes());
        blobs.extend_from_slice(data);
        offset += data.len() as u32;
    }
    header.extend(table);
    header.extend(blobs);
    header
}

pub fn read(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, PakError> {
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return Err(PakError::Magic);
    }
    let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut i = 12;
    let mut out = BTreeMap::new();
    let get = |a: usize, b: usize| bytes.get(a..b).ok_or(PakError::Truncated);
    for _ in 0..n {
        let nl = u16::from_le_bytes(get(i, i + 2)?.try_into().unwrap()) as usize;
        i += 2;
        let name = String::from_utf8_lossy(get(i, i + nl)?).to_string();
        i += nl;
        let off = u32::from_le_bytes(get(i, i + 4)?.try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(get(i + 4, i + 8)?.try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(get(i + 8, i + 12)?.try_into().unwrap());
        i += 12;
        let data = get(off, off + len)?.to_vec();
        if crc32fast::hash(&data) != crc {
            return Err(PakError::Crc(name));
        }
        out.insert(name, data);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("a.ron".to_string(), b"hello".to_vec());
        m.insert("parts/b.ron".to_string(), vec![1, 2, 3]);
        let b = super::write(&m);
        assert_eq!(super::read(&b).unwrap(), m);
        let mut bad = b.clone();
        let n = bad.len() - 1;
        bad[n] ^= 1;
        assert!(super::read(&bad).is_err());
    }
}
