extern crate alloc;

use alloc::format;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use embedded_io::{Read, Seek, SeekFrom, Write};
use tern_core::fs::{DirEntry, Directory, File, Filesystem, Mode};
use tern_core::palm::{self, PrcCodeScan, PrcDbKind, PrcInfo, PrcResourceEntry, PrcSectionStat};
use tern_core::ternos::services::db::{
    DbKind, InstallDecision, InstallInboxEntry, InstallPlanner, InstallSummary,
    InstalledDbIdentity, InstalledDbMeta,
};
use tern_core::ternos::services::state::{self, LauncherStateDb};
use crate::sdspi_fs::UsbFsOps;
use tern_core::image_viewer::{
    BookSource, EntryKind, Gray2StreamSource, ImageData, ImageEntry, ImageError, ImageSource,
    InstalledAppEntry,
    PersistenceSource, PowerSource,
};

mod embedded_prc_fonts {
    include!(concat!(env!("OUT_DIR"), "/prc_embedded_fonts.rs"));
}

pub struct SdImageSource<F>
where
    F: Filesystem + 'static,
{
    fs: F,
    trbk: Option<TrbkStream>,
    short_names: Vec<(String, String)>,
    usb_stream: Option<Box<UsbWriteStreamState<F::File<'static>>>>,
}

pub struct UsbDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

pub trait UsbStorage {
    fn usb_list(&mut self, path: &str) -> Result<Vec<UsbDirEntry>, ImageError>;
    fn usb_read(&mut self, path: &str, offset: u64, length: u32) -> Result<Vec<u8>, ImageError>;
    fn usb_write(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<u32, ImageError>;
    fn usb_write_stream(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
        final_chunk: bool,
    ) -> Result<u32, ImageError> {
        let _ = final_chunk;
        self.usb_write(path, offset, data)
    }
    fn usb_delete(&mut self, path: &str) -> Result<(), ImageError>;
    fn usb_rmdir(&mut self, path: &str) -> Result<(), ImageError>;
    fn usb_rename(&mut self, from: &str, to: &str) -> Result<(), ImageError>;
    fn usb_mkdir(&mut self, path: &str) -> Result<(), ImageError>;
}

const USB_MAX_READ_CHUNK: usize = 8 * 1024;

#[track_caller]
fn oom_error(bytes: usize, context: &'static str) -> ImageError {
    let loc = core::panic::Location::caller();
    ImageError::Message(format!(
        "OOM: reserve {} bytes in {} at {}:{}",
        bytes,
        context,
        loc.file(),
        loc.line()
    ))
}

struct UsbWriteStreamState<FileT> {
    path: String,
    file: FileT,
    next_offset: u64,
}

struct TrbkStream {
    path: Vec<String>,
    name: String,
    short_name: Option<String>,
    page_offsets: Vec<u32>,
    page_data_offset: u32,
    glyph_table_offset: u32,
    info: Rc<tern_core::trbk::TrbkBookInfo>,
}

impl<F> SdImageSource<F>
where
    F: Filesystem + 'static,
{
    fn join_usb_path(dir: &str, name: &str) -> String {
        if dir.is_empty() || dir == "/" {
            return format!("/{}", name);
        }
        format!("{}/{}", dir.trim_end_matches('/'), name)
    }

    fn build_path(path: &[String], name: &str) -> String {
        if path.is_empty() {
            return name.to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        for part in path.iter().map(|p| p.as_str()) {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }
        match name {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(name),
        }
        parts.join("/")
    }
    fn entry_path_string(&self, path: &[String], entry: &ImageEntry) -> String {
        if path.is_empty() {
            entry.name.clone()
        } else {
            let mut parts = path.to_vec();
            parts.push(entry.name.clone());
            parts.join("/")
        }
    }

    pub fn new(fs: F) -> Self {
        Self {
            fs,
            trbk: None,
            short_names: Vec::new(),
            usb_stream: None,
        }
    }

    fn lookup_short_name(&self, name: &str) -> Option<String> {
        for (long, short) in &self.short_names {
            if long.eq_ignore_ascii_case(name) {
                return Some(short.clone());
            }
        }
        None
    }

    fn is_supported(name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        name.ends_with(".tri")
            || name.ends_with(".trbk")
            || name.ends_with(".tbk")
            || name.ends_with(".epub")
            || name.ends_with(".epb")
            || name.ends_with(".prc")
            || name.ends_with(".tdb")
    }

    fn thumbnails_dirname() -> &'static str {
        "TRCACHE"
    }

    fn thumbnails_dirname_legacy() -> &'static str {
        ".trusty_cache"
    }

    fn thumbnail_name(key: &str) -> String {
        let hash = thumb_hash_hex(key);
        let short = &hash[..6.min(hash.len())];
        let mut name = String::from("TH");
        name.push_str(short);
        name.push_str(".TRI");
        name
    }

    fn thumbnail_title_name(key: &str) -> String {
        let hash = thumb_hash_hex(key);
        let short = &hash[..6.min(hash.len())];
        let mut name = String::from("TT");
        name.push_str(short);
        name.push_str(".TXT");
        name
    }

    fn install_dirname() -> &'static str {
        "/install"
    }

    fn db_root_dirname() -> &'static str {
        "/db/v1"
    }

    fn db_catalog_filename() -> &'static str {
        "/db/v1/catalog.txt"
    }

    fn db_data_dirname() -> &'static str {
        "/db/v1/db"
    }

    fn state_db_path(uid: u64) -> String {
        state::state_db_rel_path(uid)
    }

    fn load_state_db(&self) -> LauncherStateDb {
        let catalog = self.load_palm_catalog();
        let Some(uid) = state::state_db_uid(&catalog) else {
            return LauncherStateDb::default();
        };
        let path = Self::state_db_path(uid);
        let mut file = match self.fs.open_file(&path, Mode::Read) {
            Ok(file) => file,
            Err(_) => return LauncherStateDb::default(),
        };
        let mut data = Vec::new();
        let mut chunk = [0u8; 256];
        loop {
            let read = match file.read(&mut chunk) {
                Ok(read) => read,
                Err(_) => return LauncherStateDb::default(),
            };
            if read == 0 {
                break;
            }
            if data.try_reserve(read).is_err() {
                return LauncherStateDb::default();
            }
            data.extend_from_slice(&chunk[..read]);
        }
        LauncherStateDb::from_bytes(&data).unwrap_or_default()
    }

    fn save_state_db(&self, state_db: &LauncherStateDb) -> Result<(), ImageError> {
        let _ = self.fs.create_dir_all("db");
        let _ = self.fs.create_dir_all("db/v1");
        let _ = self.fs.create_dir_all(Self::db_data_dirname());

        let data = state_db.to_bytes();
        let mut catalog = self.load_palm_catalog();
        let uid = state::upsert_state_db_meta(&mut catalog, state::payload_hash_32(&data));
        let path = Self::state_db_path(uid);
        let mut file = self.fs.open_file(&path, Mode::Write).map_err(|_| ImageError::Io)?;
        write_all(&mut file, &data)?;
        let _ = file.flush();
        self.save_palm_catalog(&catalog)?;
        Ok(())
    }

}

