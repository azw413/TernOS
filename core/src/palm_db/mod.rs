//! Palm database installation/catalog scaffolding.
//!
//! This module intentionally contains lightweight data models first so we can
//! incrementally wire install-and-run behavior without destabilizing runtime
//! code paths.

pub mod catalog;
pub mod install;
pub mod types;

pub use catalog::{CatalogError, CatalogStore};
pub use install::{
    InstallDecision, InstallError, InstallInboxEntry, InstallPlanner, InstallSummary,
};
pub use types::{DbKind, InstalledDbIdentity, InstalledDbMeta};
