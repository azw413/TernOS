extern crate alloc;

use alloc::string::String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbKind {
    Resource,
    Record,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstalledDbIdentity {
    pub name: [u8; 32],
    pub db_type: [u8; 4],
    pub creator: [u8; 4],
    pub version: u16,
}

impl InstalledDbIdentity {
    pub fn display_name(&self) -> String {
        let len = self
            .name
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(self.name.len());
        String::from_utf8_lossy(&self.name[..len]).into_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledDbMeta {
    pub uid: u64,
    pub card_no: u16,
    pub identity: InstalledDbIdentity,
    pub kind: DbKind,
    pub attributes: u16,
    pub mod_number: u32,
    pub payload_hash: [u8; 32],
}
