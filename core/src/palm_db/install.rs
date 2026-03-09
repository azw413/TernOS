extern crate alloc;

use alloc::string::String;

use crate::palm_db::types::{InstalledDbIdentity, InstalledDbMeta};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallInboxEntry {
    pub path: String,
    pub size: u64,
    pub identity: InstalledDbIdentity,
    pub payload_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallDecision {
    InstallNew,
    UpgradeExisting { existing_uid: u64 },
    SkipAlreadyInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError {
    Io,
    Parse,
    Storage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct InstallSummary {
    pub scanned: u32,
    pub installed: u32,
    pub upgraded: u32,
    pub skipped: u32,
    pub failed: u32,
}

/// Stateless decision helper for `/install` inbox workflows.
pub struct InstallPlanner;

impl InstallPlanner {
    pub fn decide(entry: &InstallInboxEntry, installed: Option<&InstalledDbMeta>) -> InstallDecision {
        let Some(existing) = installed else {
            return InstallDecision::InstallNew;
        };

        if existing.identity.version < entry.identity.version {
            return InstallDecision::UpgradeExisting {
                existing_uid: existing.uid,
            };
        }

        if existing.identity.version == entry.identity.version
            && existing.payload_hash == entry.payload_hash
        {
            return InstallDecision::SkipAlreadyInstalled;
        }

        // Same version but different payload: treat as upgrade/reinstall.
        InstallDecision::UpgradeExisting {
            existing_uid: existing.uid,
        }
    }
}