impl<F> UsbStorage for SdImageSource<F>
where
    F: Filesystem + UsbFsOps + 'static,
    for<'a> F::File<'a>: 'static,
{
    fn usb_list(&mut self, path: &str) -> Result<Vec<UsbDirEntry>, ImageError> {
        let listed = {
            let dir = self.fs.open_directory(path).map_err(|_| ImageError::Io)?;
            dir.list().map_err(|_| ImageError::Io)?
        };
        let mut out = Vec::new();
        for entry in listed {
            out.push(UsbDirEntry {
                name: entry.name().to_string(),
                is_dir: entry.is_directory(),
                size: entry.size() as u64,
            });
        }
        Ok(out)
    }

    fn usb_read(&mut self, path: &str, offset: u64, length: u32) -> Result<Vec<u8>, ImageError> {
        let mut file = self.fs.open_file(path, Mode::Read).map_err(|_| ImageError::Io)?;
        let _ = file.seek(SeekFrom::Start(offset)).map_err(|_| ImageError::Io)?;
        // Host-side callers may request large chunks (e.g. 32 KiB). Cap the
        // temporary allocation to keep device heap usage predictable.
        let req_len = (length as usize).min(USB_MAX_READ_CHUNK);
        let mut buf = vec![0u8; req_len];
        let read = file.read(&mut buf).map_err(|_| ImageError::Io)?;
        buf.truncate(read);
        Ok(buf)
    }

    fn usb_write(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<u32, ImageError> {
        let mut file = if offset == 0 {
            match self.fs.open_file(path, Mode::Write) {
                Ok(file) => file,
                Err(err) => {
                    return Err(ImageError::Message(alloc::format!("open write failed: {:?}", err)));
                }
            }
        } else {
            self.fs
                .open_file(path, Mode::ReadWrite)
                .map_err(|err| ImageError::Message(alloc::format!("open rw failed: {:?}", err)))?
        };
        let _ = file
            .seek(SeekFrom::Start(offset))
            .map_err(|err| ImageError::Message(alloc::format!("seek failed: {:?}", err)))?;
        let written = file
            .write(data)
            .map_err(|err| ImageError::Message(alloc::format!("write failed: {:?}", err)))?;
        let _ = file
            .flush()
            .map_err(|err| ImageError::Message(alloc::format!("flush failed: {:?}", err)))?;
        Ok(written as u32)
    }

    fn usb_write_stream(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
        final_chunk: bool,
    ) -> Result<u32, ImageError> {
        if offset == 0 {
            if let Some(mut stream) = self.usb_stream.take() {
                let _ = stream.file.flush();
            }
            let file = self
                .fs
                .open_file(path, Mode::Write)
                .map_err(|err| ImageError::Message(alloc::format!("open write failed: {:?}", err)))?;
            // SAFETY: UsbStorage is only used on device with owned file handles (FatFs).
            // We widen the lifetime to store the handle across calls.
            let file = unsafe {
                core::mem::transmute::<F::File<'_>, F::File<'static>>(file)
            };
            self.usb_stream = Some(Box::new(UsbWriteStreamState {
                path: path.to_string(),
                file,
                next_offset: 0,
            }));
        }

        let Some(stream) = self.usb_stream.as_mut() else {
            return Err(ImageError::Message("usb stream not initialized".into()));
        };
        if !stream.path.eq_ignore_ascii_case(path) {
            return Err(ImageError::Message("usb stream path mismatch".into()));
        }
        if stream.next_offset != offset {
            let _ = stream
                .file
                .seek(SeekFrom::Start(offset))
                .map_err(|err| ImageError::Message(alloc::format!("seek failed: {:?}", err)))?;
            stream.next_offset = offset;
        }
        let written = stream
            .file
            .write(data)
            .map_err(|err| ImageError::Message(alloc::format!("write failed: {:?}", err)))?;
        stream.next_offset = stream.next_offset.saturating_add(written as u64);
        if final_chunk {
            let _ = stream
                .file
                .flush()
                .map_err(|err| ImageError::Message(alloc::format!("flush failed: {:?}", err)))?;
            self.usb_stream = None;
        }
        Ok(written as u32)
    }

    fn usb_delete(&mut self, path: &str) -> Result<(), ImageError> {
        self.fs.delete_file(path).map_err(|_| ImageError::Io)?;
        self.cleanup_deleted_path_with_usb(path);
        Ok(())
    }

    fn usb_rmdir(&mut self, path: &str) -> Result<(), ImageError> {
        self.usb_delete_dir_recursive(path)?;
        self.fs.delete_file(path).map_err(|_| ImageError::Io)?;
        Ok(())
    }

    fn usb_rename(&mut self, from: &str, to: &str) -> Result<(), ImageError> {
        self.fs.rename_file(from, to).map_err(|_| ImageError::Io)
    }

    fn usb_mkdir(&mut self, path: &str) -> Result<(), ImageError> {
        self.fs.create_dir_all(path).map_err(|_| ImageError::Io)
    }
}

impl<F> SdImageSource<F>
where
    F: Filesystem,
{
    fn normalize_deleted_path(path: &str) -> String {
        path.trim_start_matches('/').to_string()
    }

    fn path_matches(entry: &str, target: &str) -> bool {
        entry.eq_ignore_ascii_case(target)
            || entry.trim_start_matches('/').eq_ignore_ascii_case(target)
    }

    fn cleanup_deleted_path_with_usb(&mut self, path: &str)
    where
        F: UsbFsOps,
    {
        let target = Self::normalize_deleted_path(path);
        if target.is_empty() {
            return;
        }
        if let Some(resume) = self.load_resume() {
            if Self::path_matches(&resume, &target) {
                self.save_resume(None);
            }
        }

        let mut recents = self.load_recent_entries();
        let old_len = recents.len();
        recents.retain(|entry| !Self::path_matches(entry, &target));
        if recents.len() != old_len {
            self.save_recent_entries(&recents);
        }

        let mut positions = self.load_book_positions();
        let old_len = positions.len();
        positions.retain(|(entry, _)| !Self::path_matches(entry, &target));
        if positions.len() != old_len {
            self.save_book_positions(&positions);
        }

        let thumb = Self::thumbnail_name(&target);
        let title = Self::thumbnail_title_name(&target);
        let cache_primary = Self::thumbnails_dirname();
        let cache_legacy = Self::thumbnails_dirname_legacy();
        let _ = self
            .fs
            .delete_file(&format!("{}/{}", cache_primary, thumb));
        let _ = self
            .fs
            .delete_file(&format!("{}/{}", cache_primary, title));
        let _ = self
            .fs
            .delete_file(&format!("{}/{}", cache_legacy, thumb));
        let _ = self
            .fs
            .delete_file(&format!("{}/{}", cache_legacy, title));
    }

    fn usb_delete_dir_recursive(&mut self, path: &str) -> Result<(), ImageError>
    where
        F: UsbFsOps,
    {
        let listed = {
            let dir = self.fs.open_directory(path).map_err(|_| ImageError::Io)?;
            dir.list().map_err(|_| ImageError::Io)?
        };
        let entries: Vec<(String, bool)> = listed
            .into_iter()
            .map(|entry| (entry.name().to_string(), entry.is_directory()))
            .collect();
        for (name, is_dir) in entries {
            if name == "." || name == ".." {
                continue;
            }
            let full_path = Self::join_usb_path(path, &name);
            if is_dir {
                self.usb_delete_dir_recursive(&full_path)?;
                let _ = self.fs.delete_file(&full_path);
            } else {
                let _ = self.fs.delete_file(&full_path);
                self.cleanup_deleted_path_with_usb(&full_path);
            }
        }
        Ok(())
    }

    fn load_palm_catalog(&self) -> Vec<InstalledDbMeta> {
        let mut file = match self.fs.open_file(Self::db_catalog_filename(), Mode::Read) {
            Ok(file) => file,
            Err(_) => return Vec::new(),
        };
        let mut buf = Vec::new();
        let mut chunk = [0u8; 256];
        loop {
            let read = match file.read(&mut chunk) {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            };
            if read == 0 {
                break;
            }
            if buf.try_reserve(read).is_err() {
                return Vec::new();
            }
            buf.extend_from_slice(&chunk[..read]);
        }
        let Ok(text) = core::str::from_utf8(&buf) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for line in text.lines() {
            let mut cols = line.split('\t');
            let Some(uid_s) = cols.next() else { continue };
            let Some(card_s) = cols.next() else { continue };
            let Some(kind_s) = cols.next() else { continue };
            let Some(attrs_s) = cols.next() else { continue };
            let Some(mod_s) = cols.next() else { continue };
            let Some(ver_s) = cols.next() else { continue };
            let Some(name_s) = cols.next() else { continue };
            let Some(type_s) = cols.next() else { continue };
            let Some(creator_s) = cols.next() else { continue };
            let Some(hash_s) = cols.next() else { continue };
            let (Ok(uid), Ok(card_no), Ok(attributes), Ok(mod_number), Ok(version)) = (
                uid_s.parse::<u64>(),
                card_s.parse::<u16>(),
                attrs_s.parse::<u16>(),
                mod_s.parse::<u32>(),
                ver_s.parse::<u16>(),
            ) else {
                continue;
            };
            let Some(name) = hex_decode_fixed::<32>(name_s) else {
                continue;
            };
            let Some(db_type) = hex_decode_fixed::<4>(type_s) else {
                continue;
            };
            let Some(creator) = hex_decode_fixed::<4>(creator_s) else {
                continue;
            };
            let Some(payload_hash) = hex_decode_fixed::<32>(hash_s) else {
                continue;
            };
            let kind = if kind_s == "resource" {
                DbKind::Resource
            } else {
                DbKind::Record
            };
            out.push(InstalledDbMeta {
                uid,
                card_no,
                identity: InstalledDbIdentity {
                    name,
                    db_type,
                    creator,
                    version,
                },
                kind,
                attributes,
                mod_number,
                payload_hash,
            });
        }
        out
    }

    fn save_palm_catalog(&self, catalog: &[InstalledDbMeta]) -> Result<(), ImageError> {
        // FatFs mkdir returns an error when the directory already exists; tolerate that.
        let _ = self.fs.create_dir_all("/db");
        let _ = self.fs.create_dir_all(Self::db_root_dirname());
        let mut file = self
            .fs
            .open_file(Self::db_catalog_filename(), Mode::Write)
            .map_err(|_| ImageError::Io)?;
        for meta in catalog {
            let kind = match meta.kind {
                DbKind::Resource => "resource",
                DbKind::Record => "record",
            };
            let line = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                meta.uid,
                meta.card_no,
                kind,
                meta.attributes,
                meta.mod_number,
                meta.identity.version,
                hex_encode(&meta.identity.name),
                hex_encode(&meta.identity.db_type),
                hex_encode(&meta.identity.creator),
                hex_encode(&meta.payload_hash),
            );
            write_all(&mut file, line.as_bytes())?;
        }
        let _ = file.flush();
        Ok(())
    }
}

fn read_exact<R: Read + ?Sized>(reader: &mut R, mut buf: &mut [u8]) -> Result<(), ImageError> {
    while !buf.is_empty() {
        let read = reader.read(buf).map_err(|_| ImageError::Io)?;
        if read == 0 {
            return Err(ImageError::Decode);
        }
        let tmp = buf;
        buf = &mut tmp[read..];
    }
    Ok(())
}

fn write_all<W: Write>(writer: &mut W, mut data: &[u8]) -> Result<(), ImageError> {
    while !data.is_empty() {
        let written = writer.write(data).map_err(|_| ImageError::Io)?;
        if written == 0 {
            return Err(ImageError::Io);
        }
        data = &data[written..];
    }
    Ok(())
}

