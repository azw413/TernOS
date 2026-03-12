use std::vec::Vec;

use tern_core::image_viewer::{
    EntryKind, Gray2StreamSource, ImageData, ImageEntry, ImageError, ImageSource,
    PersistenceSource, PowerSource,
};
use tern_core::platform::StorageDevice;

use crate::platform::M5PaperIdfStorage;

pub struct M5PaperImageSource {
    storage: M5PaperIdfStorage,
}

impl M5PaperImageSource {
    pub fn new() -> Self {
        Self {
            storage: M5PaperIdfStorage,
        }
    }

    fn build_path(path: &[String], name: &str) -> String {
        if path.is_empty() {
            if name.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", name)
            }
        } else {
            let mut out = String::new();
            for part in path {
                out.push('/');
                out.push_str(part);
            }
            if !name.is_empty() {
                out.push('/');
                out.push_str(name);
            }
            out
        }
    }

    fn is_supported(name: &str, is_dir: bool) -> bool {
        if is_dir {
            return true;
        }
        let lower = name.to_ascii_lowercase();
        lower.ends_with(".tri")
            || lower.ends_with(".trbk")
            || lower.ends_with(".tbk")
            || lower.ends_with(".epub")
            || lower.ends_with(".epb")
            || lower.ends_with(".prc")
            || lower.ends_with(".tdb")
            || lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
    }
}

impl Default for M5PaperImageSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageSource for M5PaperImageSource {
    fn refresh(&mut self, path: &[String]) -> Result<Vec<ImageEntry>, ImageError> {
        let path_str = Self::build_path(path, "");
        let mut entries = self
            .storage
            .list(&path_str)
            .map_err(|_| ImageError::Io)?
            .into_iter()
            .filter(|entry| Self::is_supported(&entry.name, entry.is_dir))
            .map(|entry| ImageEntry {
                name: entry.name,
                kind: if entry.is_dir {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
            })
            .collect::<Vec<_>>();

        entries.sort_by(|a, b| match (a.kind, b.kind) {
            (EntryKind::Dir, EntryKind::File) => core::cmp::Ordering::Less,
            (EntryKind::File, EntryKind::Dir) => core::cmp::Ordering::Greater,
            _ => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
        });

        Ok(entries)
    }

    fn load(&mut self, _path: &[String], _entry: &ImageEntry) -> Result<ImageData, ImageError> {
        Err(ImageError::Unsupported)
    }

    fn load_prc_bytes(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<Vec<u8>, ImageError> {
        let full_path = Self::build_path(path, &entry.name);
        self.storage.read(&full_path).map_err(|_| ImageError::Io)
    }
}

impl Gray2StreamSource for M5PaperImageSource {}
impl PersistenceSource for M5PaperImageSource {}
impl PowerSource for M5PaperImageSource {}
