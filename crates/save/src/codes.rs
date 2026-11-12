//! Memory-card export codes: builds and parts as short base32 strings friends can type in.
//!
//! Layout (little endian): `ver:u8 kind:u8 side:u8 | per part: present:u8 [hash:u16 cond:u8
//! nq:u8 quirk-hash:u16 x nq] | paint 6 bytes | name len + bytes | crc16`. Part and quirk ids are
//! stored as 16-bit FNV hashes and resolved against the content database on import.

use crate::SaveError;
use isl_data::*;
use isl_sim::build::Loadout;
use isl_sim::parts::{Condition, Origin, PartInstance};
use isl_sim::rng::hash_str;

const VER: u8 = 1;

fn h16(s: &str) -> u16 {
    let h = hash_str(s);
    (h ^ (h >> 16) ^ (h >> 32) ^ (h >> 48)) as u16
}

fn crc16(b: &[u8]) -> u16 {
    (crc32fast::hash(b) & 0xFFFF) as u16
}

fn put_part(out: &mut Vec<u8>, p: &PartInstance) {
    out.extend_from_slice(&h16(&p.def).to_le_bytes());
    out.push(p.condition as u8);
    let q: Vec<&String> = p.quirks.iter().take(4).collect();
    out.push(q.len() as u8);
    for x in q {
        out.extend_from_slice(&h16(x).to_le_bytes());
    }
}

fn wrap(mut body: Vec<u8>) -> String {
    let c = crc16(&body);
    body.extend_from_slice(&c.to_le_bytes());
    let s = data_encoding::BASE32_NOPAD.encode(&body);
    // Group in fives for readability: ABCDE-FGHIJ-...
    s.as_bytes().chunks(5).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("-")
}