fn thumb_hash_hex(key: &str) -> String {
    let mut hash: u32 = 0x811c9dc5;
    for b in key.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    let mut out = String::new();
    for nibble in (0..8).rev() {
        let value = (hash >> (nibble * 4)) & 0xF;
        let ch = match value {
            0..=9 => (b'0' + value as u8) as char,
            _ => (b'a' + (value as u8 - 10)) as char,
        };
        out.push(ch);
    }
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::new();
    if out.try_reserve(bytes.len() * 2).is_err() {
        return out;
    }
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode_fixed<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    let bytes = s.as_bytes();
    for i in 0..N {
        let hi = (bytes[i * 2] as char).to_digit(16)? as u8;
        let lo = (bytes[i * 2 + 1] as char).to_digit(16)? as u8;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn payload_hash_32(data: &[u8]) -> [u8; 32] {
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

fn payload_hash_32_stream<R: Read + Seek>(reader: &mut R) -> Result<[u8; 32], ImageError> {
    let _ = reader.seek(SeekFrom::Start(0)).map_err(|_| ImageError::Io)?;
    let mut s0: u64 = 0xcbf29ce484222325;
    let mut s1: u64 = 0x9e3779b97f4a7c15;
    let mut s2: u64 = 0x243f6a8885a308d3;
    let mut s3: u64 = 0x13198a2e03707344;
    let mut idx: u64 = 0;
    let mut chunk = [0u8; 512];
    loop {
        let read = reader.read(&mut chunk).map_err(|_| ImageError::Io)?;
        if read == 0 {
            break;
        }
        for b in &chunk[..read] {
            let v = *b as u64 + idx.wrapping_mul(0x100000001b3);
            s0 = (s0 ^ v).wrapping_mul(0x100000001b3);
            s1 = (s1 ^ v.rotate_left(13)).wrapping_mul(0x100000001b3);
            s2 = (s2 ^ v.rotate_left(29)).wrapping_mul(0x100000001b3);
            s3 = (s3 ^ v.rotate_left(47)).wrapping_mul(0x100000001b3);
            idx = idx.saturating_add(1);
        }
    }
    let mut out = [0u8; 32];
    out[0..8].copy_from_slice(&s0.to_le_bytes());
    out[8..16].copy_from_slice(&s1.to_le_bytes());
    out[16..24].copy_from_slice(&s2.to_le_bytes());
    out[24..32].copy_from_slice(&s3.to_le_bytes());
    Ok(out)
}

fn identity_from_prc(info: &PrcInfo) -> InstalledDbIdentity {
    let mut name = [0u8; 32];
    let raw = info.db_name.as_bytes();
    let copy_n = raw.len().min(32);
    name[..copy_n].copy_from_slice(&raw[..copy_n]);
    let mut db_type = [0u8; 4];
    let mut creator = [0u8; 4];
    let ty = info.type_code.as_bytes();
    let cr = info.creator_code.as_bytes();
    for i in 0..4 {
        db_type[i] = *ty.get(i).unwrap_or(&b'?');
        creator[i] = *cr.get(i).unwrap_or(&b'?');
    }
    InstalledDbIdentity {
        name,
        db_type,
        creator,
        version: info.version,
    }
}

fn identity_from_prc_header(data: &[u8]) -> Option<(InstalledDbIdentity, DbKind, u16)> {
    if data.len() < 78 {
        return None;
    }
    let mut name = [0u8; 32];
    let mut end = 32usize;
    for (idx, b) in data[0..32].iter().enumerate() {
        if *b == 0 {
            end = idx;
            break;
        }
    }
    name[..end].copy_from_slice(&data[0..end]);
    let attributes = u16::from_be_bytes([data[32], data[33]]);
    let version = u16::from_be_bytes([data[34], data[35]]);
    let db_type = [data[60], data[61], data[62], data[63]];
    let creator = [data[64], data[65], data[66], data[67]];
    let kind = if (attributes & 0x0001) != 0 {
        DbKind::Resource
    } else {
        DbKind::Record
    };
    Some((
        InstalledDbIdentity {
            name,
            db_type,
            creator,
            version,
        },
        kind,
        attributes,
    ))
}

fn extract_app_icon(raw: &[u8]) -> Option<ImageData> {
    let bitmaps = tern_core::palm::bitmap::parse_prc_bitmaps(raw);
    let best = bitmaps
        .iter()
        .filter(|b| b.width > 0 && b.height > 0)
        .filter(|b| b.width <= 64 && b.height <= 64)
        .min_by_key(|b| (b.width.abs_diff(32), b.height.abs_diff(32), b.resource_id))
        .or_else(|| bitmaps.first())?;
    Some(ImageData::Mono1 {
        width: best.width as u32,
        height: best.height as u32,
        bits: best.bits.clone(),
    })
}

#[derive(Clone)]
struct ParsedBitmap {
    resource_id: u16,
    width: u16,
    height: u16,
    row_bytes: u16,
    bits: Vec<u8>,
}

fn read_u16_be_opt(data: &[u8], off: usize) -> Option<u16> {
    let b0 = *data.get(off)?;
    let b1 = *data.get(off + 1)?;
    Some(u16::from_be_bytes([b0, b1]))
}

fn parse_bitmap_blob_light(resource_id: u16, data: &[u8]) -> Option<ParsedBitmap> {
    if data.len() < 16 {
        return None;
    }
    let width = read_u16_be_opt(data, 0)?;
    let height = read_u16_be_opt(data, 2)?;
    let row_bytes_raw = read_u16_be_opt(data, 4)?;
    let flags = read_u16_be_opt(data, 6).unwrap_or(0);
    let row_bytes = row_bytes_raw & 0x3FFF;
    let pixel_size = *data.get(8)?;
    let version = *data.get(9)?;
    if width == 0 || height == 0 || row_bytes == 0 || pixel_size != 1 {
        return None;
    }
    let bits_len = row_bytes as usize * height as usize;
    let compressed = (flags & 0x8000) != 0;
    let compression = if compressed {
        if version <= 1 {
            0
        } else {
            *data.get(13).unwrap_or(&0)
        }
    } else {
        0xFF
    };
    let header_size = if version >= 3 { 24usize } else { 16usize };
    let bits = if !compressed {
        data.get(header_size..header_size.saturating_add(bits_len))
            .or_else(|| data.get(16..16usize.saturating_add(bits_len)))?
            .to_vec()
    } else {
        let mut src = data.get(header_size..)?;
        src = match version {
            0 | 1 | 2 => src.get(2..)?,
            3 => src.get(4..)?,
            _ => src,
        };
        let mut out = vec![0u8; bits_len];
        match compression {
            0 => {
                let mut si = 0usize;
                for row in 0..height as usize {
                    let row_base = row * row_bytes as usize;
                    let mut j = 0usize;
                    while j < row_bytes as usize {
                        let diff = *src.get(si)?;
                        si += 1;
                        let chunk = core::cmp::min(8usize, row_bytes as usize - j);
                        for k in 0..chunk {
                            let idx = row_base + j + k;
                            if row == 0 || (diff & (1 << (7 - k))) != 0 {
                                out[idx] = *src.get(si)?;
                                si += 1;
                            } else {
                                out[idx] = out[(row - 1) * row_bytes as usize + j + k];
                            }
                        }
                        j += 8;
                    }
                }
            }
            1 => {
                let mut si = 0usize;
                let mut di = 0usize;
                while di < out.len() {
                    let len = *src.get(si)? as usize;
                    let b = *src.get(si + 1)?;
                    si += 2;
                    let end = core::cmp::min(di + len, out.len());
                    out[di..end].fill(b);
                    di = end;
                }
            }
            2 => {
                let mut si = 0usize;
                let mut di = 0usize;
                while di < out.len() {
                    let count = *src.get(si)? as i8;
                    si += 1;
                    if (-127..=-1).contains(&count) {
                        let len = (-count as i16 + 1) as usize;
                        let b = *src.get(si)?;
                        si += 1;
                        let end = core::cmp::min(di + len, out.len());
                        out[di..end].fill(b);
                        di = end;
                    } else if (0..=127).contains(&count) {
                        let len = count as usize + 1;
                        let end = core::cmp::min(di + len, out.len());
                        let src_end = si + (end - di);
                        out[di..end].copy_from_slice(src.get(si..src_end)?);
                        di = end;
                        si = src_end;
                    }
                }
            }
            _ => return None,
        }
        out
    };
    Some(ParsedBitmap {
        resource_id,
        width,
        height,
        row_bytes,
        bits,
    })
}

fn extract_app_icon_streaming<R: File>(file: &mut R) -> Option<ImageData> {
    let file_size = file.size() as u32;
    if file_size < 78 {
        return None;
    }
    let mut header = [0u8; 78];
    let _ = file.seek(SeekFrom::Start(0)).ok()?;
    read_exact(file, &mut header).ok()?;
    let attrs = u16::from_be_bytes([header[32], header[33]]);
    if (attrs & 0x0001) == 0 {
        return None;
    }
    let entry_count = u16::from_be_bytes([header[76], header[77]]) as usize;
    if entry_count == 0 || entry_count > 4096 {
        return None;
    }
    let table_len = entry_count.saturating_mul(10);
    let mut table = vec![0u8; table_len];
    let _ = file.seek(SeekFrom::Start(78)).ok()?;
    read_exact(file, &mut table).ok()?;

    let mut best: Option<ParsedBitmap> = None;
    for i in 0..entry_count {
        let off = i * 10;
        let kind = &table[off..off + 4];
        if kind != b"Tbmp" && kind != b"tAIB" {
            continue;
        }
        let id = u16::from_be_bytes([table[off + 4], table[off + 5]]);
        let start = u32::from_be_bytes([table[off + 6], table[off + 7], table[off + 8], table[off + 9]]);
        if start >= file_size {
            continue;
        }
        let next = if i + 1 < entry_count {
            u32::from_be_bytes([
                table[(i + 1) * 10 + 6],
                table[(i + 1) * 10 + 7],
                table[(i + 1) * 10 + 8],
                table[(i + 1) * 10 + 9],
            ])
        } else {
            file_size
        };
        let end = next.min(file_size);
        if end <= start {
            continue;
        }
        let size = (end - start) as usize;
        if size < 16 || size > 8192 {
            continue;
        }
        let mut blob = vec![0u8; size];
        let _ = file.seek(SeekFrom::Start(start as u64)).ok()?;
        read_exact(file, &mut blob).ok()?;
        let Some(parsed) = parse_bitmap_blob_light(id, &blob) else {
            continue;
        };
        if parsed.width > 64 || parsed.height > 64 {
            continue;
        }
        let take = match &best {
            None => true,
            Some(cur) => {
                let cur_key = (
                    cur.width.abs_diff(32),
                    cur.height.abs_diff(32),
                    cur.resource_id,
                );
                let new_key = (
                    parsed.width.abs_diff(32),
                    parsed.height.abs_diff(32),
                    parsed.resource_id,
                );
                new_key < cur_key
            }
        };
        if take {
            best = Some(parsed);
        }
    }

    let bmp = best?;
    Some(ImageData::Mono1 {
        width: bmp.width as u32,
        height: bmp.height as u32,
        bits: bmp.bits,
    })
}

fn parse_tdb_uid_from_name(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(".tdb")?;
    if stem.len() != 16 {
        return None;
    }
    u64::from_str_radix(stem, 16).ok()
}

fn same_db_key(a: &InstalledDbIdentity, b: &InstalledDbIdentity) -> bool {
    a.name == b.name && a.db_type == b.db_type && a.creator == b.creator
}

fn serialize_thumbnail(image: &ImageData) -> Option<Vec<u8>> {
    let (width, height, bits, version, format) = match image {
        ImageData::Mono1 {
            width,
            height,
            bits,
        } => (*width, *height, bits.as_slice(), 1u8, 1u8),
        ImageData::Gray2 { width, height, data } => (*width, *height, data.as_slice(), 2u8, 2u8),
        _ => return None,
    };
    let expected = ((width as usize * height as usize) + 7) / 8;
    let expected_len = if version == 2 { expected * 3 } else { expected };
    if bits.len() != expected_len {
        return None;
    }
    let mut data = Vec::new();
    if data.try_reserve(16 + bits.len()).is_err() {
        return None;
    }
    data.extend_from_slice(b"TRIM");
    data.push(version);
    data.push(format);
    data.extend_from_slice(&(width as u16).to_le_bytes());
    data.extend_from_slice(&(height as u16).to_le_bytes());
    data.extend_from_slice(&[0u8; 6]);
    data.extend_from_slice(bits);
    Some(data)
}

fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, ImageError> {
    if offset + 2 > data.len() {
        return Err(ImageError::Decode);
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn read_u16_be(data: &[u8], offset: usize) -> Result<u16, ImageError> {
    if offset + 2 > data.len() {
        return Err(ImageError::Decode);
    }
    Ok(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn read_u32_be(data: &[u8], offset: usize) -> Result<u32, ImageError> {
    if offset + 4 > data.len() {
        return Err(ImageError::Decode);
    }
    Ok(u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn parse_c_string_ascii(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let mut out = String::new();
    for b in &bytes[..end] {
        if (0x20..=0x7e).contains(b) {
            out.push(*b as char);
        }
    }
    out
}

fn parse_fourcc_ascii(bytes: &[u8]) -> String {
    let mut out = String::new();
    for b in bytes {
        if (0x20..=0x7e).contains(b) {
            out.push(*b as char);
        } else {
            out.push('.');
        }
    }
    out
}

fn add_prc_section(sections: &mut Vec<PrcSectionStat>, name: &str, size: u32) {
    for section in sections.iter_mut() {
        if section.name == name {
            section.count = section.count.saturating_add(1);
            section.bytes = section.bytes.saturating_add(size);
            return;
        }
    }
    sections.push(PrcSectionStat {
        name: name.to_string(),
        count: 1,
        bytes: size,
    });
}

fn add_unique_u16(values: &mut Vec<u16>, value: u16) {
    if !values.iter().any(|v| *v == value) {
        values.push(value);
    }
}

fn read_i16_le(data: &[u8], offset: usize) -> Result<i16, ImageError> {
    if offset + 2 > data.len() {
        return Err(ImageError::Decode);
    }
    Ok(i16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, ImageError> {
    if offset + 4 > data.len() {
        return Err(ImageError::Decode);
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn read_trimg_from_file<R: Read>(reader: &mut R, len: usize) -> Result<ImageData, ImageError> {
    if len < 16 {
        return Err(ImageError::Decode);
    }
    let mut header = [0u8; 16];
    read_exact(reader, &mut header)?;
    if &header[0..4] != b"TRIM" {
        return Err(ImageError::Unsupported);
    }
    let width = u16::from_le_bytes([header[6], header[7]]) as u32;
    let height = u16::from_le_bytes([header[8], header[9]]) as u32;
    let plane = ((width as usize * height as usize) + 7) / 8;

    match (header[4], header[5]) {
        (1, 1) => {
            if 16 + plane != len {
                return Err(ImageError::Decode);
            }
            let mut bits = Vec::new();
            if bits.try_reserve(plane).is_err() {
                return Err(oom_error(plane, "load_image_data: mono buffer"));
            }
            let mut buffer = [0u8; 512];
            while bits.len() < plane {
                let read = reader.read(&mut buffer).map_err(|_| ImageError::Io)?;
                if read == 0 {
                    break;
                }
                let remaining = plane - bits.len();
                let take = read.min(remaining);
                if bits.try_reserve(take).is_err() {
                    return Err(oom_error(take, "load_image_data: mono stream growth"));
                }
                bits.extend_from_slice(&buffer[..take]);
            }
            if bits.len() != plane {
                return Err(ImageError::Decode);
            }
            Ok(ImageData::Mono1 { width, height, bits })
        }
        (2, 2) => {
            if 16 + plane * 3 != len {
                return Err(ImageError::Decode);
            }
            let mut data = Vec::new();
            if data.try_reserve(plane * 3).is_err() {
                return Err(oom_error(plane * 3, "load_image_data: gray2 expansion"));
            }
            data.resize(plane * 3, 0u8);
            read_exact(reader, &mut data)?;
            Ok(ImageData::Gray2 { width, height, data })
        }
        _ => Err(ImageError::Unsupported),
    }
}

fn read_string(data: &[u8], cursor: &mut usize) -> Result<String, ImageError> {
    let len = read_u32_le(data, *cursor)? as usize;
    *cursor += 4;
    if *cursor + len > data.len() {
        return Err(ImageError::Decode);
    }
    let value = core::str::from_utf8(&data[*cursor..*cursor + len])
        .map_err(|_| ImageError::Decode)?
        .to_string();
    *cursor += len;
    Ok(value)
}

impl<F> ImageSource for SdImageSource<F>
where
    F: Filesystem + UsbFsOps,
{
    fn refresh(&mut self, path: &[String]) -> Result<Vec<ImageEntry>, ImageError> {
        let path_str = if path.is_empty() {
            "/".to_string()
        } else {
            path.join("/")
        };
        log::info!("SD refresh dir: '{}'", path_str);
        let read_dir = match self.fs.open_directory(&path_str) {
            Ok(dir) => dir,
            Err(_) => {
                let upper = path_str.to_ascii_uppercase();
                if upper != path_str {
                    match self.fs.open_directory(&upper) {
                        Ok(dir) => dir,
                        Err(_) => {
                            log::warn!("Failed to open directory: '{}'", upper);
                            log::warn!("Failed to open directory: '{}'", path_str);
                            return Err(ImageError::Io);
                        }
                    }
                } else {
                    log::warn!("Failed to open directory: '{}'", path_str);
                    return Err(ImageError::Io);
                }
            }
        };
        let mut entries = Vec::new();
        let listed = read_dir.list().map_err(|err| {
            log::warn!("Failed to list directory '{}': {:?}", path_str, err);
            ImageError::Io
        })?;
        self.short_names.clear();
        for entry in listed {
            let name = entry.name().to_string();
            let short = entry.short_name().to_string();
            let upper = name.to_ascii_uppercase();
            let short_upper = short.to_ascii_uppercase();
            let short_is_hidden = short.starts_with('.');
            if !name.is_empty() {
                self.short_names.push((name.clone(), short));
            }
            if name.is_empty()
                || name.starts_with('.')
                || short_is_hidden
                || (path.is_empty() && upper == "DB")
                || upper == Self::thumbnails_dirname()
                || upper == Self::thumbnails_dirname_legacy().to_ascii_uppercase()
                || short_upper == Self::thumbnails_dirname()
            {
                continue;
            }
            if entry.is_directory() {
                entries.push(ImageEntry {
                    name,
                    kind: EntryKind::Dir,
                });
            } else if Self::is_supported(&name) {
                entries.push(ImageEntry {
                    name,
                    kind: EntryKind::File,
                });
            }
        }

        entries.sort_by(|a, b| match (a.kind, b.kind) {
            (EntryKind::Dir, EntryKind::File) => core::cmp::Ordering::Less,
            (EntryKind::File, EntryKind::Dir) => core::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        Ok(entries)
    }

    fn load(&mut self, path: &[String], entry: &ImageEntry) -> Result<ImageData, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Message("Select a file, not a folder.".into()));
        }
        let lower = entry.name.to_ascii_lowercase();
        if lower.ends_with(".epub") || lower.ends_with(".epb") {
            return Err(ImageError::Message("EPUB files must be converted to .trbk.".into()));
        }
        if lower.ends_with(".trbk") || lower.ends_with(".tbk") {
            return Err(ImageError::Unsupported);
        }

        let file_path = Self::build_path(path, &entry.name);
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;

        const MAX_IMAGE_BYTES: usize = 200_000;
        let file_len = file.size();
        if file_len < 16 || file_len > MAX_IMAGE_BYTES {
            return Err(ImageError::Message(
                "Image size not supported on device.".into(),
            ));
        }

        let mut header = [0u8; 16];
        let read = file.read(&mut header).map_err(|_| ImageError::Io)?;
        if read != header.len() || &header[0..4] != b"TRIM" {
            return Err(ImageError::Unsupported);
        }
        let width = u16::from_le_bytes([header[6], header[7]]) as u32;
        let height = u16::from_le_bytes([header[8], header[9]]) as u32;
        let plane = ((width as usize * height as usize) + 7) / 8;
        match (header[4], header[5]) {
            (1, 1) => {
                if 16 + plane != file_len {
                    return Err(ImageError::Decode);
                }
                let mut bits = Vec::new();
                if bits.try_reserve(plane).is_err() {
                    return Err(oom_error(plane, "load_gray2_stream: mono buffer"));
                }
                let mut buffer = [0u8; 512];
                while bits.len() < plane {
                    let read = file.read(&mut buffer).map_err(|_| ImageError::Io)?;
                    if read == 0 {
                        break;
                    }
                    let remaining = plane - bits.len();
                    let take = read.min(remaining);
                    if bits.try_reserve(take).is_err() {
                        return Err(oom_error(take, "load_gray2_stream: mono stream growth"));
                    }
                    bits.extend_from_slice(&buffer[..take]);
                }
                if bits.len() != plane {
                    return Err(ImageError::Decode);
                }
                Ok(ImageData::Mono1 { width, height, bits })
            }
            (2, 2) => {
                if 16 + plane * 3 != file_len {
                    return Err(ImageError::Decode);
                }
                let key = self.entry_path_string(path, entry);
                Ok(ImageData::Gray2Stream { width, height, key })
            }
            _ => Err(ImageError::Unsupported),
        }
    }

    fn load_prc_info(&mut self, path: &[String], entry: &ImageEntry) -> Result<PrcInfo, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Message("Select a file, not a folder.".into()));
        }
        let lower = entry.name.to_ascii_lowercase();
        if !lower.ends_with(".prc") && !lower.ends_with(".tdb") {
            return Err(ImageError::Unsupported);
        }

        let file_path = Self::build_path(path, &entry.name);
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;
        let file_size = file.size() as u32;
        if file_size < 78 {
            return Err(ImageError::Decode);
        }

        let mut header = [0u8; 78];
        read_exact(&mut file, &mut header)?;
        let db_name = parse_c_string_ascii(&header[0..32]);
        let attrs = read_u16_be(&header, 32)?;
        let version = read_u16_be(&header, 34)?;
        let type_code = parse_fourcc_ascii(&header[60..64]);
        let creator_code = parse_fourcc_ascii(&header[64..68]);
        let entry_count = read_u16_be(&header, 76)?;
        let is_resource = (attrs & 0x0001) != 0;
        let entry_size = if is_resource { 10usize } else { 8usize };
        let table_len = entry_size.saturating_mul(entry_count as usize);
        if (78 + table_len) as u32 > file_size {
            return Err(ImageError::Decode);
        }

        let mut table = Vec::new();
        if table.try_reserve(table_len).is_err() {
            return Err(oom_error(table_len, "load_prc_info: header table"));
        }
        table.resize(table_len, 0);
        read_exact(&mut file, &mut table)?;

        let mut sections: Vec<PrcSectionStat> = Vec::new();
        let mut resources: Vec<PrcResourceEntry> = Vec::new();
        let mut code_scan: Vec<PrcCodeScan> = Vec::new();
        let mut code_bytes = 0u32;
        let mut other_bytes = 0u32;
        let mut a_trap_total = 0u32;
        let mut trap15_total = 0u32;
        let mut unique_a_traps: Vec<u16> = Vec::new();

        let mut scan_code_resource = |data_off: u32, size: u32, res_id: u16| -> Result<PrcCodeScan, ImageError> {
            let _ = file
                .seek(SeekFrom::Start(data_off as u64))
                .map_err(|_| ImageError::Io)?;
            let mut remaining = size as usize;
            let mut buf = [0u8; 256];
            let mut carry: Option<u8> = None;
            let mut a_count = 0u32;
            let mut t15_count = 0u32;
            let mut traps = Vec::new();
            while remaining > 0 {
                let want = remaining.min(buf.len());
                let read = file.read(&mut buf[..want]).map_err(|_| ImageError::Io)?;
                if read == 0 {
                    break;
                }
                let chunk = &buf[..read];
                let mut idx = 0usize;
                if let Some(prev) = carry.take() {
                    let word = u16::from_be_bytes([prev, chunk[0]]);
                    if (word & 0xF000) == 0xA000 {
                        a_count = a_count.saturating_add(1);
                        add_unique_u16(&mut traps, word);
                    } else if (word & 0xFFF0) == 0x4E40 && (word & 0x000F) == 0x000F {
                        t15_count = t15_count.saturating_add(1);
                    }
                    idx = 1;
                }
                while idx + 1 < chunk.len() {
                    let word = u16::from_be_bytes([chunk[idx], chunk[idx + 1]]);
                    if (word & 0xF000) == 0xA000 {
                        a_count = a_count.saturating_add(1);
                        add_unique_u16(&mut traps, word);
                    } else if (word & 0xFFF0) == 0x4E40 && (word & 0x000F) == 0x000F {
                        t15_count = t15_count.saturating_add(1);
                    }
                    idx += 2;
                }
                carry = if idx < chunk.len() {
                    Some(chunk[idx])
                } else {
                    None
                };
                remaining -= read;
            }
            Ok(PrcCodeScan {
                resource_id: res_id,
                size,
                a_trap_count: a_count,
                trap15_count: t15_count,
                unique_a_traps: traps,
            })
        };

        if is_resource {
            let mut offsets = Vec::new();
            let mut kinds = Vec::new();
            let mut ids = Vec::new();
            if offsets.try_reserve(entry_count as usize).is_err()
                || kinds.try_reserve(entry_count as usize).is_err()
                || ids.try_reserve(entry_count as usize).is_err()
            {
                return Err(oom_error(entry_count as usize, "load_prc_info: resource index"));
            }
            for i in 0..entry_count as usize {
                let off = i * 10;
                let kind = parse_fourcc_ascii(&table[off..off + 4]);
                let id = read_u16_be(&table, off + 4)?;
                let data_off = read_u32_be(&table, off + 6)?.min(file_size);
                kinds.push(kind);
                ids.push(id);
                offsets.push(data_off);
            }
            for i in 0..offsets.len() {
                let cur = offsets[i];
                let next = if i + 1 < offsets.len() {
                    offsets[i + 1]
                } else {
                    file_size
                };
                let size = if next > cur { next - cur } else { 0 };
                resources.push(PrcResourceEntry {
                    kind: kinds[i].clone(),
                    id: ids[i],
                    offset: cur,
                    size,
                });
                add_prc_section(&mut sections, &kinds[i], size);
                if kinds[i].eq_ignore_ascii_case("code") {
                    code_bytes = code_bytes.saturating_add(size);
                    let scan = scan_code_resource(cur, size, ids[i])?;
                    a_trap_total = a_trap_total.saturating_add(scan.a_trap_count);
                    trap15_total = trap15_total.saturating_add(scan.trap15_count);
                    for trap in &scan.unique_a_traps {
                        add_unique_u16(&mut unique_a_traps, *trap);
                    }
                    code_scan.push(scan);
                } else {
                    other_bytes = other_bytes.saturating_add(size);
                }
            }
        } else {
            let mut offsets = Vec::new();
            if offsets.try_reserve(entry_count as usize).is_err() {
                return Err(oom_error(entry_count as usize, "load_prc_info: record index"));
            }
            for i in 0..entry_count as usize {
                let off = i * 8;
                offsets.push(read_u32_be(&table, off)?.min(file_size));
            }
            for i in 0..offsets.len() {
                let cur = offsets[i];
                let next = if i + 1 < offsets.len() {
                    offsets[i + 1]
                } else {
                    file_size
                };
                let size = if next > cur { next - cur } else { 0 };
                resources.push(PrcResourceEntry {
                    kind: "record".into(),
                    id: i as u16,
                    offset: cur,
                    size,
                });
                add_prc_section(&mut sections, "record", size);
                other_bytes = other_bytes.saturating_add(size);
            }
        }

        Ok(PrcInfo {
            db_name,
            kind: if is_resource {
                PrcDbKind::Resource
            } else {
                PrcDbKind::Record
            },
            file_size,
            type_code,
            creator_code,
            attributes: attrs,
            version,
            entry_count,
            code_bytes,
            other_bytes,
            sections,
            resources,
            code_scan,
            a_trap_total,
            trap15_total,
            unique_a_traps,
            trap_hits: Vec::new(),
        })
    }

    fn load_prc_code_resource(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
        resource_id: u16,
    ) -> Result<Vec<u8>, ImageError> {
        let info = self.load_prc_info(path, entry)?;
        let res = info
            .resources
            .iter()
            .find(|r| r.kind.eq_ignore_ascii_case("code") && r.id == resource_id)
            .ok_or(ImageError::Decode)?;
        let file_path = Self::build_path(path, &entry.name);
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;
        let _ = file
            .seek(SeekFrom::Start(res.offset as u64))
            .map_err(|_| ImageError::Io)?;
        let mut out = Vec::new();
        let sz = res.size as usize;
        if out.try_reserve(sz).is_err() {
            return Err(oom_error(sz, "load_prc_code_resource"));
        }
        out.resize(sz, 0);
        read_exact(&mut file, &mut out)?;
        Ok(out)
    }

    fn load_prc_bytes(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<Vec<u8>, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Message("Select a file, not a folder.".into()));
        }
        let lower = entry.name.to_ascii_lowercase();
        if !lower.ends_with(".prc") && !lower.ends_with(".tdb") {
            return Err(ImageError::Unsupported);
        }
        let file_path = Self::build_path(path, &entry.name);
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;
        let size = file.size() as usize;
        if size < 78 {
            return Err(ImageError::Decode);
        }
        let mut out = Vec::new();
        if out.try_reserve(size).is_err() {
            return Err(oom_error(size, "load_prc_bytes"));
        }
        out.resize(size, 0);
        read_exact(&mut file, &mut out)?;
        Ok(out)
    }

    fn load_prc_app_resources(
        &mut self,
        _path: &[String],
        entry: &ImageEntry,
        info: &PrcInfo,
    ) -> Vec<tern_core::palm::runtime::ResourceBlob> {
        let mut out = Vec::new();
        let Some(current_uid) = parse_tdb_uid_from_name(&entry.name) else {
            return out;
        };
        let creator = info.creator_code.as_bytes();
        let Ok(creator_4) = <[u8; 4]>::try_from(creator.get(..4).unwrap_or(b"????")) else {
            return out;
        };
        let catalog = self.load_palm_catalog();
        for meta in catalog {
            if meta.uid == current_uid {
                continue;
            }
            if meta.identity.db_type != *b"ovly" || meta.identity.creator != creator_4 {
                continue;
            }
            let file_path = format!("/db/v1/db/{:016x}.tdb", meta.uid);
            let Ok(mut file) = self.fs.open_file(&file_path, Mode::Read) else {
                continue;
            };
            let size = file.size() as usize;
            if size < 78 {
                continue;
            }
            let mut raw = Vec::new();
            if raw.try_reserve(size).is_err() {
                continue;
            }
            raw.resize(size, 0);
            if read_exact(&mut file, &mut raw).is_err() {
                continue;
            }
            let blobs = tern_core::palm::parse_prc_resource_blobs(&raw);
            out.extend(blobs);
        }
        out
    }

    fn load_prc_system_resources(&mut self) -> Vec<tern_core::palm::runtime::ResourceBlob> {
        let mut out = Vec::new();
        let mut listed = None;
        for dir in ["fonts", "/fonts"] {
            if let Ok(d) = self.fs.open_directory(dir) {
                if let Ok(items) = d.list() {
                    listed = Some(items);
                    break;
                }
            }
        }
        let Some(entries) = listed else {
            return out;
        };

        for entry in entries {
            if entry.is_directory() {
                continue;
            }
            let name = entry.name().to_string();
            if !tern_core::palm::font::is_prc_font_resource_blob_name(&name) {
                continue;
            }
            let Some(id) = tern_core::palm::font::parse_font_resource_id_from_name(&name) else {
                continue;
            };
            let mut data = Vec::new();
            let mut opened = None;
            for base in ["fonts", "/fonts"] {
                let file_path = format!("{}/{}", base.trim_end_matches('/'), name);
                if let Ok(file) = self.fs.open_file(&file_path, Mode::Read) {
                    opened = Some(file);
                    break;
                }
            }
            let Some(mut file) = opened else {
                continue;
            };
            let size = file.size();
            if size < 26 {
                continue;
            }
            if data.try_reserve(size).is_err() {
                continue;
            }
            data.resize(size, 0);
            if read_exact(&mut file, &mut data).is_err() {
                continue;
            }
            out.push(tern_core::palm::runtime::ResourceBlob {
                kind: u32::from_be_bytes(*b"NFNT"),
                id,
                data,
            });
        }
        if !out.is_empty() {
            log::info!("Loaded {} system font resources from sdcard/fonts", out.len());
        }
        out
    }

    fn load_prc_system_fonts(&mut self) -> Vec<tern_core::palm::runtime::PalmFont> {
        let out = embedded_prc_fonts::load_embedded_prc_fonts_72();
        let embedded_loaded = out.len();
        if embedded_loaded > 0 {
            log::info!(
                "Loaded {} embedded text system fonts from firmware image",
                embedded_loaded
            );
        } else {
            log::warn!("No embedded PRC text fonts found in firmware image");
        }
        out
    }

    fn load_home_system_fonts(&mut self) -> Vec<tern_core::palm::runtime::PalmFont> {
        let mut out = embedded_prc_fonts::load_embedded_prc_fonts_144();
        if out.is_empty() {
            out = embedded_prc_fonts::load_embedded_prc_fonts_72();
        }
        if !out.is_empty() {
            log::info!(
                "Loaded {} embedded home fonts from firmware image",
                out.len()
            );
        }
        out
    }

    fn scan_palm_install_inbox(&mut self) -> Option<InstallSummary> {
        // FatFs backend's `create_dir_all` is single-level; ensure parents explicitly.
        let _ = self.fs.create_dir_all("db");
        let _ = self.fs.create_dir_all("db/v1");
        let _ = self.fs.create_dir_all(Self::db_data_dirname());

        let listed = {
            let Ok(dir) = self.fs.open_directory(Self::install_dirname()) else {
                log::warn!("Palm install inbox missing: {}", Self::install_dirname());
                return Some(InstallSummary::default());
            };
            let Ok(items) = dir.list() else {
                log::warn!("Palm install inbox unreadable: {}", Self::install_dirname());
                return Some(InstallSummary::default());
            };
            items
        };

        if self.fs.open_directory(Self::db_data_dirname()).is_err() {
            log::warn!(
                "Palm DB directory unavailable after mkdir: {}",
                Self::db_data_dirname()
            );
            return Some(InstallSummary {
                scanned: 0,
                installed: 0,
                upgraded: 0,
                skipped: 0,
                failed: 1,
            });
        }

        let mut catalog = self.load_palm_catalog();
        let mut summary = InstallSummary::default();
        let mut delete_after_commit: Vec<String> = Vec::new();
        let mut seen_inbox_names: Vec<String> = Vec::new();
        let mut seen_signatures: Vec<([u8; 4], [u8; 4], [u8; 32])> = Vec::new();

        for entry in listed {
            if entry.is_directory() {
                continue;
            }
            let name = entry.name().to_string();
            if name.starts_with('.') || name.starts_with("._") {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if !lower.ends_with(".prc") && !lower.ends_with(".pdb") {
                continue;
            }
            if seen_inbox_names.iter().any(|n| n == &lower) {
                continue;
            }
            seen_inbox_names.push(lower.clone());
            let full_path = format!("{}/{}", Self::install_dirname(), name);
            let mut file = match self.fs.open_file(&full_path, Mode::Read) {
                Ok(f) => f,
                Err(_) => {
                    summary.failed = summary.failed.saturating_add(1);
                    continue;
                }
            };
            let size = file.size();
            if size < 78 {
                summary.failed = summary.failed.saturating_add(1);
                continue;
            }
            let mut header = [0u8; 78];
            if read_exact(&mut file, &mut header).is_err() {
                summary.failed = summary.failed.saturating_add(1);
                continue;
            }
            let Some((identity, kind, attributes)) = identity_from_prc_header(&header) else {
                summary.failed = summary.failed.saturating_add(1);
                continue;
            };
            let payload_hash = match payload_hash_32_stream(&mut file) {
                Ok(v) => v,
                Err(_) => {
                    summary.failed = summary.failed.saturating_add(1);
                    continue;
                }
            };
            let sig = (identity.db_type, identity.creator, payload_hash);
            if seen_signatures.iter().any(|s| *s == sig) {
                continue;
            }
            seen_signatures.push(sig);
            summary.scanned = summary.scanned.saturating_add(1);
            let existing_idx = catalog
                .iter()
                .position(|m| same_db_key(&m.identity, &identity));
            let decision = InstallPlanner::decide(
                &InstallInboxEntry {
                    path: full_path.clone(),
                    size: size as u64,
                    identity: identity.clone(),
                    payload_hash,
                },
                existing_idx.and_then(|idx| catalog.get(idx)),
            );
            let type_code = core::str::from_utf8(&identity.db_type).unwrap_or("????");
            let creator = core::str::from_utf8(&identity.creator).unwrap_or("????");

            match decision {
                InstallDecision::SkipAlreadyInstalled => {
                    log::info!(
                        "Palm install skip path={} name='{}' type='{}' creator='{}'",
                        full_path,
                        identity.display_name(),
                        type_code,
                        creator
                    );
                    summary.skipped = summary.skipped.saturating_add(1);
                    delete_after_commit.push(full_path.clone());
                }
                InstallDecision::InstallNew => {
                    log::info!(
                        "Palm install new path={} name='{}' type='{}' creator='{}'",
                        full_path,
                        identity.display_name(),
                        type_code,
                        creator
                    );
                    let uid = catalog.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                    let out_path = format!("{}/{:016x}.tdb", Self::db_data_dirname(), uid);
                    let mut out = match self.fs.open_file(&out_path, Mode::Write) {
                        Ok(f) => f,
                        Err(_) => {
                            summary.failed = summary.failed.saturating_add(1);
                            continue;
                        }
                    };
                    let mut src = match self.fs.open_file(&full_path, Mode::Read) {
                        Ok(f) => f,
                        Err(_) => {
                            summary.failed = summary.failed.saturating_add(1);
                            continue;
                        }
                    };
                    let mut chunk = [0u8; 512];
                    let mut copy_failed = false;
                    loop {
                        let read = match src.read(&mut chunk) {
                            Ok(v) => v,
                            Err(_) => {
                                copy_failed = true;
                                break;
                            }
                        };
                        if read == 0 {
                            break;
                        }
                        if write_all(&mut out, &chunk[..read]).is_err() {
                            copy_failed = true;
                            break;
                        }
                    }
                    if copy_failed {
                        summary.failed = summary.failed.saturating_add(1);
                        continue;
                    }
                    let _ = out.flush();
                    catalog.push(InstalledDbMeta {
                        uid,
                        card_no: 0,
                        identity,
                        kind,
                        attributes,
                        mod_number: 1,
                        payload_hash,
                    });
                    summary.installed = summary.installed.saturating_add(1);
                    delete_after_commit.push(full_path.clone());
                }
                InstallDecision::UpgradeExisting { existing_uid } => {
                    log::info!(
                        "Palm install upgrade path={} uid={} name='{}' type='{}' creator='{}'",
                        full_path,
                        existing_uid,
                        identity.display_name(),
                        type_code,
                        creator
                    );
                    let out_path = format!("{}/{:016x}.tdb", Self::db_data_dirname(), existing_uid);
                    let mut out = match self.fs.open_file(&out_path, Mode::Write) {
                        Ok(f) => f,
                        Err(_) => {
                            summary.failed = summary.failed.saturating_add(1);
                            continue;
                        }
                    };
                    let mut src = match self.fs.open_file(&full_path, Mode::Read) {
                        Ok(f) => f,
                        Err(_) => {
                            summary.failed = summary.failed.saturating_add(1);
                            continue;
                        }
                    };
                    let mut chunk = [0u8; 512];
                    let mut copy_failed = false;
                    loop {
                        let read = match src.read(&mut chunk) {
                            Ok(v) => v,
                            Err(_) => {
                                copy_failed = true;
                                break;
                            }
                        };
                        if read == 0 {
                            break;
                        }
                        if write_all(&mut out, &chunk[..read]).is_err() {
                            copy_failed = true;
                            break;
                        }
                    }
                    if copy_failed {
                        summary.failed = summary.failed.saturating_add(1);
                        continue;
                    }
                    let _ = out.flush();
                    if let Some(meta) = catalog.iter_mut().find(|m| m.uid == existing_uid) {
                        meta.identity = identity;
                        meta.attributes = attributes;
                        meta.kind = kind;
                        meta.mod_number = meta.mod_number.saturating_add(1);
                        meta.payload_hash = payload_hash;
                    }
                    summary.upgraded = summary.upgraded.saturating_add(1);
                    delete_after_commit.push(full_path.clone());
                }
            }
        }

        if summary.scanned > 0 {
            if self.save_palm_catalog(&catalog).is_err() {
                log::warn!("Failed to save Palm DB catalog");
                summary.failed = summary.failed.saturating_add(1);
            } else {
                for path in delete_after_commit {
                    let _ = self.fs.delete_file(&path);
                }
            }
        }
        Some(summary)
    }

    fn list_installed_apps(&mut self) -> Vec<InstalledAppEntry> {
        let mut out = Vec::new();
        for meta in self.load_palm_catalog() {
            // Show launchable app-like databases in Home > Apps.
            // `panl` is used by Palm control-panel apps like Date & Time.
            if meta.identity.db_type != *b"appl" && meta.identity.db_type != *b"panl" {
                continue;
            }
            let path = format!("/db/v1/db/{:016x}.tdb", meta.uid);
            let icon = (|| {
                let mut file = self.fs.open_file(&path, Mode::Read).ok()?;
                extract_app_icon_streaming(&mut file)
            })();
            out.push(InstalledAppEntry {
                title: meta.identity.display_name(),
                path,
                icon,
            });
        }
        out.sort_by(|a, b| a.title.cmp(&b.title));
        out
    }

}

impl<F> PersistenceSource for SdImageSource<F>
where
    F: Filesystem,
{
    fn save_resume(&mut self, name: Option<&str>) {
        let mut state_db = self.load_state_db();
        state_db.resume = name.map(|value| value.to_string());
        let _ = self.save_state_db(&state_db);
    }

    fn load_resume(&mut self) -> Option<String> {
        let state_db = self.load_state_db();
        state_db.resume
    }

    fn save_book_positions(&mut self, entries: &[(String, usize)]) {
        let mut state_db = self.load_state_db();
        state_db.book_positions = entries.to_vec();
        let _ = self.save_state_db(&state_db);
    }

    fn load_book_positions(&mut self) -> Vec<(String, usize)> {
        let state_db = self.load_state_db();
        state_db.book_positions
    }

    fn save_recent_entries(&mut self, entries: &[String]) {
        let mut state_db = self.load_state_db();
        state_db.recent_entries = entries.to_vec();
        let _ = self.save_state_db(&state_db);
    }

    fn load_recent_entries(&mut self) -> Vec<String> {
        let state_db = self.load_state_db();
        state_db.recent_entries
    }

    fn save_book_catalog(&mut self, signature: &str, entries: &[(String, String)]) {
        let mut state_db = self.load_state_db();
        state_db.book_catalog_signature = Some(signature.to_string());
        state_db.book_catalog_entries = entries.to_vec();
        let _ = self.save_state_db(&state_db);
    }

    fn load_book_catalog(&mut self) -> Option<(String, Vec<(String, String)>)> {
        let state_db = self.load_state_db();
        Some((
            state_db.book_catalog_signature?,
            state_db.book_catalog_entries,
        ))
    }

    fn save_image_catalog(&mut self, signature: &str, entries: &[(String, String)]) {
        let mut state_db = self.load_state_db();
        state_db.image_catalog_signature = Some(signature.to_string());
        state_db.image_catalog_entries = entries.to_vec();
        let _ = self.save_state_db(&state_db);
    }

    fn load_image_catalog(&mut self) -> Option<(String, Vec<(String, String)>)> {
        let state_db = self.load_state_db();
        Some((
            state_db.image_catalog_signature?,
            state_db.image_catalog_entries,
        ))
    }

    fn load_thumbnail(&mut self, key: &str) -> Option<ImageData> {
        let name = Self::thumbnail_name(key);
        let primary = format!("{}/{}", Self::thumbnails_dirname(), name);
        let legacy = format!("{}/{}", Self::thumbnails_dirname_legacy(), name);
        let mut file = self
            .fs
            .open_file(&primary, Mode::Read)
            .or_else(|_| self.fs.open_file(&legacy, Mode::Read))
            .ok()?;
        let mut header = [0u8; 16];
        let read = file.read(&mut header).ok()?;
        if read != header.len() || &header[0..4] != b"TRIM" {
            return None;
        }
        let width = u16::from_le_bytes([header[6], header[7]]) as u32;
        let height = u16::from_le_bytes([header[8], header[9]]) as u32;
        let plane = ((width as usize * height as usize) + 7) / 8;
        let expected = if header[4] == 2 && header[5] == 2 {
            plane * 3
        } else if header[4] == 1 && header[5] == 1 {
            plane
        } else {
            return None;
        };
        let mut bits = Vec::new();
        if bits.try_reserve(expected).is_err() {
            return None;
        }
        let mut buffer = [0u8; 256];
        while bits.len() < expected {
            let read = file.read(&mut buffer).ok()?;
            if read == 0 {
                break;
            }
            let remaining = expected - bits.len();
            let take = read.min(remaining);
            if bits.try_reserve(take).is_err() {
                return None;
            }
            bits.extend_from_slice(&buffer[..take]);
        }
        if bits.len() != expected {
            return None;
        }
        if expected == plane {
            Some(ImageData::Mono1 {
                width,
                height,
                bits,
            })
        } else {
            Some(ImageData::Gray2 {
                width,
                height,
                data: bits,
            })
        }
    }

    fn save_thumbnail(&mut self, key: &str, image: &ImageData) {
        let Some(data) = serialize_thumbnail(image) else {
            return;
        };
        let cache_name = Self::thumbnails_dirname();
        if self.fs.create_dir_all(cache_name).is_err() {
            return;
        }
        let name = Self::thumbnail_name(key);
        let path = format!("{}/{}", cache_name, name);
        let mut file = match self.fs.open_file(&path, Mode::Write) {
            Ok(file) => file,
            Err(_) => return,
        };
        if write_all(&mut file, &data).is_err() {
            return;
        }
        let _ = file.flush();
    }

    fn load_thumbnail_title(&mut self, key: &str) -> Option<String> {
        let name = Self::thumbnail_title_name(key);
        let primary = format!("{}/{}", Self::thumbnails_dirname(), name);
        let legacy = format!("{}/{}", Self::thumbnails_dirname_legacy(), name);
        let mut file = self
            .fs
            .open_file(&primary, Mode::Read)
            .or_else(|_| self.fs.open_file(&legacy, Mode::Read))
            .ok()?;
        let mut buf = [0u8; 128];
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            return None;
        }
        let text = core::str::from_utf8(&buf[..read]).ok()?.trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }

    fn save_thumbnail_title(&mut self, key: &str, title: &str) {
        let cache_name = Self::thumbnails_dirname();
        if self.fs.create_dir_all(cache_name).is_err() {
            return;
        }
        let name = Self::thumbnail_title_name(key);
        let path = format!("{}/{}", cache_name, name);
        let mut file = match self.fs.open_file(&path, Mode::Write) {
            Ok(file) => file,
            Err(_) => return,
        };
        if write_all(&mut file, title.as_bytes()).is_err() {
            return;
        }
        let _ = file.flush();
    }

}

impl<F> Gray2StreamSource for SdImageSource<F>
where
    F: Filesystem,
{
    fn load_gray2_stream(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
        rotation: tern_core::framebuffer::Rotation,
        base: &mut [u8],
        lsb: &mut [u8],
        msb: &mut [u8],
    ) -> Result<(), ImageError> {
        self.load_gray2_stream_region(key, width, height, rotation, base, lsb, msb, 0, 0)
    }

    fn load_gray2_stream_region(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
        rotation: tern_core::framebuffer::Rotation,
        base: &mut [u8],
        lsb: &mut [u8],
        msb: &mut [u8],
        dst_x: i32,
        dst_y: i32,
    ) -> Result<(), ImageError> {
        use tern_core::framebuffer::{HEIGHT as FB_HEIGHT, WIDTH as FB_WIDTH};

        fn map_point(
            rotation: tern_core::framebuffer::Rotation,
            x: usize,
            y: usize,
        ) -> Option<(usize, usize)> {
            let (x, y) = match rotation {
                tern_core::framebuffer::Rotation::Rotate0 => (x, y),
                tern_core::framebuffer::Rotation::Rotate90 => (y, FB_HEIGHT - 1 - x),
                tern_core::framebuffer::Rotation::Rotate180 => {
                    (FB_WIDTH - 1 - x, FB_HEIGHT - 1 - y)
                }
                tern_core::framebuffer::Rotation::Rotate270 => (FB_WIDTH - 1 - y, x),
            };
            if x >= FB_WIDTH || y >= FB_HEIGHT {
                None
            } else {
                Some((x, y))
            }
        }

        fn set_bit(buf: &mut [u8], x: usize, y: usize) {
            let idx = y * FB_WIDTH + x;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            buf[byte] |= 1 << bit;
        }

        fn clear_bit(buf: &mut [u8], x: usize, y: usize) {
            let idx = y * FB_WIDTH + x;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            buf[byte] &= !(1 << bit);
        }

        let mut load_from_reader = |reader: &mut dyn Read<Error = <F::File<'_> as embedded_io::ErrorType>::Error>|
            -> Result<(), ImageError> {
            let mut header = [0u8; 16];
            read_exact(reader, &mut header)?;
            if &header[0..4] != b"TRIM" || header[4] != 2 || header[5] != 2 {
                return Err(ImageError::Unsupported);
            }
            let w = u16::from_le_bytes([header[6], header[7]]) as u32;
            let h = u16::from_le_bytes([header[8], header[9]]) as u32;
            if w != width || h != height {
                return Err(ImageError::Decode);
            }

            let total_pixels = (width as usize) * (height as usize);
            let plane_len = (total_pixels + 7) / 8;
            let mut tmp = [0u8; 256];
            let mut pixel_index: usize = 0;
            let mut read_plane = |target: &mut [u8], is_base: bool| -> Result<(), ImageError> {
                pixel_index = 0;
                let mut remaining = plane_len;
                while remaining > 0 {
                    let want = remaining.min(tmp.len());
                    read_exact(reader, &mut tmp[..want])?;
                    for byte in &tmp[..want] {
                        for bit in 0..8 {
                            if pixel_index >= total_pixels {
                                break;
                            }
                            let sx = pixel_index % (width as usize);
                            let sy = pixel_index / (width as usize);
                            let bit_set = (byte >> (7 - bit)) & 0x01 == 1;
                            let dx = dst_x + sx as i32;
                            let dy = dst_y + sy as i32;
                            if dx >= 0 && dy >= 0 {
                                if let Some((fx, fy)) =
                                    map_point(rotation, dx as usize, dy as usize)
                                {
                                    if is_base {
                                        if bit_set {
                                            set_bit(target, fx, fy);
                                        } else {
                                            clear_bit(target, fx, fy);
                                        }
                                    } else if bit_set {
                                        set_bit(target, fx, fy);
                                    }
                                }
                            }
                            pixel_index += 1;
                        }
                    }
                    remaining -= want;
                }
                Ok(())
            };

            read_plane(base, true)?;
            read_plane(lsb, false)?;
            read_plane(msb, false)?;
            Ok(())
        };

        if let Some(offset_str) = key.strip_prefix("trbk:") {
            let offset: u32 = offset_str.parse().map_err(|_| ImageError::Decode)?;
            let Some(state) = &self.trbk else {
                return Err(ImageError::Decode);
            };
            let file_path = if state.path.is_empty() {
                state
                    .short_name
                    .as_deref()
                    .unwrap_or(state.name.as_str())
                    .to_string()
            } else {
                format!("{}/{}", state.path.join("/"), state.name)
            };
            let mut file = self
                .fs
                .open_file(&file_path, Mode::Read)
                .map_err(|_| ImageError::Io)?;
            file.seek(SeekFrom::Start(offset as u64))
                .map_err(|_| ImageError::Io)?;
            return load_from_reader(&mut file);
        }

        let mut file = self
            .fs
            .open_file(key, Mode::Read)
            .map_err(|_| ImageError::Io)?;
        load_from_reader(&mut file)
    }

    fn load_gray2_stream_thumbnail(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
        thumb_w: u32,
        thumb_h: u32,
    ) -> Option<ImageData> {
        fn set_bit(buf: &mut [u8], x: usize, y: usize, width: usize, value: bool) {
            let idx = y * width + x;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            if value {
                buf[byte] |= 1 << bit;
            } else {
                buf[byte] &= !(1 << bit);
            }
        }

        fn set_bit_on(buf: &mut [u8], x: usize, y: usize, width: usize) {
            let idx = y * width + x;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            buf[byte] |= 1 << bit;
        }

        fn alloc_u16(len: usize) -> Option<Vec<u16>> {
            let mut out = Vec::new();
            if out.try_reserve_exact(len).is_err() {
                return None;
            }
            out.resize(len, 0);
            Some(out)
        }

        fn alloc_u8(len: usize, fill: u8) -> Option<Vec<u8>> {
            let mut out = Vec::new();
            if out.try_reserve_exact(len).is_err() {
                return None;
            }
            out.resize(len, fill);
            Some(out)
        }

        let total_pixels = (width as usize) * (height as usize);
        if total_pixels == 0 {
            return None;
        }
        let thumb_w = thumb_w.max(1) as usize;
        let thumb_h = thumb_h.max(1) as usize;
        let thumb_pixels = thumb_w * thumb_h;
        let thumb_plane = (thumb_pixels + 7) / 8;
        let mut sum_bw = alloc_u16(thumb_pixels)?;
        let mut sum_l = alloc_u16(thumb_pixels)?;
        let mut sum_m = alloc_u16(thumb_pixels)?;
        let mut counts = alloc_u16(thumb_pixels)?;

        let mut load_from_reader = |reader: &mut dyn Read<Error = <F::File<'_> as embedded_io::ErrorType>::Error>|
            -> Result<(), ImageError> {
            let mut header = [0u8; 16];
            read_exact(reader, &mut header)?;
            if &header[0..4] != b"TRIM" || header[4] != 2 || header[5] != 2 {
                return Err(ImageError::Unsupported);
            }
            let w = u16::from_le_bytes([header[6], header[7]]) as u32;
            let h = u16::from_le_bytes([header[8], header[9]]) as u32;
            if w != width || h != height {
                return Err(ImageError::Decode);
            }

            let plane_len = (total_pixels + 7) / 8;
            let mut tmp = [0u8; 256];
            let mut pixel_index = 0usize;
            let mut read_plane = |sum: &mut [u16], track_count: bool| -> Result<(), ImageError> {
                pixel_index = 0;
                let mut remaining = plane_len;
                while remaining > 0 {
                    let want = remaining.min(tmp.len());
                    read_exact(reader, &mut tmp[..want])?;
                    for byte in &tmp[..want] {
                        for bit in 0..8 {
                            if pixel_index >= total_pixels {
                                break;
                            }
                            let sx = pixel_index % (width as usize);
                            let sy = pixel_index / (width as usize);
                            let dx = (sx * thumb_w) / (width as usize);
                            let dy = (sy * thumb_h) / (height as usize);
                            let bit_set = (byte >> (7 - bit)) & 0x01;
                            if dx < thumb_w && dy < thumb_h {
                                let dst = dy * thumb_w + dx;
                                if track_count {
                                    counts[dst] = counts[dst].saturating_add(1);
                                }
                                sum[dst] = sum[dst].saturating_add(bit_set as u16);
                            }
                            pixel_index += 1;
                        }
                    }
                    remaining -= want;
                }
                Ok(())
            };

            read_plane(&mut sum_bw, true)?;
            read_plane(&mut sum_l, false)?;
            read_plane(&mut sum_m, false)?;
            Ok(())
        };

        let result = if let Some(offset_str) = key.strip_prefix("trbk:") {
            let offset: u32 = offset_str.parse().ok()?;
            let state = self.trbk.as_ref()?;
            let file_path = if state.path.is_empty() {
                state
                    .short_name
                    .as_deref()
                    .unwrap_or(state.name.as_str())
                    .to_string()
            } else {
                format!("{}/{}", state.path.join("/"), state.name)
            };
            let mut file = self.fs.open_file(&file_path, Mode::Read).ok()?;
            file.seek(SeekFrom::Start(offset as u64)).ok()?;
            load_from_reader(&mut file)
        } else {
            let mut file = self.fs.open_file(key, Mode::Read).ok()?;
            load_from_reader(&mut file)
        };

        if result.is_err() {
            return None;
        }

        let mut bits = alloc_u8(thumb_plane, 0xFF)?;
        for idx in 0..thumb_pixels {
            let count = counts[idx].max(1) as i32;
            let avg_bw = sum_bw[idx] as i32;
            let avg_l = sum_l[idx] as i32;
            let avg_m = sum_m[idx] as i32;
            let mut lum = (255 * avg_bw + 128 * avg_m - 64 * avg_l) / count;
            if lum < 0 {
                lum = 0;
            } else if lum > 255 {
                lum = 255;
            }
            let lum = adjust_thumbnail_luma(lum as u8);
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            if lum >= 128 {
                bits[byte] |= 1 << bit;
            } else {
                bits[byte] &= !(1 << bit);
            }
        }

        Some(ImageData::Mono1 {
            width: thumb_w as u32,
            height: thumb_h as u32,
            bits,
        })
    }
}

impl<F> BookSource for SdImageSource<F>
where
    F: Filesystem,
{
    fn load_trbk(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<tern_core::trbk::TrbkBook, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Unsupported);
        }
        let file_path = Self::build_path(path, &entry.name);
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;
        let file_len = file.size();

        const MAX_BOOK_BYTES: usize = 900_000;
        if file_len < 16 || file_len > MAX_BOOK_BYTES {
            return Err(ImageError::Message(
                "Book file too large for device.".into(),
            ));
        }

        let mut data = Vec::new();
        if data.try_reserve(file_len).is_err() {
            return Err(oom_error(file_len, "load_book_data: initial buffer"));
        }
        let mut buffer = [0u8; 512];
        while data.len() < file_len {
            let read = file.read(&mut buffer).map_err(|_| ImageError::Io)?;
            if read == 0 {
                break;
            }
            let remaining = file_len - data.len();
            let take = read.min(remaining);
            if data.try_reserve(take).is_err() {
                return Err(oom_error(take, "load_book_data: stream growth"));
            }
            data.extend_from_slice(&buffer[..take]);
        }
        if data.len() != file_len {
            return Err(ImageError::Decode);
        }

        tern_core::trbk::parse_trbk(&data)
    }

    fn open_trbk(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<Rc<tern_core::trbk::TrbkBookInfo>, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Unsupported);
        }
        let file_path = Self::build_path(path, &entry.name);
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;

        let mut header = [0u8; 0x30];
        read_exact(&mut file, &mut header)?;
        if &header[0..4] != b"TRBK" {
            return Err(ImageError::Decode);
        }
        let version = header[4];
        if version != 1 && version != 2 {
            return Err(ImageError::Unsupported);
        }
        let header_size = read_u16_le(&header, 0x06)? as usize;
        let screen_width = read_u16_le(&header, 0x08)?;
        let screen_height = read_u16_le(&header, 0x0A)?;
        let page_count = read_u32_le(&header, 0x0C)? as usize;
        let toc_count = read_u32_le(&header, 0x10)? as usize;
        let page_lut_offset = read_u32_le(&header, 0x14)? as u32;
        let toc_offset = read_u32_le(&header, 0x18)? as u32;
        let page_data_offset = read_u32_le(&header, 0x1C)? as u32;
        let (glyph_count, glyph_table_offset) = if version >= 2 {
            (
                read_u32_le(&header, 0x28)? as usize,
                read_u32_le(&header, 0x2C)? as u32,
            )
        } else {
            (0usize, 0u32)
        };
        let images_offset = if version >= 2 {
            read_u32_le(&header, 0x20)? as u32
        } else {
            0
        };

        if toc_count != 0 && toc_offset as usize != header_size {
            return Err(ImageError::Decode);
        }

        // Read header + metadata
        let mut header_buf = vec![0u8; header_size];
        file.seek(SeekFrom::Start(0)).map_err(|_| ImageError::Io)?;
        read_exact(&mut file, &mut header_buf)?;

        let mut cursor = if version >= 2 { 0x30 } else { 0x2C };
        let title = read_string(&header_buf, &mut cursor)?;
        let author = read_string(&header_buf, &mut cursor)?;
        let language = read_string(&header_buf, &mut cursor)?;
        let identifier = read_string(&header_buf, &mut cursor)?;
        let font_name = read_string(&header_buf, &mut cursor)?;
        let char_width = read_u16_le(&header_buf, cursor)?; cursor += 2;
        let line_height = read_u16_le(&header_buf, cursor)?; cursor += 2;
        let ascent = read_i16_le(&header_buf, cursor)?; cursor += 2;
        let margin_left = read_u16_le(&header_buf, cursor)?; cursor += 2;
        let margin_right = read_u16_le(&header_buf, cursor)?; cursor += 2;
        let margin_top = read_u16_le(&header_buf, cursor)?; cursor += 2;
        let margin_bottom = read_u16_le(&header_buf, cursor)?;

        let metadata = tern_core::trbk::TrbkMetadata {
            title,
            author,
            language,
            identifier,
            font_name,
            char_width,
            line_height,
            ascent,
            margin_left,
            margin_right,
            margin_top,
            margin_bottom,
        };

        let mut toc_entries = Vec::new();
        if toc_count > 0 {
            file.seek(SeekFrom::Start(toc_offset as u64))
                .map_err(|_| ImageError::Io)?;
            for _ in 0..toc_count {
                let mut len_buf = [0u8; 4];
                read_exact(&mut file, &mut len_buf)?;
                let title_len = u32::from_le_bytes(len_buf) as usize;
                let mut title_buf = vec![0u8; title_len];
                read_exact(&mut file, &mut title_buf)?;
                let title = core::str::from_utf8(&title_buf)
                    .map_err(|_| ImageError::Decode)?
                    .to_string();
                let mut entry_buf = [0u8; 4 + 1 + 1 + 2];
                read_exact(&mut file, &mut entry_buf)?;
                let page_index = u32::from_le_bytes([entry_buf[0], entry_buf[1], entry_buf[2], entry_buf[3]]);
                let level = entry_buf[4];
                toc_entries.push(tern_core::trbk::TrbkTocEntry {
                    title,
                    page_index,
                    level,
                });
            }
        }

        // Page offsets
        let lut_len = page_count * 4;
        let mut page_offsets = vec![0u8; lut_len];
        file.seek(SeekFrom::Start(page_lut_offset as u64))
            .map_err(|_| ImageError::Io)?;
        read_exact(&mut file, &mut page_offsets)?;
        let mut offsets = Vec::with_capacity(page_count);
        for i in 0..page_count {
            let idx = i * 4;
            offsets.push(u32::from_le_bytes([
                page_offsets[idx],
                page_offsets[idx + 1],
                page_offsets[idx + 2],
                page_offsets[idx + 3],
            ]));
        }

        // Glyphs
        let mut glyphs = Vec::new();
        if glyph_count > 0 {
            file.seek(SeekFrom::Start(glyph_table_offset as u64))
                .map_err(|_| ImageError::Io)?;
            for _ in 0..glyph_count {
                let mut header = [0u8; 4 + 1 + 1 + 1 + 2 + 2 + 2 + 4];
                read_exact(&mut file, &mut header)?;
                let codepoint = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                let style = header[4];
                let width = header[5];
                let height = header[6];
                let x_advance = i16::from_le_bytes([header[7], header[8]]);
                let x_offset = i16::from_le_bytes([header[9], header[10]]);
                let y_offset = i16::from_le_bytes([header[11], header[12]]);
                let bitmap_len = u32::from_le_bytes([header[13], header[14], header[15], header[16]]) as usize;
                let mut bitmap = vec![0u8; bitmap_len];
                read_exact(&mut file, &mut bitmap)?;
                let plane_len = ((width as usize * height as usize) + 7) / 8;
                let (bitmap_bw, bitmap_lsb, bitmap_msb) = if bitmap_len == plane_len * 3 {
                    let bw = bitmap[0..plane_len].to_vec();
                    let lsb = bitmap[plane_len..plane_len * 2].to_vec();
                    let msb = bitmap[plane_len * 2..plane_len * 3].to_vec();
                    (bw, Some(lsb), Some(msb))
                } else {
                    (bitmap, None, None)
                };
                glyphs.push(tern_core::trbk::TrbkGlyph {
                    codepoint,
                    style,
                    width,
                    height,
                    x_advance,
                    x_offset,
                    y_offset,
                    bitmap_bw,
                    bitmap_lsb,
                    bitmap_msb,
                });
            }
        }

        let mut images = Vec::new();
        if images_offset > 0 {
            file.seek(SeekFrom::Start(images_offset as u64))
                .map_err(|_| ImageError::Io)?;
            let mut count_buf = [0u8; 4];
            read_exact(&mut file, &mut count_buf)?;
            let image_count = u32::from_le_bytes(count_buf) as usize;

            let mut first_buf = [0u8; 16];
            if image_count > 0 {
                read_exact(&mut file, &mut first_buf)?;
            }
            let table_size_16 = 4 + image_count * 16;
            let table_size_14 = 4 + image_count * 14;
            let rel_offset_16 = u32::from_le_bytes([first_buf[0], first_buf[1], first_buf[2], first_buf[3]]);
            let rel_offset_14 = u32::from_le_bytes([first_buf[0], first_buf[1], first_buf[2], first_buf[3]]);
            let entry_size = if image_count == 0 {
                16
            } else if rel_offset_16 as usize == table_size_16 {
                16
            } else if rel_offset_14 as usize == table_size_14 {
                14
            } else {
                16
            };

            let parse_entry = |entry_buf: &[u8]| {
                let rel_offset = u32::from_le_bytes([entry_buf[0], entry_buf[1], entry_buf[2], entry_buf[3]]);
                let data_len = u32::from_le_bytes([entry_buf[4], entry_buf[5], entry_buf[6], entry_buf[7]]);
                let width = u16::from_le_bytes([entry_buf[8], entry_buf[9]]);
                let height = u16::from_le_bytes([entry_buf[10], entry_buf[11]]);
                (rel_offset, data_len, width, height)
            };

            if image_count > 0 {
                let (rel_offset, data_len, width, height) = parse_entry(&first_buf);
                let data_offset = images_offset.saturating_add(rel_offset);
                images.push(tern_core::trbk::TrbkImageInfo {
                    data_offset,
                    data_len,
                    width,
                    height,
                });
            }

            for _ in 1..image_count {
                if entry_size == 16 {
                    let mut entry_buf = [0u8; 16];
                    read_exact(&mut file, &mut entry_buf)?;
                    let (rel_offset, data_len, width, height) = parse_entry(&entry_buf);
                    let data_offset = images_offset.saturating_add(rel_offset);
                    images.push(tern_core::trbk::TrbkImageInfo {
                        data_offset,
                        data_len,
                        width,
                        height,
                    });
                } else {
                    let mut entry_buf = [0u8; 14];
                    read_exact(&mut file, &mut entry_buf)?;
                    let rel_offset = u32::from_le_bytes([entry_buf[0], entry_buf[1], entry_buf[2], entry_buf[3]]);
                    let data_len = u32::from_le_bytes([entry_buf[4], entry_buf[5], entry_buf[6], entry_buf[7]]);
                    let width = u16::from_le_bytes([entry_buf[8], entry_buf[9]]);
                    let height = u16::from_le_bytes([entry_buf[10], entry_buf[11]]);
                    let data_offset = images_offset.saturating_add(rel_offset);
                    images.push(tern_core::trbk::TrbkImageInfo {
                        data_offset,
                        data_len,
                        width,
                        height,
                    });
                }
            }
        }

        let glyphs = Rc::new(glyphs);
        let info = Rc::new(tern_core::trbk::TrbkBookInfo {
            screen_width,
            screen_height,
            page_count,
            metadata,
            glyphs: glyphs.clone(),
            toc: toc_entries,
            images,
        });

        self.trbk = Some(TrbkStream {
            path: path.to_vec(),
            name: entry.name.clone(),
            short_name: self.lookup_short_name(&entry.name),
            page_offsets: offsets,
            page_data_offset,
            glyph_table_offset,
            info: info.clone(),
        });

        Ok(info)
    }

    fn trbk_page(&mut self, page_index: usize) -> Result<tern_core::trbk::TrbkPage, ImageError> {
        let Some(state) = &self.trbk else {
            return Err(ImageError::Decode);
        };
        if page_index >= state.page_offsets.len() {
            return Err(ImageError::Decode);
        }
        let file_path = if state.path.is_empty() {
            state
                .short_name
                .as_deref()
                .unwrap_or(state.name.as_str())
                .to_string()
        } else {
            Self::build_path(&state.path, &state.name)
        };
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;

        let start = state.page_data_offset + state.page_offsets[page_index];
        let end = if page_index + 1 < state.page_offsets.len() {
            state.page_data_offset + state.page_offsets[page_index + 1]
        } else {
            state.glyph_table_offset
        };
        if end < start {
            return Err(ImageError::Decode);
        }
        let len = (end - start) as usize;
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(start as u64))
            .map_err(|_| ImageError::Io)?;
        read_exact(&mut file, &mut buf)?;
        let ops = tern_core::trbk::parse_trbk_page_ops(&buf)?;
        Ok(tern_core::trbk::TrbkPage { ops })
    }

    fn trbk_image(&mut self, image_index: usize) -> Result<ImageData, ImageError> {
        let Some(state) = &self.trbk else {
            return Err(ImageError::Decode);
        };
        let image = state
            .info
            .images
            .get(image_index)
            .ok_or(ImageError::Decode)?;
        let file_path = if state.path.is_empty() {
            state
                .short_name
                .as_deref()
                .unwrap_or(state.name.as_str())
                .to_string()
        } else {
            Self::build_path(&state.path, &state.name)
        };
        let mut file = self
            .fs
            .open_file(&file_path, Mode::Read)
            .map_err(|_| ImageError::Io)?;
        file.seek(SeekFrom::Start(image.data_offset as u64))
            .map_err(|_| ImageError::Io)?;
        let mut header = [0u8; 16];
        read_exact(&mut file, &mut header)?;
        if &header[0..4] == b"TRIM" && header[4] == 2 && header[5] == 2 {
            let w = u16::from_le_bytes([header[6], header[7]]) as u32;
            let h = u16::from_le_bytes([header[8], header[9]]) as u32;
            if w == image.width as u32 && h == image.height as u32 {
                let plane_len = ((w as usize * h as usize) + 7) / 8;
                if plane_len.saturating_mul(3) >= tern_core::framebuffer::BUFFER_SIZE {
                    // For large grayscale images, stream directly from TRBK to avoid heap.
                    let key = alloc::format!("trbk:{}", image.data_offset);
                    return Ok(ImageData::Gray2Stream { width: w, height: h, key });
                }
            }
        }
        file.seek(SeekFrom::Start(image.data_offset as u64))
            .map_err(|_| ImageError::Io)?;
        read_trimg_from_file(&mut file, image.data_len as usize)
    }

    fn close_trbk(&mut self) {
        self.trbk = None;
    }
}

impl<F> PowerSource for SdImageSource<F>
where
    F: Filesystem,
{
}


fn adjust_thumbnail_luma(lum: u8) -> u8 {
    let mut value = ((lum as i32 - 128) * 13) / 10 + 128;
    if value < 0 {
        value = 0;
    } else if value > 255 {
        value = 255;
    }
    value as u8
}
