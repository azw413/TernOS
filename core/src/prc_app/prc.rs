extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrcDbKind {
    Resource,
    Record,
}

#[derive(Clone, Debug)]
pub struct PrcSectionStat {
    pub name: String,
    pub count: u16,
    pub bytes: u32,
}

#[derive(Clone, Debug)]
pub struct PrcResourceEntry {
    pub kind: String,
    pub id: u16,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug)]
pub struct PrcCodeScan {
    pub resource_id: u16,
    pub size: u32,
    pub a_trap_count: u32,
    pub trap15_count: u32,
    pub unique_a_traps: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct PrcTrapHit {
    pub resource_id: u16,
    pub file_offset: u32,
    pub code_offset: u32,
    pub trap_word: u16,
    pub is_trap15: bool,
    pub prev_word: Option<u16>,
    pub next_word_1: Option<u16>,
    pub next_word_2: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct PrcInfo {
    pub db_name: String,
    pub kind: PrcDbKind,
    pub file_size: u32,
    pub type_code: String,
    pub creator_code: String,
    pub attributes: u16,
    pub version: u16,
    pub entry_count: u16,
    pub code_bytes: u32,
    pub other_bytes: u32,
    pub sections: Vec<PrcSectionStat>,
    pub resources: Vec<PrcResourceEntry>,
    pub code_scan: Vec<PrcCodeScan>,
    pub a_trap_total: u32,
    pub trap15_total: u32,
    pub unique_a_traps: Vec<u16>,
    pub trap_hits: Vec<PrcTrapHit>,
}

fn push_section(sections: &mut Vec<PrcSectionStat>, name: &str, bytes: u32) {
    for section in sections.iter_mut() {
        if section.name == name {
            section.count = section.count.saturating_add(1);
            section.bytes = section.bytes.saturating_add(bytes);
            return;
        }
    }
    sections.push(PrcSectionStat {
        name: name.into(),
        count: 1,
        bytes,
    });
}

fn add_unique_u16(values: &mut Vec<u16>, value: u16) {
    if !values.iter().any(|v| *v == value) {
        values.push(value);
    }
}

fn scan_68k_code(
    data: &[u8],
    resource_id: u16,
    file_base: u32,
    size: u32,
    trap_hits: &mut Vec<PrcTrapHit>,
) -> PrcCodeScan {
    let mut a_trap_count = 0u32;
    let mut trap15_count = 0u32;
    let mut unique_a_traps = Vec::new();

    let mut i = 0usize;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        if (word & 0xF000) == 0xA000 {
            a_trap_count = a_trap_count.saturating_add(1);
            add_unique_u16(&mut unique_a_traps, word);
            trap_hits.push(PrcTrapHit {
                resource_id,
                file_offset: file_base.saturating_add(i as u32),
                code_offset: i as u32,
                trap_word: word,
                is_trap15: false,
                prev_word: if i >= 2 {
                    Some(u16::from_be_bytes([data[i - 2], data[i - 1]]))
                } else {
                    None
                },
                next_word_1: if i + 3 < data.len() {
                    Some(u16::from_be_bytes([data[i + 2], data[i + 3]]))
                } else {
                    None
                },
                next_word_2: if i + 5 < data.len() {
                    Some(u16::from_be_bytes([data[i + 4], data[i + 5]]))
                } else {
                    None
                },
            });
        } else if (word & 0xFFF0) == 0x4E40 && (word & 0x000F) == 0x000F {
            trap15_count = trap15_count.saturating_add(1);
            trap_hits.push(PrcTrapHit {
                resource_id,
                file_offset: file_base.saturating_add(i as u32),
                code_offset: i as u32,
                trap_word: word,
                is_trap15: true,
                prev_word: if i >= 2 {
                    Some(u16::from_be_bytes([data[i - 2], data[i - 1]]))
                } else {
                    None
                },
                next_word_1: if i + 3 < data.len() {
                    Some(u16::from_be_bytes([data[i + 2], data[i + 3]]))
                } else {
                    None
                },
                next_word_2: if i + 5 < data.len() {
                    Some(u16::from_be_bytes([data[i + 4], data[i + 5]]))
                } else {
                    None
                },
            });
        }
        i += 2;
    }

    PrcCodeScan {
        resource_id,
        size,
        a_trap_count,
        trap15_count,
        unique_a_traps,
    }
}

pub fn format_info_lines(info: &PrcInfo) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!("Name: {}", info.db_name));
    out.push(format!("Type: {}", info.type_code));
    out.push(format!("Creator: {}", info.creator_code));
    out.push(format!(
        "Kind: {}",
        match info.kind {
            PrcDbKind::Resource => "Resource DB",
            PrcDbKind::Record => "Record DB",
        }
    ));
    out.push(format!("Entries: {}", info.entry_count));
    out.push(format!("Version: {}", info.version));
    out.push(format!("Attrs: 0x{:04X}", info.attributes));
    out.push(format!("File size: {} B", info.file_size));
    out.push(format!("Code bytes: {} B", info.code_bytes));
    out.push(format!("Other bytes: {} B", info.other_bytes));
    out.push(format!("A-traps total: {}", info.a_trap_total));
    out.push(format!("TRAP #15 total: {}", info.trap15_total));
    if !info.unique_a_traps.is_empty() {
        let mut trap_line = String::from("Unique A-traps:");
        for trap in info.unique_a_traps.iter().take(12) {
            trap_line.push(' ');
            let _ = core::fmt::Write::write_fmt(&mut trap_line, format_args!("0x{trap:04X}"));
        }
        if info.unique_a_traps.len() > 12 {
            trap_line.push_str(" ...");
        }
        out.push(trap_line);
    }
    out.push("Sections:".into());
    for section in &info.sections {
        out.push(format!(
            "  {}: {} entries, {} B",
            section.name, section.count, section.bytes
        ));
    }
    out.push("Resources:".into());
    for resource in info.resources.iter().take(20) {
        out.push(format!(
            "  {}#{} @{} ({} B)",
            resource.kind, resource.id, resource.offset, resource.size
        ));
    }
    if info.resources.len() > 20 {
        out.push(format!("  ... {} more", info.resources.len() - 20));
    }
    if !info.code_scan.is_empty() {
        out.push("Code scan:".into());
        for code in &info.code_scan {
            out.push(format!(
                "  code#{} {}B A:{} T15:{}",
                code.resource_id, code.size, code.a_trap_count, code.trap15_count
            ));
        }
    }
    out
}

