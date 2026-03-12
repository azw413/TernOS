extern crate alloc;

use alloc::{format, string::{String, ToString}, vec, vec::Vec};
use core::ffi::c_char;

use tern_core::image_viewer::{
    BookSource, EntryKind, Gray2StreamSource, ImageData, ImageEntry, ImageError, ImageSource,
    InstalledAppEntry, PersistenceSource, PowerSource,
};

use crate::ffi;

pub struct M5PaperImageSource;

impl M5PaperImageSource {
    pub fn new() -> Self {
        Self
    }

    fn path_bytes(path: &str) -> Vec<u8> {
        let mut bytes = path.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }

    fn build_path(path: &[String], name: &str) -> String {
        if path.is_empty() {
            if name.is_empty() {
                "/".to_string()
            } else {
                let mut s = String::from("/");
                s.push_str(name);
                s
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

    fn entry_name(entry: &ffi::StorageEntry) -> String {
        let nul = entry.name.iter().position(|&b| b == 0).unwrap_or(entry.name.len());
        let bytes = entry.name[..nul]
            .iter()
            .map(|&c| c as u8)
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).into_owned()
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
        let path_string = Self::build_path(path, "");
        let path_bytes = Self::path_bytes(&path_string);
        let begin = ffi::storage_list_begin(path_bytes.as_ptr() as *const c_char);
        if begin != ffi::Status::Ok {
            return Err(ImageError::Io);
        }

        let mut entries = Vec::new();
        loop {
            match ffi::storage_list_next() {
                Ok(Some(raw)) => {
                    let name = Self::entry_name(&raw);
                    if Self::is_supported(&name, raw.is_dir) {
                        entries.push(ImageEntry {
                            name,
                            kind: if raw.is_dir { EntryKind::Dir } else { EntryKind::File },
                        });
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    ffi::storage_list_end();
                    return Err(ImageError::Io);
                }
            }
        }
        ffi::storage_list_end();
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

    fn load_prc_bytes(&mut self, path: &[String], entry: &ImageEntry) -> Result<Vec<u8>, ImageError> {
        let full_path = Self::build_path(path, &entry.name);
        let full_path_bytes = Self::path_bytes(&full_path);
        let size = ffi::storage_file_size(full_path_bytes.as_ptr() as *const c_char)
            .map_err(|_| ImageError::Io)? as usize;
        let mut data = vec![0u8; size];
        let mut offset = 0u32;
        while (offset as usize) < size {
            let read = ffi::storage_read_chunk(
                full_path_bytes.as_ptr() as *const c_char,
                offset,
                &mut data[offset as usize..],
            ).map_err(|_| ImageError::Io)?;
            if read == 0 {
                return Err(ImageError::Io);
            }
            offset += read;
        }
        Ok(data)
    }

    fn list_installed_apps(&mut self) -> Vec<InstalledAppEntry> {
        let path_bytes = Self::path_bytes("/");
        if ffi::storage_list_begin(path_bytes.as_ptr() as *const c_char) != ffi::Status::Ok {
            return Vec::new();
        }

        let mut entries = Vec::new();
        loop {
            match ffi::storage_list_next() {
                Ok(Some(raw)) => {
                    if raw.is_dir {
                        continue;
                    }
                    let name = Self::entry_name(&raw);
                    let lower = name.to_ascii_lowercase();
                    if lower.ends_with(".prc") || lower.ends_with(".tdb") {
                        entries.push(InstalledAppEntry {
                            title: name.clone(),
                            path: format!("/{}", name),
                            icon: None,
                        });
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        ffi::storage_list_end();
        entries.sort_by(|a, b| a.title.to_ascii_lowercase().cmp(&b.title.to_ascii_lowercase()));
        entries
    }
}

impl Gray2StreamSource for M5PaperImageSource {}
impl BookSource for M5PaperImageSource {}
impl PersistenceSource for M5PaperImageSource {}
impl PowerSource for M5PaperImageSource {}
