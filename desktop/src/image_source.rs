use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use log::error;
use tern_core::palm_db::{DbKind, InstallDecision, InstallInboxEntry, InstallPlanner, InstallSummary, InstalledDbIdentity, InstalledDbMeta};
use tern_core::image_viewer::{
    BookSource, EntryKind, Gray2StreamSource, ImageData, ImageEntry, ImageError, ImageSource,
    InstalledAppEntry,
    PersistenceSource, PowerSource,
};

mod embedded_prc_fonts {
    include!(concat!(env!("OUT_DIR"), "/prc_embedded_fonts.rs"));
}

pub struct DesktopImageSource {
    root: PathBuf,
    trbk_pages: Option<Vec<tern_core::trbk::TrbkPage>>,
    trbk_data: Option<Vec<u8>>,
    trbk_images: Option<Vec<tern_core::trbk::TrbkImageInfo>>,
}

impl DesktopImageSource {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            trbk_pages: None,
            trbk_data: None,
            trbk_images: None,
        }
    }

    fn is_supported(name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        name.ends_with(".png")
            || name.ends_with(".jpg")
            || name.ends_with(".jpeg")
            || name.ends_with(".trimg")
            || name.ends_with(".tri")
            || name.ends_with(".trbk")
            || name.ends_with(".prc")
            || name.ends_with(".tdb")
    }

    fn resume_path(&self) -> PathBuf {
        self.root.join(".tern_resume")
    }

    fn resume_path_legacy(&self) -> PathBuf {
        self.root.join(".trusty_resume")
    }

    fn book_positions_path(&self) -> PathBuf {
        self.root.join(".tern_books")
    }

    fn book_positions_path_legacy(&self) -> PathBuf {
        self.root.join(".trusty_books")
    }

    fn recent_entries_path(&self) -> PathBuf {
        self.root.join(".tern_recents")
    }

    fn recent_entries_path_legacy(&self) -> PathBuf {
        self.root.join(".trusty_recents")
    }

    fn thumbnail_dir(&self) -> PathBuf {
        self.root.join(".tern_cache")
    }

    fn thumbnail_dir_legacy(&self) -> PathBuf {
        self.root.join(".trusty_cache")
    }

    fn thumbnail_path(&self, key: &str) -> PathBuf {
        let name = format!("thumb_{}.tri", thumb_hash_hex(key));
        self.thumbnail_dir().join(name)
    }

    fn thumbnail_title_path(&self, key: &str) -> PathBuf {
        let name = format!("thumb_{}.txt", thumb_hash_hex(key));
        self.thumbnail_dir().join(name)
    }

    fn load_trbk_data(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<(tern_core::trbk::TrbkBook, Vec<u8>), ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Unsupported);
        }
        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let path = base.join(&entry.name);
        let data = fs::read(&path).map_err(|_| ImageError::Io)?;
        match tern_core::trbk::parse_trbk(&data) {
            Ok(book) => Ok((book, data)),
            Err(err) => {
                log_trbk_header(&data, &path);
                Err(err)
            }
        }
    }

    fn fonts_dir(&self) -> PathBuf {
        self.root.join("fonts")
    }

    fn install_dir(&self) -> PathBuf {
        self.root.join("install")
    }

    fn palmdb_root(&self) -> PathBuf {
        self.root.join("palmdb").join("v1")
    }

    fn palmdb_catalog_path(&self) -> PathBuf {
        self.palmdb_root().join("catalog.txt")
    }

    fn palmdb_db_dir(&self) -> PathBuf {
        self.palmdb_root().join("db")
    }

    fn load_prc_fonts_with_variant(
        &self,
        prefer_144: bool,
    ) -> Vec<tern_core::prc_app::runtime::PalmFont> {
        let mut out = Vec::new();
        fn font_variant_rank(name: &str, prefer_144: bool) -> u8 {
            let lower = name.to_ascii_lowercase();
            if prefer_144 {
                if lower.ends_with("_144.txt") {
                    0
                } else if lower.ends_with("_72.txt") {
                    2
                } else {
                    1
                }
            } else if lower.ends_with("_72.txt") {
                0
            } else if lower.ends_with("_144.txt") {
                2
            } else {
                1
            }
        }
        let mut embedded: std::collections::BTreeMap<u16, (&str, &str, u8)> =
            std::collections::BTreeMap::new();
        for (name, text) in embedded_prc_fonts::EMBEDDED_PRC_FONT_TXT {
            let Some(resource_id) = tern_core::prc_app::font::parse_font_resource_id_from_name(name) else {
                continue;
            };
            let font_id = resource_id.saturating_sub(9100);
            let rank = font_variant_rank(name, prefer_144);
            match embedded.get(&font_id) {
                Some((_, _, cur_rank)) if *cur_rank <= rank => {}
                _ => {
                    embedded.insert(font_id, (name, text, rank));
                }
            }
        }
        for (font_id, (_name, text, _rank)) in embedded.into_iter() {
            if let Some(font) = tern_core::prc_app::font::parse_pumpkin_txt_font(text, font_id) {
                out.push(font);
            }
        }
        let embedded_loaded = out.len();
        if embedded_loaded > 0 {
            log::info!(
                "Loaded {} embedded text system fonts from app image",
                embedded_loaded
            );
        }

        let dir = self.fonts_dir();
        let Ok(read_dir) = fs::read_dir(&dir) else {
            return out;
        };
        let mut sd_candidates: std::collections::BTreeMap<u16, (String, u8)> =
            std::collections::BTreeMap::new();
        for dent in read_dir.flatten() {
            let Ok(ft) = dent.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let name = dent.file_name().to_string_lossy().to_string();
            if !name.to_ascii_lowercase().ends_with(".txt") {
                continue;
            }
            let Some(resource_id) = tern_core::prc_app::font::parse_font_resource_id_from_name(&name) else {
                continue;
            };
            let font_id = resource_id.saturating_sub(9100);
            let rank = font_variant_rank(&name, prefer_144);
            match sd_candidates.get(&font_id) {
                Some((_, cur_rank)) if *cur_rank <= rank => {}
                _ => {
                    sd_candidates.insert(font_id, (dent.path().to_string_lossy().to_string(), rank));
                }
            }
        }
        for (font_id, (path, _rank)) in sd_candidates {
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            if let Some(pos) = out
                .iter()
                .position(|f: &tern_core::prc_app::runtime::PalmFont| f.font_id == font_id)
            {
                if let Some(font) = tern_core::prc_app::font::parse_pumpkin_txt_font(&text, font_id) {
                    out[pos] = font;
                }
            } else if let Some(font) = tern_core::prc_app::font::parse_pumpkin_txt_font(&text, font_id) {
                out.push(font);
            }
        }
        if out.len() > embedded_loaded {
            log::info!(
                "Loaded {} text system fonts from {}",
                out.len() - embedded_loaded,
                dir.display()
            );
        }
        out
    }

}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
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
        s1 = (s1 ^ (v.rotate_left(13))).wrapping_mul(0x100000001b3);
        s2 = (s2 ^ (v.rotate_left(29))).wrapping_mul(0x100000001b3);
        s3 = (s3 ^ (v.rotate_left(47))).wrapping_mul(0x100000001b3);
    }
    let mut out = [0u8; 32];
    out[0..8].copy_from_slice(&s0.to_le_bytes());
    out[8..16].copy_from_slice(&s1.to_le_bytes());
    out[16..24].copy_from_slice(&s2.to_le_bytes());
    out[24..32].copy_from_slice(&s3.to_le_bytes());
    out
}