fn unwrap_code(code: &str) -> Result<Vec<u8>, SaveError> {
    let clean: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    let b = data_encoding::BASE32_NOPAD.decode(clean.as_bytes()).map_err(|e| SaveError::Code(e.to_string()))?;
    if b.len() < 5 {
        return Err(SaveError::Code("too short".into()));
    }
    let (body, c) = b.split_at(b.len() - 2);
    if crc16(body).to_le_bytes() != [c[0], c[1]] {
        return Err(SaveError::Code("checksum".into()));
    }
    if body[0] != VER {
        return Err(SaveError::Code(format!("version {}", body[0])));
    }
    Ok(body.to_vec())
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}
impl Reader<'_> {
    fn u8(&mut self) -> Result<u8, SaveError> {
        let v = *self.b.get(self.i).ok_or_else(|| SaveError::Code("truncated".into()))?;
        self.i += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, SaveError> {
        Ok(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }
}

fn get_part(r: &mut Reader, content: &Content, uid: u64, code: &str) -> Result<PartInstance, SaveError> {
    let h = r.u16()?;
    let cond = match r.u8()? {
        0 => Condition::Pristine,
        1 => Condition::Scarred,
        _ => Condition::Mangled,
    };
    let nq = r.u8()?;
    let mut quirks = vec![];
    for _ in 0..nq {
        let qh = r.u16()?;
        if let Some(q) = content.quirks.iter().find(|q| h16(&q.id) == qh) {
            quirks.push(q.id.clone());
        }
    }
    let def = content.parts.iter().find(|p| h16(&p.id) == h).ok_or_else(|| SaveError::Code("unknown part".into()))?;
    Ok(PartInstance { uid, def: def.id.clone(), condition: cond, quirks, origin: Origin::Import(code.chars().take(11).collect()), memory: def.memory.clone() })
}

pub fn export_build(lo: &Loadout) -> String {
    let mut out = vec![VER, 0, lo.side as u8];
    for s in &lo.slots {
        match s {
            Some(p) => {
                out.push(1);
                put_part(&mut out, p);
            }
            None => out.push(0),
        }
    }
    for c in lo.paint {
        out.extend_from_slice(&c);
    }
    let name: Vec<u8> = lo.name.bytes().filter(|b| b.is_ascii_graphic() || *b == b' ').take(16).collect();
    out.push(name.len() as u8);
    out.extend_from_slice(&name);
    wrap(out)
}

pub fn import_build(code: &str, content: &Content, uid_base: u64) -> Result<Loadout, SaveError> {
    let b = unwrap_code(code)?;
    let mut r = Reader { b: &b, i: 1 };
    if r.u8()? != 0 {
        return Err(SaveError::Code("not a build code".into()));
    }
    let side = if r.u8()? == 0 { Side::Saint } else { Side::Kaiju };
    let mut slots: [Option<PartInstance>; 7] = Default::default();
    for (i, slot) in slots.iter_mut().enumerate() {
        if r.u8()? == 1 {
            *slot = Some(get_part(&mut r, content, uid_base + i as u64, code)?);
        }
    }
    let mut paint = [[0u8; 3]; 2];
    for c in paint.iter_mut() {
        *c = [r.u8()?, r.u8()?, r.u8()?];
    }
    let n = r.u8()? as usize;
    let mut name = String::new();
    for _ in 0..n {
        name.push(r.u8()? as char);
    }
    Ok(Loadout { name, side, chassis: None, slots, paint, decal: 0, stencil: String::new() })
}

pub fn export_part(p: &PartInstance) -> String {
    let mut out = vec![VER, 1];
    put_part(&mut out, p);
    wrap(out)
}

/// Imported parts sometimes carry import-only quirks (they only drop this way).
pub fn import_part(code: &str, content: &Content, uid: u64, rng: &mut isl_sim::rng::SimRng) -> Result<PartInstance, SaveError> {
    let b = unwrap_code(code)?;
    let mut r = Reader { b: &b, i: 1 };
    if r.u8()? != 1 {
        return Err(SaveError::Code("not a part code".into()));
    }
    let mut p = get_part(&mut r, content, uid, code)?;
    let side = content.part(&p.def).map_or(Side::Saint, |d| d.side);
    for q in isl_sim::parts::roll_quirks(rng, content, side, isl_sim::parts::Condition::Pristine, true) {
        if !p.quirks.contains(&q) {
            p.quirks.push(q);
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn build_and_part_codes_roundtrip() {
        let c = Content::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
        let mut lo = Loadout::premade(c.chassis_def("st_wren").unwrap(), &c, 1);
        lo.slots[2].as_mut().unwrap().quirks = vec!["twitching".into(), "old_grudge".into()];
        lo.slots[2].as_mut().unwrap().condition = Condition::Mangled;
        lo.slots[6] = None;
        lo.name = "KESTREL-9".into();
        let code = export_build(&lo);
        assert!(code.len() < 160, "code stays short: {} chars", code.len());
        let back = import_build(&code, &c, 100).unwrap();
        assert_eq!(back.name, lo.name);
        assert_eq!(back.side, lo.side);
        assert_eq!(back.paint, lo.paint);
        for i in 0..7 {
            assert_eq!(back.slots[i].as_ref().map(|p| &p.def), lo.slots[i].as_ref().map(|p| &p.def));
        }
        assert_eq!(back.slots[2].as_ref().unwrap().quirks, vec!["twitching".to_string(), "old_grudge".to_string()]);
        assert_eq!(back.slots[2].as_ref().unwrap().condition, Condition::Mangled);
        // A typo fails the checksum instead of importing garbage.
        let mut bad = code.clone().into_bytes();
        bad[3] = if bad[3] == b'A' { b'B' } else { b'A' };
        assert!(import_build(std::str::from_utf8(&bad).unwrap(), &c, 1).is_err());
        let pc = export_part(lo.slots[0].as_ref().unwrap());
        let mut rng = isl_sim::rng::SimRng::new(1);
        let p = import_part(&pc.to_lowercase(), &c, 1, &mut rng).unwrap();
        assert_eq!(p.def, "st_wren_frame");
        assert!(matches!(p.origin, Origin::Import(_)));
    }

    #[test]
    fn hashes_are_unique_across_content() {
        let c = Content::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
        let mut seen = std::collections::HashSet::new();
        for p in &c.parts {
            assert!(seen.insert(h16(&p.id)), "part hash collision on {}", p.id);
        }
        let mut seen = std::collections::HashSet::new();
        for q in &c.quirks {
            assert!(seen.insert(h16(&q.id)), "quirk hash collision on {}", q.id);
        }
    }
}
