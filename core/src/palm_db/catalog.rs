extern crate alloc;

use alloc::vec::Vec;

use crate::palm_db::types::{InstalledDbIdentity, InstalledDbMeta};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogError {
    Io,
    Corrupt,
    UnsupportedVersion,
}

/// Persistent installed-database catalog abstraction.
///
/// Concrete implementations can back this with `sdcard/palmdb/v1/catalog.bin`
/// plus per-db payload blobs.
pub trait CatalogStore {
    fn load_all(&mut self) -> Result<Vec<InstalledDbMeta>, CatalogError>;
    fn upsert(&mut self, meta: &InstalledDbMeta) -> Result<(), CatalogError>;
    fn remove(&mut self, uid: u64) -> Result<(), CatalogError>;

    fn find_by_identity(
        &mut self,
        identity: &InstalledDbIdentity,
    ) -> Result<Option<InstalledDbMeta>, CatalogError> {
        let mut found = None;
        for db in self.load_all()?.into_iter() {
            if &db.identity == identity {
                found = Some(db);
                break;
            }
        }
        Ok(found)
    }
}