fn identity_from_prc(info: &tern_core::prc_app::PrcInfo) -> InstalledDbIdentity {
    let mut name = [0u8; 32];
    let raw_name = info.db_name.as_bytes();
    let copy_n = raw_name.len().min(name.len());
    name[..copy_n].copy_from_slice(&raw_name[..copy_n]);
    let mut db_type = [0u8; 4];
    let mut creator = [0u8; 4];
    db_type.copy_from_slice(info.type_code.as_bytes().get(..4).unwrap_or(b"????"));
    creator.copy_from_slice(info.creator_code.as_bytes().get(..4).unwrap_or(b"????"));
    InstalledDbIdentity {
        name,
        db_type,
        creator,
        version: info.version,
    }
}

fn extract_app_icon(raw: &[u8]) -> Option<ImageData> {
    let bitmaps = tern_core::prc_app::bitmap::parse_prc_bitmaps(raw);
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

fn load_catalog(path: &Path) -> Vec<InstalledDbMeta> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != 10 {
            continue;
        }
        let Some(name) = hex_decode_fixed::<32>(cols[6]) else {
            continue;
        };
        let Some(db_type) = hex_decode_fixed::<4>(cols[7]) else {
            continue;
        };
        let Some(creator) = hex_decode_fixed::<4>(cols[8]) else {
            continue;
        };
        let Some(payload_hash) = hex_decode_fixed::<32>(cols[9]) else {
            continue;
        };
        let kind = if cols[2] == "resource" {
            DbKind::Resource
        } else {
            DbKind::Record
        };
        let Ok(uid) = cols[0].parse::<u64>() else {
            continue;
        };
        let Ok(card_no) = cols[1].parse::<u16>() else {
            continue;
        };
        let Ok(attributes) = cols[3].parse::<u16>() else {
            continue;
        };
        let Ok(mod_number) = cols[4].parse::<u32>() else {
            continue;
        };
        let Ok(version) = cols[5].parse::<u16>() else {
            continue;
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

fn save_catalog(path: &Path, entries: &[InstalledDbMeta]) -> Result<(), ImageError> {
    let mut text = String::new();
    for e in entries {
        let kind = match e.kind {
            DbKind::Resource => "resource",
            DbKind::Record => "record",
        };
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            e.uid,
            e.card_no,
            kind,
            e.attributes,
            e.mod_number,
            e.identity.version,
            hex_encode(&e.identity.name),
            hex_encode(&e.identity.db_type),
            hex_encode(&e.identity.creator),
            hex_encode(&e.payload_hash)
        ));
    }
    fs::write(path, text).map_err(|_| ImageError::Io)
}

