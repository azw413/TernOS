extern crate alloc;

use alloc::{vec, vec::Vec};

use crate::prc_app::runtime::{PalmFont, ResourceBlob};

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
        widths,
        glyphs: vec![None; count],
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
