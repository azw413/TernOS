extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use super::types::InstalledDbIdentity;

const DM_HDR_ATTR_RES_DB: u16 = 0x0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordDbError {
    Corrupt,
    NotRecordDb,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordEntry {
    pub attributes: u8,
    pub unique_id: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordDatabase {
    pub identity: InstalledDbIdentity,
    pub records: Vec<RecordEntry>,
}

impl RecordDatabase {
    pub fn new(identity: InstalledDbIdentity) -> Self {
        Self {
            identity,
            records: Vec::new(),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, RecordDbError> {
        fn be_u16(data: &[u8], off: usize) -> Option<u16> {
            data.get(off..off + 2)
                .map(|b| u16::from_be_bytes([b[0], b[1]]))
        }
        fn be_u32(data: &[u8], off: usize) -> Option<u32> {
            data.get(off..off + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        }

        if data.len() < 78 {
            return Err(RecordDbError::Corrupt);
        }

        let attrs = be_u16(data, 32).ok_or(RecordDbError::Corrupt)?;
        if (attrs & DM_HDR_ATTR_RES_DB) != 0 {
            return Err(RecordDbError::NotRecordDb);
        }

        let version = be_u16(data, 34).ok_or(RecordDbError::Corrupt)?;
        let entry_count = be_u16(data, 76).ok_or(RecordDbError::Corrupt)? as usize;
        let table_len = entry_count.checked_mul(8).ok_or(RecordDbError::Corrupt)?;
        if 78 + table_len > data.len() {
            return Err(RecordDbError::Corrupt);
        }

        let mut name = [0u8; 32];
        name.copy_from_slice(&data[..32]);
        let mut db_type = [0u8; 4];
        db_type.copy_from_slice(&data[60..64]);
        let mut creator = [0u8; 4];
        creator.copy_from_slice(&data[64..68]);

        let mut offsets = Vec::with_capacity(entry_count);
        let mut entries = Vec::with_capacity(entry_count);
        for idx in 0..entry_count {
            let off = 78 + idx * 8;
            offsets.push(be_u32(data, off).ok_or(RecordDbError::Corrupt)? as usize);
            entries.push((
                data[off + 4],
                ((data[off + 5] as u32) << 16) | ((data[off + 6] as u32) << 8) | data[off + 7] as u32,
            ));
        }

        let mut records = Vec::with_capacity(entry_count);
        for idx in 0..entry_count {
            let start = offsets[idx];
            let end = if idx + 1 < entry_count {
                offsets[idx + 1]
            } else {
                data.len()
            };
            if start > end || end > data.len() {
                return Err(RecordDbError::Corrupt);
            }
            records.push(RecordEntry {
                attributes: entries[idx].0,
                unique_id: entries[idx].1,
                data: data[start..end].to_vec(),
            });
        }

        Ok(Self {
            identity: InstalledDbIdentity {
                name,
                db_type,
                creator,
                version,
            },
            records,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let header_len = 78usize;
        let table_len = self.records.len() * 8;
        let mut next_offset = (header_len + table_len) as u32;
        let mut offsets = Vec::with_capacity(self.records.len());
        for record in &self.records {
            offsets.push(next_offset);
            next_offset = next_offset.saturating_add(record.data.len() as u32);
        }

        let mut out = vec![0u8; header_len + table_len];
        out[..self.identity.name.len()].copy_from_slice(&self.identity.name);
        out[34..36].copy_from_slice(&self.identity.version.to_be_bytes());
        out[60..64].copy_from_slice(&self.identity.db_type);
        out[64..68].copy_from_slice(&self.identity.creator);
        out[76..78].copy_from_slice(&(self.records.len() as u16).to_be_bytes());

        for (idx, record) in self.records.iter().enumerate() {
            let base = 78 + idx * 8;
            out[base..base + 4].copy_from_slice(&offsets[idx].to_be_bytes());
            out[base + 4] = record.attributes;
            let unique_id = record.unique_id.min(0x00ff_ffff);
            out[base + 5] = ((unique_id >> 16) & 0xff) as u8;
            out[base + 6] = ((unique_id >> 8) & 0xff) as u8;
            out[base + 7] = (unique_id & 0xff) as u8;
        }

        for record in &self.records {
            out.extend_from_slice(&record.data);
        }
        out
    }

    pub fn num_records(&self) -> usize {
        self.records.len()
    }

    pub fn record(&self, index: usize) -> Option<&RecordEntry> {
        self.records.get(index)
    }

    pub fn record_mut(&mut self, index: usize) -> Option<&mut RecordEntry> {
        self.records.get_mut(index)
    }

    pub fn replace_record(&mut self, index: usize, data: Vec<u8>) -> bool {
        let Some(record) = self.records.get_mut(index) else {
            return false;
        };
        record.data = data;
        true
    }

    pub fn push_record(&mut self, attributes: u8, unique_id: u32, data: Vec<u8>) {
        self.records.push(RecordEntry {
            attributes,
            unique_id: unique_id.min(0x00ff_ffff),
            data,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{RecordDatabase, RecordDbError};
    use crate::ternos::services::db::InstalledDbIdentity;

    #[test]
    fn roundtrip_record_db() {
        let mut name = [0u8; 32];
        name[..8].copy_from_slice(b"TestData");
        let mut db = RecordDatabase::new(InstalledDbIdentity {
            name,
            db_type: *b"DATA",
            creator: *b"TERN",
            version: 7,
        });
        db.push_record(0, 1, b"first".to_vec());
        db.push_record(0x20, 2, b"second".to_vec());

        let encoded = db.to_bytes();
        let decoded = RecordDatabase::from_bytes(&encoded).expect("decode");
        assert_eq!(decoded, db);
        assert_eq!(decoded.num_records(), 2);
        assert_eq!(decoded.record(1).map(|r| r.attributes), Some(0x20));
    }

    #[test]
    fn rejects_resource_db_headers() {
        let mut bytes = [0u8; 78].to_vec();
        bytes[32..34].copy_from_slice(&0x0001u16.to_be_bytes());
        bytes[76..78].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(RecordDatabase::from_bytes(&bytes), Err(RecordDbError::NotRecordDb));
    }
}