impl ImageSource for DesktopImageSource {
    fn refresh(&mut self, path: &[String]) -> Result<Vec<ImageEntry>, ImageError> {
        let mut entries = Vec::new();
        let dir_path = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let read_dir = match fs::read_dir(&dir_path) {
            Ok(read_dir) => read_dir,
            Err(_) => return Ok(entries),
        };
        for entry in read_dir {
            let entry = entry.map_err(|_| ImageError::Io)?;
            let file_type = entry.file_type().map_err(|_| ImageError::Io)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".tern_resume"
                || name == ".trusty_resume"
                || name == ".tern_books"
                || name == ".trusty_books"
                || name == ".tern_recents"
                || name == ".trusty_recents"
                || name == ".tern_cache"
                || name == ".trusty_cache"
            {
                continue;
            }
            if file_type.is_dir() {
                entries.push(ImageEntry {
                    name,
                    kind: EntryKind::Dir,
                });
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if Self::is_supported(&name) {
                entries.push(ImageEntry {
                    name,
                    kind: EntryKind::File,
                });
            }
        }
        entries.sort_by(|a, b| {
            match (a.kind, b.kind) {
                (EntryKind::Dir, EntryKind::File) => std::cmp::Ordering::Less,
                (EntryKind::File, EntryKind::Dir) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        Ok(entries)
    }

    fn load(&mut self, path: &[String], entry: &ImageEntry) -> Result<ImageData, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Unsupported);
        }
        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let path = base.join(&entry.name);
        let lower = entry.name.to_ascii_lowercase();
        if lower.ends_with(".trbk") {
            return Err(ImageError::Unsupported);
        }
        if lower.ends_with(".trimg") || lower.ends_with(".tri") {
            let data = fs::read(&path).map_err(|_| ImageError::Io)?;
            return parse_trimg(&data);
        }

        let data = fs::read(&path).map_err(|_| ImageError::Io)?;
        let image = image::load_from_memory(&data).map_err(|_| ImageError::Decode)?;
        let luma = image.to_luma8();
        Ok(ImageData::Gray8 {
            width: luma.width(),
            height: luma.height(),
            pixels: luma.into_raw(),
        })
    }

    fn load_prc_info(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<tern_core::prc_app::PrcInfo, ImageError> {
        let lower = entry.name.to_ascii_lowercase();
        if entry.kind != EntryKind::File || (!lower.ends_with(".prc") && !lower.ends_with(".tdb")) {
            return Err(ImageError::Unsupported);
        }
        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let full_path = base.join(&entry.name);
        let data = fs::read(full_path).map_err(|_| ImageError::Io)?;
        tern_core::prc_app::parse_prc(&data).ok_or(ImageError::Decode)
    }

    fn load_prc_code_resource(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
        resource_id: u16,
    ) -> Result<Vec<u8>, ImageError> {
        let lower = entry.name.to_ascii_lowercase();
        if entry.kind != EntryKind::File || (!lower.ends_with(".prc") && !lower.ends_with(".tdb")) {
            return Err(ImageError::Unsupported);
        }
        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let full_path = base.join(&entry.name);
        let data = fs::read(full_path).map_err(|_| ImageError::Io)?;
        let info = tern_core::prc_app::parse_prc(&data).ok_or(ImageError::Decode)?;
        let res = info
            .resources
            .iter()
            .find(|res| res.kind == "code" && res.id == resource_id)
            .ok_or(ImageError::Unsupported)?;
        let start = res.offset as usize;
        let end = start.saturating_add(res.size as usize);
        let slice = data.get(start..end).ok_or(ImageError::Decode)?;
        Ok(slice.to_vec())
    }

    fn load_prc_bytes(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<Vec<u8>, ImageError> {
        let lower = entry.name.to_ascii_lowercase();
        if entry.kind != EntryKind::File || (!lower.ends_with(".prc") && !lower.ends_with(".tdb")) {
            return Err(ImageError::Unsupported);
        }
        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let full_path = base.join(&entry.name);
        fs::read(full_path).map_err(|_| ImageError::Io)
    }

    fn load_prc_app_resources(
        &mut self,
        _path: &[String],
        entry: &ImageEntry,
        info: &tern_core::prc_app::PrcInfo,
    ) -> Vec<tern_core::prc_app::runtime::ResourceBlob> {
        let mut out = Vec::new();
        let Some(current_uid) = parse_tdb_uid_from_name(&entry.name) else {
            return out;
        };
        let creator = info.creator_code.as_bytes();
        let Ok(creator_4) = <[u8; 4]>::try_from(creator.get(..4).unwrap_or(b"????")) else {
            return out;
        };
        let catalog = load_catalog(&self.palmdb_catalog_path());
        for meta in catalog {
            if meta.uid == current_uid {
                continue;
            }
            if meta.identity.db_type != *b"ovly" || meta.identity.creator != creator_4 {
                continue;
            }
            let path = self
                .root
                .join(format!("palmdb/v1/db/{:016x}.tdb", meta.uid));
            let Ok(raw) = fs::read(path) else {
                continue;
            };
            let blobs = tern_core::prc_app::parse_prc_resource_blobs(&raw);
            out.extend(blobs);
        }
        out
    }

    fn load_prc_system_resources(&mut self) -> Vec<tern_core::prc_app::runtime::ResourceBlob> {
        let dir = self.fonts_dir();
        let Ok(read_dir) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for dent in read_dir.flatten() {
            let Ok(ft) = dent.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let name = dent.file_name().to_string_lossy().to_string();
            if !tern_core::prc_app::font::is_prc_font_resource_blob_name(&name)
            {
                continue;
            }
            let Some(id) = tern_core::prc_app::font::parse_font_resource_id_from_name(&name) else {
                continue;
            };
            let Ok(data) = fs::read(dent.path()) else {
                continue;
            };
            if data.len() < 26 {
                continue;
            }
            out.push(tern_core::prc_app::runtime::ResourceBlob {
                kind: u32::from_be_bytes(*b"NFNT"),
                id,
                data,
            });
        }
        if !out.is_empty() {
            log::info!("Loaded {} system font resources from {}", out.len(), dir.display());
        }
        out
    }

    fn load_prc_system_fonts(&mut self) -> Vec<tern_core::prc_app::runtime::PalmFont> {
        self.load_prc_fonts_with_variant(false)
    }

    fn load_home_system_fonts(&mut self) -> Vec<tern_core::prc_app::runtime::PalmFont> {
        self.load_prc_fonts_with_variant(true)
    }

    fn scan_palm_install_inbox(&mut self) -> Option<InstallSummary> {
        let install_dir = self.install_dir();
        let Ok(read_dir) = fs::read_dir(&install_dir) else {
            return Some(InstallSummary::default());
        };

        let db_dir = self.palmdb_db_dir();
        if fs::create_dir_all(&db_dir).is_err() {
            return Some(InstallSummary {
                scanned: 0,
                installed: 0,
                upgraded: 0,
                skipped: 0,
                failed: 1,
            });
        }
        let catalog_path = self.palmdb_catalog_path();
        let mut catalog = load_catalog(&catalog_path);
        let mut summary = InstallSummary::default();

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with('.') || name.starts_with("._") {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if ext != "prc" && ext != "pdb" {
                continue;
            }
            summary.scanned += 1;
            let Ok(data) = fs::read(&path) else {
                summary.failed += 1;
                continue;
            };
            let Some(info) = tern_core::prc_app::parse_prc(&data) else {
                summary.failed += 1;
                continue;
            };
            let identity = identity_from_prc(&info);
            let payload_hash = payload_hash_32(&data);
            let existing_idx = catalog
                .iter()
                .position(|m| same_db_key(&m.identity, &identity));
            let decision = InstallPlanner::decide(
                &InstallInboxEntry {
                    path: path.to_string_lossy().into_owned(),
                    size: data.len() as u64,
                    identity,
                    payload_hash,
                },
                existing_idx.and_then(|i| catalog.get(i)),
            );

            match decision {
                InstallDecision::SkipAlreadyInstalled => {
                    summary.skipped += 1;
                    let _ = fs::remove_file(&path);
                }
                InstallDecision::InstallNew => {
                    let uid = catalog.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                    let db_path = db_dir.join(format!("{uid:016x}.tdb"));
                    if fs::write(&db_path, &data).is_err() {
                        summary.failed += 1;
                        continue;
                    }
                    let kind = match info.kind {
                        tern_core::prc_app::PrcDbKind::Resource => DbKind::Resource,
                        tern_core::prc_app::PrcDbKind::Record => DbKind::Record,
                    };
                    catalog.push(InstalledDbMeta {
                        uid,
                        card_no: 0,
                        identity,
                        kind,
                        attributes: info.attributes,
                        mod_number: 1,
                        payload_hash,
                    });
                    summary.installed += 1;
                    let _ = fs::remove_file(&path);
                }
                InstallDecision::UpgradeExisting { existing_uid } => {
                    let db_path = db_dir.join(format!("{existing_uid:016x}.tdb"));
                    if fs::write(&db_path, &data).is_err() {
                        summary.failed += 1;
                        continue;
                    }
                    if let Some(meta) = catalog.iter_mut().find(|m| m.uid == existing_uid) {
                        meta.identity = identity;
                        meta.attributes = info.attributes;
                        meta.kind = match info.kind {
                            tern_core::prc_app::PrcDbKind::Resource => DbKind::Resource,
                            tern_core::prc_app::PrcDbKind::Record => DbKind::Record,
                        };
                        meta.mod_number = meta.mod_number.saturating_add(1);
                        meta.payload_hash = payload_hash;
                    }
                    summary.upgraded += 1;
                    let _ = fs::remove_file(&path);
                }
            }
        }

        if summary.scanned > 0 && save_catalog(&catalog_path, &catalog).is_err() {
            summary.failed = summary.failed.saturating_add(1);
        }
        Some(summary)
    }

    fn list_installed_apps(&mut self) -> Vec<InstalledAppEntry> {
        let catalog = load_catalog(&self.palmdb_catalog_path());
        let mut out = Vec::new();
        for meta in catalog {
            // Show launchable app-like databases in Home > Apps.
            // `panl` is used by Palm control-panel apps like Date & Time.
            if meta.identity.db_type != *b"appl" && meta.identity.db_type != *b"panl" {
                continue;
            }
            let path = format!("palmdb/v1/db/{:016x}.tdb", meta.uid);
            let icon = fs::read(self.root.join(&path))
                .ok()
                .and_then(|raw| extract_app_icon(&raw));
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

impl PersistenceSource for DesktopImageSource {
    fn save_resume(&mut self, name: Option<&str>) {
        let path = self.resume_path();
        if let Some(name) = name {
            let _ = fs::write(path, name.as_bytes());
        } else {
            let _ = fs::remove_file(path);
        }
    }

    fn load_resume(&mut self) -> Option<String> {
        let data = fs::read(self.resume_path())
            .or_else(|_| fs::read(self.resume_path_legacy()))
            .ok()?;
        let name = String::from_utf8_lossy(&data).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    fn save_book_positions(&mut self, entries: &[(String, usize)]) {
        let path = self.book_positions_path();
        if entries.is_empty() {
            let _ = fs::remove_file(path);
            return;
        }
        let mut contents = String::new();
        for (name, page) in entries {
            contents.push_str(name);
            contents.push('\t');
            contents.push_str(&page.to_string());
            contents.push('\n');
        }
        let _ = fs::write(path, contents.as_bytes());
    }

    fn load_book_positions(&mut self) -> Vec<(String, usize)> {
        let data = match fs::read(self.book_positions_path())
            .or_else(|_| fs::read(self.book_positions_path_legacy()))
        {
            Ok(data) => data,
            Err(_) => return Vec::new(),
        };
        let text = String::from_utf8_lossy(&data);
        let mut entries = Vec::new();
        for line in text.lines() {
            let Some((name, page_str)) = line.split_once('\t') else {
                continue;
            };
            let name = name.trim();
            let page_str = page_str.trim();
            if name.is_empty() {
                continue;
            }
            let Ok(page) = page_str.parse::<usize>() else {
                continue;
            };
            entries.push((name.to_string(), page));
        }
        entries
    }

    fn save_recent_entries(&mut self, entries: &[String]) {
        let path = self.recent_entries_path();
        if entries.is_empty() {
            let _ = fs::remove_file(path);
            return;
        }
        let mut contents = String::new();
        for entry in entries {
            contents.push_str(entry);
            contents.push('\n');
        }
        let _ = fs::write(path, contents.as_bytes());
    }

    fn load_recent_entries(&mut self) -> Vec<String> {
        let data = match fs::read(self.recent_entries_path())
            .or_else(|_| fs::read(self.recent_entries_path_legacy()))
        {
            Ok(data) => data,
            Err(_) => return Vec::new(),
        };
        let text = String::from_utf8_lossy(&data);
        let mut entries = Vec::new();
        for line in text.lines() {
            let value = line.trim();
            if !value.is_empty() {
                entries.push(value.to_string());
            }
        }
        entries
    }

    fn load_thumbnail(&mut self, key: &str) -> Option<ImageData> {
        let data = fs::read(self.thumbnail_path(key))
            .or_else(|_| fs::read(self.thumbnail_dir_legacy().join(format!("thumb_{}.tri", thumb_hash_hex(key)))))
            .ok()?;
        parse_trimg(&data).ok()
    }

    fn save_thumbnail(&mut self, key: &str, image: &ImageData) {
        let Some(data) = serialize_thumbnail(image) else {
            return;
        };
        let dir = self.thumbnail_dir();
        let _ = fs::create_dir_all(&dir);
        let path = self.thumbnail_path(key);
        let _ = fs::write(path, &data);
    }

    fn load_thumbnail_title(&mut self, key: &str) -> Option<String> {
        let data = fs::read(self.thumbnail_title_path(key))
            .or_else(|_| fs::read(self.thumbnail_dir_legacy().join(format!("thumb_{}.txt", thumb_hash_hex(key)))))
            .ok()?;
        let text = String::from_utf8_lossy(&data).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn save_thumbnail_title(&mut self, key: &str, title: &str) {
        let dir = self.thumbnail_dir();
        let _ = fs::create_dir_all(&dir);
        let path = self.thumbnail_title_path(key);
        let _ = fs::write(path, title.as_bytes());
    }

}

impl BookSource for DesktopImageSource {
    fn load_trbk(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<tern_core::trbk::TrbkBook, ImageError> {
        let (book, _) = self.load_trbk_data(path, entry)?;
        Ok(book)
    }

    fn open_trbk(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
    ) -> Result<Rc<tern_core::trbk::TrbkBookInfo>, ImageError> {
        let (book, data) = self.load_trbk_data(path, entry)?;
        let info = Rc::new(book.info());
        self.trbk_pages = Some(book.pages);
        self.trbk_images = Some(info.images.clone());
        self.trbk_data = Some(data);
        Ok(info)
    }

    fn trbk_page(&mut self, page_index: usize) -> Result<tern_core::trbk::TrbkPage, ImageError> {
        let Some(pages) = self.trbk_pages.as_ref() else {
            return Err(ImageError::Decode);
        };
        pages
            .get(page_index)
            .cloned()
            .ok_or(ImageError::Decode)
    }

    fn trbk_image(&mut self, image_index: usize) -> Result<ImageData, ImageError> {
        let Some(images) = self.trbk_images.as_ref() else {
            return Err(ImageError::Decode);
        };
        let Some(data) = self.trbk_data.as_ref() else {
            return Err(ImageError::Decode);
        };
        let image = images.get(image_index).ok_or(ImageError::Decode)?;
        let start = image.data_offset as usize;
        let end = start + image.data_len as usize;
        if end > data.len() {
            return Err(ImageError::Decode);
        }
        parse_trimg(&data[start..end])
    }

    fn close_trbk(&mut self) {
        self.trbk_pages = None;
        self.trbk_data = None;
        self.trbk_images = None;
    }
}

impl Gray2StreamSource for DesktopImageSource {}

impl PowerSource for DesktopImageSource {}

fn log_trbk_header(data: &[u8], path: &Path) {
    if data.len() < 8 {
        error!(
            "TRBK parse failed: file {} too small ({} bytes)",
            path.display(),
            data.len()
        );
        return;
    }
    if &data[0..4] != b"TRBK" {
        error!(
            "TRBK parse failed: file {} missing magic (len={})",
            path.display(),
            data.len()
        );
        return;
    }
    let version = data[4];
    let header_size = u16::from_le_bytes([data[6], data[7]]) as usize;
    let page_count = if data.len() >= 0x10 {
        u32::from_le_bytes([data[0x0C], data[0x0D], data[0x0E], data[0x0F]])
    } else {
        0
    };
    let page_lut_offset = if data.len() >= 0x18 {
        u32::from_le_bytes([data[0x14], data[0x15], data[0x16], data[0x17]])
    } else {
        0
    };
    let page_data_offset = if data.len() >= 0x20 {
        u32::from_le_bytes([data[0x1C], data[0x1D], data[0x1E], data[0x1F]])
    } else {
        0
    };
    let glyph_count = if data.len() >= 0x2C {
        u32::from_le_bytes([data[0x28], data[0x29], data[0x2A], data[0x2B]])
    } else {
        0
    };
    let glyph_table_offset = if data.len() >= 0x30 {
        u32::from_le_bytes([data[0x2C], data[0x2D], data[0x2E], data[0x2F]])
    } else {
        0
    };
    error!(
        "TRBK parse failed: {} ver={} len={} header={} pages={} page_lut={} page_data={} glyphs={} glyph_off={}",
        path.display(),
        version,
        data.len(),
        header_size,
        page_count,
        page_lut_offset,
        page_data_offset,
        glyph_count,
        glyph_table_offset
    );
}

fn parse_trimg(data: &[u8]) -> Result<ImageData, ImageError> {
    if data.len() < 16 || &data[0..4] != b"TRIM" {
        return Err(ImageError::Decode);
    }
    let width = u16::from_le_bytes([data[6], data[7]]) as u32;
    let height = u16::from_le_bytes([data[8], data[9]]) as u32;
    let payload = &data[16..];
    let plane = ((width as usize * height as usize) + 7) / 8;
    match (data[4], data[5]) {
        (1, 1) => {
            if payload.len() != plane {
                return Err(ImageError::Decode);
            }
            Ok(ImageData::Mono1 {
                width,
                height,
                bits: payload.to_vec(),
            })
        }
        (2, 2) => {
            if payload.len() != plane * 3 {
                return Err(ImageError::Decode);
            }
            Ok(ImageData::Gray2 {
                width,
                height,
                data: payload.to_vec(),
            })
        }
        _ => Err(ImageError::Unsupported),
    }
}

fn thumb_hash_hex(key: &str) -> String {
    let mut hash: u32 = 0x811c9dc5;
    for b in key.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{:08x}", hash)
}

fn serialize_thumbnail(image: &ImageData) -> Option<Vec<u8>> {
    let (width, height, bits) = match image {
        ImageData::Mono1 {
            width,
            height,
            bits,
        } => (*width, *height, bits.as_slice()),
        ImageData::Gray2 { width, height, data } => (*width, *height, data.as_slice()),
        _ => return None,
    };
    let expected = ((width as usize * height as usize) + 7) / 8;
    if bits.len() != expected {
        return None;
    }
    let mut data = Vec::with_capacity(16 + bits.len());
    data.extend_from_slice(b"TRIM");
    data.push(1);
    data.push(1);
    data.extend_from_slice(&(width as u16).to_le_bytes());
    data.extend_from_slice(&(height as u16).to_le_bytes());
    data.extend_from_slice(&[0u8; 6]);
    data.extend_from_slice(bits);
    Some(data)
}
