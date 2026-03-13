extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::palm::prc::parse_prc;

#[derive(Clone, Debug, Default)]
pub struct MenuItemPreview {
    pub id: u16,
    pub text: String,
    pub shortcut: Option<char>,
}

#[derive(Clone, Debug, Default)]
pub struct MenuPullDownPreview {
    pub resource_id: u16,
    pub title: String,
    pub items: Vec<MenuItemPreview>,
}

#[derive(Clone, Debug, Default)]
pub struct MenuBarPreview {
    pub resource_id: u16,
    pub menus: Vec<MenuPullDownPreview>,
}

fn read_u16_be(data: &[u8], off: usize) -> Option<u16> {
    let b0 = *data.get(off)?;
    let b1 = *data.get(off + 1)?;
    Some(u16::from_be_bytes([b0, b1]))
}

fn read_c_strings(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        while i < data.len() && data[i] == 0 {
            i += 1;
        }
        if i >= data.len() {
            break;
        }
        let start = i;
        while i < data.len() && data[i] != 0 {
            i += 1;
        }
        let bytes = &data[start..i];
        let printable = bytes
            .iter()
            .all(|b| b.is_ascii_graphic() || *b == b' ' || *b == b'/');
        if printable && !bytes.is_empty() {
            let text = String::from_utf8_lossy(bytes).trim().to_string();
            if !text.is_empty() {
                out.push(text);
            }
        }
    }
    out
}

fn read_string_at(data: &[u8], off: usize) -> Option<String> {
    if off >= data.len() {
        return None;
    }
    if !data[off].is_ascii_graphic() {
        return None;
    }
    let mut end = off;
    while end < data.len() && data[end] != 0 {
        if !data[end].is_ascii_graphic() && data[end] != b' ' {
            return None;
        }
        end += 1;
    }
    if end == off {
        return None;
    }
    Some(String::from_utf8_lossy(&data[off..end]).trim().to_string())
}

fn read_strings_from_word_offsets(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for off in (0..data.len().saturating_sub(1)).step_by(2) {
        let Some(ptr) = read_u16_be(data, off) else {
            continue;
        };
        let p = ptr as usize;
        if let Some(s) = read_string_at(data, p) {
            if !s.is_empty() && !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

fn parse_menu_resource(resource_id: u16, data: &[u8]) -> MenuPullDownPreview {
    let mut strings = read_strings_from_word_offsets(data);
    if strings.is_empty() {
        strings = read_c_strings(data);
    }
    let mut title = None;
    let mut item_texts = Vec::new();
    for s in strings {
        if s.len() <= 1 {
            continue;
        }
        if title.is_none() {
            title = Some(s);
        } else {
            item_texts.push(s);
        }
    }
    let title = title.unwrap_or_else(|| format!("Menu{}", resource_id));
    let title_lower = title.to_ascii_lowercase();
    item_texts.retain(|s| {
        let sl = s.to_ascii_lowercase();
        if sl == title_lower {
            return false;
        }
        if sl.ends_with(&title_lower) || title_lower.ends_with(&sl) {
            return false;
        }
        true
    });

    let mut ids = Vec::new();
    for off in (0..data.len().saturating_sub(1)).step_by(2) {
        let Some(v) = read_u16_be(data, off) else {
            continue;
        };
        if (1000..=0x7FFF).contains(&v) && v != resource_id && v != 0xFFFF && !ids.contains(&v) {
            ids.push(v);
        }
    }

    let mut items = Vec::new();
    for (idx, text) in item_texts.into_iter().enumerate() {
        let id = ids
            .get(idx)
            .copied()
            .unwrap_or_else(|| resource_id.saturating_mul(16).saturating_add(idx as u16 + 1));
        let shortcut = text
            .chars()
            .find(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase());
        items.push(MenuItemPreview { id, text, shortcut });
    }

    MenuPullDownPreview {
        resource_id,
        title,
        items,
    }
}

pub fn parse_menu_bar_preview(raw: &[u8]) -> Option<MenuBarPreview> {
    let info = parse_prc(raw)?;
    let mbar = info.resources.iter().find(|r| r.kind == "MBAR");

    let mut menu_ids = Vec::new();
    if let Some(mbar) = mbar {
        let start = mbar.offset as usize;
        let end = start.saturating_add(mbar.size as usize);
        let mbar_data = raw.get(start..end)?;
        for off in (0..mbar_data.len().saturating_sub(1)).step_by(2) {
            let Some(id) = read_u16_be(mbar_data, off) else {
                continue;
            };
            if info
                .resources
                .iter()
                .any(|r| (r.kind == "MENU" || r.kind == "tMEN") && r.id == id)
                && !menu_ids.contains(&id)
            {
                menu_ids.push(id);
            }
        }
    }
    if menu_ids.is_empty() {
        for r in &info.resources {
            if r.kind == "MENU" || r.kind == "tMEN" {
                menu_ids.push(r.id);
            }
        }
        menu_ids.sort_unstable();
    }
    if menu_ids.is_empty() {
        if let Some(mbar) = mbar {
            let start = mbar.offset as usize;
            let end = start.saturating_add(mbar.size as usize);
            let data = raw.get(start..end)?;
            let single = parse_menu_resource(mbar.id, data);
            if !single.items.is_empty() {
                return Some(MenuBarPreview {
                    resource_id: mbar.id,
                    menus: vec![single],
                });
            }
        }
        return None;
    }

    let mut menus = Vec::new();
    for id in menu_ids {
        let Some(res) = info
            .resources
            .iter()
            .find(|r| (r.kind == "MENU" || r.kind == "tMEN") && r.id == id)
        else {
            continue;
        };
        let rs = res.offset as usize;
        let re = rs.saturating_add(res.size as usize);
        let Some(data) = raw.get(rs..re) else {
            continue;
        };
        menus.push(parse_menu_resource(id, data));
    }
    if menus.is_empty() {
        return None;
    }
    Some(MenuBarPreview {
        resource_id: mbar.map(|m| m.id).unwrap_or(0),
        menus,
    })
}
