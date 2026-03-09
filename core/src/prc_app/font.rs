extern crate alloc;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use crate::prc_app::runtime::{PalmFont, PalmGlyphBitmap, PalmGlyphRows, PalmGlyphs, PalmWidths, ResourceBlob};

fn be_u16(data: &[u8], off: usize) -> Option<u16> {
    let b0 = *data.get(off)?;
    let b1 = *data.get(off + 1)?;
    Some(u16::from_be_bytes([b0, b1]))
}

fn parse_nfnt_font(font_id: u16, data: &[u8]) -> Option<PalmFont> {
    // Palm FontType v1 (NFNT).
    // Based on PumpkinOS pumpkin_create_font parser.
    if data.len() < 26 {
        return None;
    }
    let first_char = be_u16(data, 2)? as u8;
    let last_char = be_u16(data, 4)? as u8;
    if last_char < first_char {
        return None;
    }
    let max_width = (be_u16(data, 6)? & 0x00FF) as u8;
    let _rect_width = (be_u16(data, 12)? & 0x00FF) as u8;
    let rect_height = (be_u16(data, 14)? & 0x00FF) as u8;
    let row_words = be_u16(data, 24)? as usize;
    let glyph_len = row_words.saturating_mul(2).saturating_mul(rect_height as usize);
    let mut i = 26usize.saturating_add(glyph_len);
    if i > data.len() {
        return None;
    }
    let count = (last_char - first_char) as usize + 1;
    let column_table_len = count.saturating_mul(2);
    i = i.saturating_add(column_table_len);
    if i > data.len() {
        return None;
    }
    let mut widths = Vec::with_capacity(count);
    for _ in 0..count {
        let _offset = *data.get(i)?;
        let width = *data.get(i + 1)?;
        widths.push(width);
        i += 2;
    }
    if widths.is_empty() {
        return None;
    }
    let sum: u32 = widths.iter().map(|w| *w as u32).sum();
    let avg_width = (sum / widths.len() as u32) as u8;

    Some(PalmFont {
        font_id,
        first_char,
        last_char,
        max_width,
        avg_width: avg_width.max(1),
        rect_height: rect_height.max(1),
        widths: PalmWidths::Owned(widths),
        glyphs: PalmGlyphs::Owned(vec![None; count]),
    })
}

pub fn load_nfnt_fonts(resources: &[ResourceBlob]) -> Vec<PalmFont> {
    // Palm built-ins are commonly NFNT 9100+fontId.
    let mut out = Vec::new();
    let nfnt = u32::from_be_bytes(*b"NFNT");
    for res in resources {
        if res.kind != nfnt {
            continue;
        }
        let font_id = res.id.saturating_sub(9100);
        if let Some(font) = parse_nfnt_font(font_id, &res.data) {
            out.push(font);
        }
    }
    out
}

pub fn parse_font_resource_id_from_name(file_name: &str) -> Option<u16> {
    let mut nums: Vec<u16> = Vec::new();
    let mut cur = String::new();
    for ch in file_name.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<u16>() {
                nums.push(v);
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        if let Ok(v) = cur.parse::<u16>() {
            nums.push(v);
        }
    }
    for v in nums {
        if (9000..=9099).contains(&v) {
            return Some(v.saturating_add(100));
        }
        if (9100..=9999).contains(&v) {
            return Some(v);
        }
        if v <= 255 {
            return Some(9100u16.saturating_add(v));
        }
    }
    None
}

pub fn is_prc_font_resource_blob_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".nfnt")
        || lower.ends_with(".fnt")
        || lower.ends_with(".bin")
        || lower.ends_with(".dat")
}

pub fn parse_pumpkin_txt_font(text: &str, font_id: u16) -> Option<PalmFont> {
    let mut ascent: u8 = 0;
    let mut descent: u8 = 0;
    let mut glyphs: BTreeMap<u8, (u8, Vec<u16>)> = BTreeMap::new();

    // Parse incrementally to avoid collecting all lines into a large Vec on
    // constrained targets.
    let mut lines = text.lines().peekable();
    while let Some(raw_line) = lines.next() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("ascent ") {
            if let Ok(v) = rest.trim().parse::<u8>() {
                ascent = v;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("descent ") {
            if let Ok(v) = rest.trim().parse::<u8>() {
                descent = v;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("GLYPH ") {
            let Ok(code_u16) = rest.trim().parse::<u16>() else {
                continue;
            };
            if code_u16 > 255 {
                continue;
            }
            let code = code_u16 as u8;
            let mut rows: Vec<&str> = Vec::new();
            while let Some(row) = lines.peek().copied() {
                let row_trim = row.trim();
                if row_trim.is_empty() || row_trim.starts_with("GLYPH ") {
                    break;
                }
                rows.push(row);
                let _ = lines.next();
            }
            let mut width = 0usize;
            let mut row_bits: Vec<u16> = Vec::new();
            for row in &rows {
                let bytes = row.as_bytes();
                let mut last_hash: Option<usize> = None;
                let mut bits = 0u16;
                for (idx, b) in bytes.iter().enumerate() {
                    if *b == b'#' {
                        last_hash = Some(idx);
                        if idx < 16 {
                            bits |= 1u16 << idx;
                        }
                    }
                }
                let w = if let Some(last) = last_hash {
                    last + 1
                } else {
                    bytes.len()
                };
                width = width.max(w);
                row_bits.push(bits);
            }
            glyphs.insert(code, (width.min(255) as u8, row_bits));
            continue;
        }
    }

    if glyphs.is_empty() {
        return None;
    }
    let first_char = *glyphs.keys().next()?;
    let last_char = *glyphs.keys().next_back()?;
    let mut widths = vec![0u8; (last_char - first_char) as usize + 1];
    let mut bitmaps = vec![None; widths.len()];
    for (ch, (w, rows)) in glyphs {
        let idx = (ch - first_char) as usize;
        widths[idx] = w.max(1);
        bitmaps[idx] = Some(PalmGlyphBitmap {
            width: w.max(1),
            rows: PalmGlyphRows::Owned(rows),
        });
    }
    let max_width = widths.iter().copied().max().unwrap_or(1).max(1);
    let avg_width = {
        let sum: u32 = widths.iter().map(|w| *w as u32).sum();
        ((sum / widths.len().max(1) as u32) as u8).max(1)
    };
    let rect_height = ascent.saturating_add(descent).max(1);

    Some(PalmFont {
        font_id,
        first_char,
        last_char,
        max_width,
        avg_width,
        rect_height,
        widths: PalmWidths::Owned(widths),
        glyphs: PalmGlyphs::Owned(bitmaps),
    })
}
