extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ternos::services::db::{
    DbKind, InstalledDbIdentity, InstalledDbMeta, RecordDatabase,
};

const STATE_DB_NAME: &[u8] = b"TernState";
const STATE_DB_TYPE: [u8; 4] = *b"DATA";
const STATE_DB_CREATOR: [u8; 4] = *b"TERN";
const STATE_DB_VERSION: u16 = 1;

const TAG_RESUME: [u8; 4] = *b"RSME";
const TAG_BOOKS: [u8; 4] = *b"BOOK";
const TAG_RECENTS: [u8; 4] = *b"RCNT";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LauncherStateDb {
    pub resume: Option<String>,
    pub book_positions: Vec<(String, usize)>,
    pub recent_entries: Vec<String>,
}

impl LauncherStateDb {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let db = RecordDatabase::from_bytes(data).ok()?;
        let mut out = Self::default();

        for record in db.records {
            if record.data.len() < 4 {
                continue;
            }
            let tag = [record.data[0], record.data[1], record.data[2], record.data[3]];
            let body = core::str::from_utf8(&record.data[4..])
                .ok()?
                .trim_end_matches('\0');
            match tag {
                TAG_RESUME => {
                    let value = body.trim();
                    out.resume = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                TAG_BOOKS => {
                    out.book_positions.clear();
                    for line in body.lines() {
                        let Some((name, page_str)) = line.split_once('\t') else {
                            continue;
                        };
                        let name = name.trim();
                        let Ok(page) = page_str.trim().parse::<usize>() else {
                            continue;
                        };
                        if !name.is_empty() {
                            out.book_positions.push((name.to_string(), page));
                        }
                    }
                }
                TAG_RECENTS => {
                    out.recent_entries.clear();
                    for line in body.lines() {
                        let value = line.trim();
                        if !value.is_empty() {
                            out.recent_entries.push(value.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        Some(out)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut db = RecordDatabase::new(state_db_identity());
        db.push_record(0, 1, encode_record(TAG_RESUME, encode_resume(self.resume.as_deref())));
        db.push_record(0, 2, encode_record(TAG_BOOKS, encode_book_positions(&self.book_positions)));
        db.push_record(0, 3, encode_record(TAG_RECENTS, encode_recent_entries(&self.recent_entries)));
        db.to_bytes()
    }
}

pub fn state_db_identity() -> InstalledDbIdentity {
    let mut name = [0u8; 32];
    name[..STATE_DB_NAME.len()].copy_from_slice(STATE_DB_NAME);
    InstalledDbIdentity {
        name,
        db_type: STATE_DB_TYPE,
        creator: STATE_DB_CREATOR,
        version: STATE_DB_VERSION,
    }
}

pub fn state_db_uid(catalog: &[InstalledDbMeta]) -> Option<u64> {
    let identity = state_db_identity();
    catalog
        .iter()
        .find(|meta| {
            meta.identity.name == identity.name
                && meta.identity.db_type == identity.db_type
                && meta.identity.creator == identity.creator
        })
        .map(|meta| meta.uid)
}

pub fn upsert_state_db_meta(catalog: &mut Vec<InstalledDbMeta>, payload_hash: [u8; 32]) -> u64 {
    let identity = state_db_identity();
    if let Some(meta) = catalog.iter_mut().find(|meta| {
        meta.identity.name == identity.name
            && meta.identity.db_type == identity.db_type
            && meta.identity.creator == identity.creator
    }) {
        meta.identity.version = STATE_DB_VERSION;
        meta.kind = DbKind::Record;
        meta.attributes = 0;
        meta.mod_number = meta.mod_number.saturating_add(1).max(1);
        meta.payload_hash = payload_hash;
        return meta.uid;
    }

    let uid = catalog.iter().map(|meta| meta.uid).max().unwrap_or(0) + 1;
    catalog.push(InstalledDbMeta {
        uid,
        card_no: 0,
        identity,
        kind: DbKind::Record,
        attributes: 0,
        mod_number: 1,
        payload_hash,
    });
    uid
}

pub fn payload_hash_32(data: &[u8]) -> [u8; 32] {
    let mut s0: u64 = 0xcbf29ce484222325;
    let mut s1: u64 = 0x9e3779b97f4a7c15;
    let mut s2: u64 = 0x243f6a8885a308d3;
    let mut s3: u64 = 0x13198a2e03707344;
    for (i, b) in data.iter().enumerate() {
        let v = *b as u64 + (i as u64).wrapping_mul(0x100000001b3);
        s0 = (s0 ^ v).wrapping_mul(0x100000001b3);
        s1 = (s1 ^ v.rotate_left(13)).wrapping_mul(0x100000001b3);
        s2 = (s2 ^ v.rotate_left(29)).wrapping_mul(0x100000001b3);
        s3 = (s3 ^ v.rotate_left(47)).wrapping_mul(0x100000001b3);
    }
    let mut out = [0u8; 32];
    out[0..8].copy_from_slice(&s0.to_le_bytes());
    out[8..16].copy_from_slice(&s1.to_le_bytes());
    out[16..24].copy_from_slice(&s2.to_le_bytes());
    out[24..32].copy_from_slice(&s3.to_le_bytes());
    out
}

pub fn state_db_rel_path(uid: u64) -> alloc::string::String {
    alloc::format!("db/v1/db/{uid:016x}.tdb")
}

fn encode_record(tag: [u8; 4], body: String) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&tag);
    out.extend_from_slice(body.as_bytes());
    out
}

fn encode_resume(value: Option<&str>) -> String {
    value.unwrap_or_default().to_string()
}

fn encode_book_positions(entries: &[(String, usize)]) -> String {
    let mut out = String::new();
    for (name, page) in entries {
        out.push_str(name);
        out.push('\t');
        out.push_str(&page.to_string());
        out.push('\n');
    }
    out
}

fn encode_recent_entries(entries: &[String]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str(entry);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{LauncherStateDb, payload_hash_32};

    #[test]
    fn roundtrip_launcher_state_db() {
        let state = LauncherStateDb {
            resume: Some("books/one.trbk".into()),
            book_positions: [
                ("books/one.trbk".into(), 12),
                ("books/two.trbk".into(), 99),
            ]
            .to_vec(),
            recent_entries: ["img/a.tri".into(), "books/one.trbk".into()].to_vec(),
        };

        let encoded = state.to_bytes();
        let decoded = LauncherStateDb::from_bytes(&encoded).expect("decode state db");
        assert_eq!(decoded, state);
        assert_ne!(payload_hash_32(&encoded), [0u8; 32]);
    }
}