pub fn parse_prc(data: &[u8]) -> Option<PrcInfo> {
    fn be_u16(data: &[u8], off: usize) -> Option<u16> {
        data.get(off..off + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    }
    fn be_u32(data: &[u8], off: usize) -> Option<u32> {
        data.get(off..off + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn clean_name(raw: &[u8]) -> String {
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        let mut s = String::new();
        for b in &raw[..end] {
            if (0x20..=0x7e).contains(b) {
                s.push(*b as char);
            }
        }
        s
    }
    fn fourcc(raw: &[u8]) -> String {
        let mut s = String::new();
        for b in raw {
            if (0x20..=0x7e).contains(b) {
                s.push(*b as char);
            } else {
                s.push('.');
            }
        }
        s
    }
    fn section_sizes(offsets: &[u32], file_size: u32, idx: usize) -> u32 {
        let cur = offsets[idx];
        let next = if idx + 1 < offsets.len() {
            offsets[idx + 1]
        } else {
            file_size
        };
        if next > cur { next - cur } else { 0 }
    }

    if data.len() < 78 {
        return None;
    }
    let attrs = be_u16(data, 32)?;
    let version = be_u16(data, 34)?;
    let entry_count = be_u16(data, 76)?;
    let is_resource = (attrs & 0x0001) != 0;
    let entry_size = if is_resource { 10usize } else { 8usize };
    let table_len = entry_size.saturating_mul(entry_count as usize);
    if 78 + table_len > data.len() {
        return None;
    }

    let mut sections = Vec::new();
    let mut resources = Vec::new();
    let mut code_scan = Vec::new();
    let mut code_bytes = 0u32;
    let mut other_bytes = 0u32;
    let mut a_trap_total = 0u32;
    let mut trap15_total = 0u32;
    let mut unique_a_traps = Vec::new();
    let mut trap_hits = Vec::new();
    let file_size = data.len() as u32;

    if is_resource {
        let mut types = Vec::new();
        let mut ids = Vec::new();
        let mut offsets = Vec::new();
        for i in 0..entry_count as usize {
            let off = 78 + i * 10;
            let kind = fourcc(&data[off..off + 4]);
            let id = be_u16(data, off + 4)?;
            let data_off = be_u32(data, off + 6)?;
            types.push(kind);
            ids.push(id);
            offsets.push(data_off.min(file_size));
        }
        for i in 0..types.len() {
            let size = section_sizes(&offsets, file_size, i);
            resources.push(PrcResourceEntry {
                kind: types[i].clone(),
                id: ids[i],
                offset: offsets[i],
                size,
            });
            push_section(&mut sections, &types[i], size);
            if types[i].eq_ignore_ascii_case("code") {
                code_bytes = code_bytes.saturating_add(size);
                let start = offsets[i] as usize;
                let end = start.saturating_add(size as usize).min(data.len());
                let scan = scan_68k_code(&data[start..end], ids[i], offsets[i], size, &mut trap_hits);
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
        for i in 0..entry_count as usize {
            let off = 78 + i * 8;
            let data_off = be_u32(data, off)?;
            offsets.push(data_off.min(file_size));
        }
        for i in 0..offsets.len() {
            let size = section_sizes(&offsets, file_size, i);
            resources.push(PrcResourceEntry {
                kind: "record".into(),
                id: i as u16,
                offset: offsets[i],
                size,
            });
            push_section(&mut sections, "record", size);
            other_bytes = other_bytes.saturating_add(size);
        }
    }

    Some(PrcInfo {
        db_name: clean_name(&data[0..32]),
        kind: if is_resource {
            PrcDbKind::Resource
        } else {
            PrcDbKind::Record
        },
        file_size,
        type_code: fourcc(&data[60..64]),
        creator_code: fourcc(&data[64..68]),
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
        trap_hits,
    })
}
