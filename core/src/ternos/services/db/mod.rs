extern crate alloc;

pub mod catalog;
pub mod install;
pub mod record;
pub mod runtime;
pub mod types;

pub use catalog::{CatalogError, CatalogStore};
pub use install::{
    InstallDecision, InstallError, InstallInboxEntry, InstallPlanner, InstallSummary,
};
pub use record::{RecordDatabase, RecordDbError, RecordEntry};
pub use types::{DbKind, InstalledDbIdentity, InstalledDbMeta};

/// Canonical database service surface for both native Tern code and Palm adapters.
pub trait DatabaseCatalog {
    fn load_all(&mut self) -> Result<alloc::vec::Vec<InstalledDbMeta>, CatalogError>;
    fn upsert(&mut self, meta: &InstalledDbMeta) -> Result<(), CatalogError>;
    fn remove(&mut self, uid: u64) -> Result<(), CatalogError>;
    fn find_by_identity(
        &mut self,
        identity: &InstalledDbIdentity,
    ) -> Result<Option<InstalledDbMeta>, CatalogError>;
}

impl<T: CatalogStore> DatabaseCatalog for T {
    fn load_all(&mut self) -> Result<alloc::vec::Vec<InstalledDbMeta>, CatalogError> {
        CatalogStore::load_all(self)
    }

    fn upsert(&mut self, meta: &InstalledDbMeta) -> Result<(), CatalogError> {
        CatalogStore::upsert(self, meta)
    }

    fn remove(&mut self, uid: u64) -> Result<(), CatalogError> {
        CatalogStore::remove(self, uid)
    }

    fn find_by_identity(
        &mut self,
        identity: &InstalledDbIdentity,
    ) -> Result<Option<InstalledDbMeta>, CatalogError> {
        CatalogStore::find_by_identity(self, identity)
    }
}
