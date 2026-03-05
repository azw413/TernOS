extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use crate::prc_app::prc::{PrcResourceEntry, parse_prc};

#[derive(Clone, Debug)]
pub enum FormPreviewObject {
    Label {
        x: i16,
        y: i16,
        font: u8,
        text: String,
    },
    Button {
        x: i16,
        y: i16,
        w: i16,
        h: i16,
        font: u8,
        text: String,
    },
    Bitmap {
        x: i16,
        y: i16,
        resource_id: u16,
    },
}

#[derive(Clone, Debug, Default)]
pub struct FormPreview {
    pub resource_id: u16,
    pub form_id: u16,
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
    pub object_count: u16,
    pub objects: Vec<FormPreviewObject>,
}

fn read_u16_be(data: &[u8], off: usize) -> Option<u16> {
    let b0 = *data.get(off)?;
    let b1 = *data.get(off + 1)?;
    Some(u16::from_be_bytes([b0, b1]))
}

fn read_i16_be(data: &[u8], off: usize) -> Option<i16> {
    read_u16_be(data, off).map(|v| v as i16)
}

fn read_u32_be(data: &[u8], off: usize) -> Option<u32> {
    let b0 = *data.get(off)?;
    let b1 = *data.get(off + 1)?;
    let b2 = *data.get(off + 2)?;
    let b3 = *data.get(off + 3)?;
    Some(u32::from_be_bytes([b0, b1, b2, b3]))
}

fn read_c_string(data: &[u8], off: usize) -> String {
    if off >= data.len() {
        return String::new();
    }
    let mut end = off;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    String::from_utf8_lossy(&data[off..end]).into_owned()
}

fn parse_tlbl(data: &[u8]) -> Option<FormPreviewObject> {
    // tLBL: id, left, top, usable, pad, font, cstring
    if data.len() < 10 {
        return None;
    }
    let x = read_i16_be(data, 2)?;
    let y = read_i16_be(data, 4)?;
    let font = *data.get(8).unwrap_or(&0);
    let text = read_c_string(data, 9);
    Some(FormPreviewObject::Label { x, y, font, text })
}

fn parse_text_button(data: &[u8], text_off: usize, font_off: usize) -> Option<FormPreviewObject> {
    if data.len() <= text_off {
        return None;
    }
    let x = read_i16_be(data, 2)?;
    let y = read_i16_be(data, 4)?;
    let w = read_i16_be(data, 6)?;
    let h = read_i16_be(data, 8)?;
    let font = *data.get(font_off).unwrap_or(&0);
    let text = read_c_string(data, text_off);
    Some(FormPreviewObject::Button {
        x,
        y,
        w,
        h,
        font,
        text,
    })
}

fn parse_form_object(kind: &str, data: &[u8]) -> Option<FormPreviewObject> {
    match kind {
        "tLBL" => parse_tlbl(data),
        "tBTN" => parse_text_button(data, 19, 16),
        "tPBN" => parse_text_button(data, 14, 11),
        "tPUT" => parse_text_button(data, 14, 11),
        "tCBX" => parse_text_button(data, 17, 14),
        _ => None,
    }
}

fn is_known_form_object_kind(kind: &str) -> bool {
    matches!(kind, "tLBL" | "tBTN" | "tPBN" | "tPUT" | "tCBX")
}

fn collect_objects_from_refs(
    refs: &[(u16, String)],
    resources: &[PrcResourceEntry],
    raw: &[u8],
) -> Vec<FormPreviewObject> {
    let mut out = Vec::new();
    for (obj_id, kind) in refs {
        if !is_known_form_object_kind(kind) {
            continue;
        }
        let obj_data = resources
            .iter()
            .find(|r| r.kind == *kind && r.id == *obj_id)
            .and_then(|r| {
                let rs = r.offset as usize;
                let re = rs.saturating_add(r.size as usize);
                raw.get(rs..re)
            });
        if let Some(obj_data) = obj_data {
            if let Some(obj) = parse_form_object(kind, obj_data) {
                out.push(obj);
            }
        }
    }
    out
}

fn heuristic_object_refs(form_data: &[u8]) -> Vec<(u16, String)> {
    let mut refs = Vec::new();
    if form_data.len() < 6 {
        return refs;
    }
    // Heuristic: look for packed [id:u16][kind:4cc] tuples anywhere in tFRM payload.
    for off in 0..(form_data.len() - 5) {
        let kind_bytes = &form_data[off + 2..off + 6];
        let kind = String::from_utf8_lossy(kind_bytes).into_owned();
        if !is_known_form_object_kind(&kind) {
            continue;
        }
        let Some(obj_id) = read_u16_be(form_data, off) else {
            continue;
        };
        if obj_id == 0 {
            continue;
        }
        if !refs.iter().any(|(id, k)| *id == obj_id && *k == kind) {
            refs.push((obj_id, kind));
        }
    }
    refs
}

fn align2(off: usize) -> usize {
    if off & 1 == 0 { off } else { off + 1 }
}

fn parse_packed_label(form_data: &[u8], off: usize) -> Option<FormPreviewObject> {
    // PumpkinOS: szRCFormLabelBA16 "w,w2,uzu15,b,zb,p"
    if off + 14 > form_data.len() {
        return None;
    }
    let x = read_i16_be(form_data, off + 2)?;
    let y = read_i16_be(form_data, off + 4)?;
    let font = *form_data.get(off + 8).unwrap_or(&0);
    let text = read_c_string(form_data, off + 14);
    Some(FormPreviewObject::Label { x, y, font, text })
}

fn parse_packed_title(form_data: &[u8], off: usize) -> Option<FormPreviewObject> {
    // PumpkinOS: szRCFORMTITLE "w4,p"
    if off + 12 > form_data.len() {
        return None;
    }
    let x = read_i16_be(form_data, off)?;
    let y = read_i16_be(form_data, off + 2)?;
    let text = read_c_string(form_data, off + 12);
    Some(FormPreviewObject::Label {
        x,
        y,
        font: 1,
        text,
    })
}

fn parse_packed_control(form_data: &[u8], off: usize) -> Option<FormPreviewObject> {
    // PumpkinOS: szRCControlBA16 / szRCSliderControlBA16.
    if off + 17 > form_data.len() {
        return None;
    }
    let x = read_i16_be(form_data, off + 2)?;
    let y = read_i16_be(form_data, off + 4)?;
    let w = read_i16_be(form_data, off + 6)?;
    let h = read_i16_be(form_data, off + 8)?;
    let attr = read_u16_be(form_data, off + 14)?;
    let style = *form_data.get(off + 16)?;

    let mut i = off + 17;
    let (font, text) = if style == 6 || style == 7 {
        // sliderCtl / feedbackSliderCtl
        (0, String::new())
    } else {
        if i + 3 > form_data.len() {
            return None;
        }
        let font = *form_data.get(i).unwrap_or(&0);
        i += 3; // font, group, reserved
        if attr & 0x0040 != 0 {
            // graphical control has no title text in the packed payload
            (font, String::new())
        } else {
            let s = read_c_string(form_data, i);
            let next = align2(i + s.len() + 1);
            if next > form_data.len() {
                return None;
            }
            (font, s)
        }
    };

    Some(FormPreviewObject::Button {
        x,
        y,
        w,
        h,
        font,
        text,
    })
}

fn parse_packed_bitmap(form_data: &[u8], off: usize) -> Option<FormPreviewObject> {
    // PumpkinOS: szRCFormBitMapBA16 "uzu15,w2,w"
    if off + 8 > form_data.len() {
        return None;
    }
    let x = read_i16_be(form_data, off + 2)?;
    let y = read_i16_be(form_data, off + 4)?;
    let resource_id = read_u16_be(form_data, off + 6)?;
    Some(FormPreviewObject::Bitmap {
        x,
        y,
        resource_id,
    })
}

fn parse_packed_form(resource_id: u16, form_data: &[u8]) -> Option<FormPreview> {
    // PumpkinOS parser layout:
    // RCWindow (40 bytes) + RCForm (28 bytes) + object table (6 bytes each).
    if form_data.len() < 68 {
        return None;
    }

    let x = read_i16_be(form_data, 10)?;
    let y = read_i16_be(form_data, 12)?;
    let w = read_i16_be(form_data, 14)?;
    let h = read_i16_be(form_data, 16)?;
    let form_id = read_u16_be(form_data, 40).unwrap_or(resource_id);
    let object_count = read_u16_be(form_data, 62)?;
    let table_off = 68usize;
    let table_len = (object_count as usize).saturating_mul(6);
    if table_off + table_len > form_data.len() {
        return None;
    }
    if w <= 0 || h <= 0 || w > 4096 || h > 4096 {
        return None;
    }

    let mut objects = Vec::new();
    for j in 0..(object_count as usize) {
        let eoff = table_off + j * 6;
        let Some(object_type) = form_data.get(eoff).copied() else {
            break;
        };
        let Some(obj_off_u32) = read_u32_be(form_data, eoff + 2) else {
            continue;
        };
        let obj_off = obj_off_u32 as usize;
        if obj_off >= form_data.len() {
            continue;
        }
        let obj = match object_type {
            1 => parse_packed_control(form_data, obj_off), // frmControlObj
            4 => parse_packed_bitmap(form_data, obj_off),  // frmBitmapObj
            8 => parse_packed_label(form_data, obj_off),   // frmLabelObj
            9 => parse_packed_title(form_data, obj_off),   // frmTitleObj
            _ => None,
        };
        if let Some(obj) = obj {
            objects.push(obj);
        }
    }

    Some(FormPreview {
        resource_id,
        form_id,
        x,
        y,
        w,
        h,
        object_count,
        objects,
    })
}

fn parse_single_form(
    resource_id: u16,
    form_data: &[u8],
    resources: &[PrcResourceEntry],
    raw: &[u8],
) -> Option<FormPreview> {
    // Prefer the packed on-resource object table format (as parsed by PumpkinOS).
    if let Some(form) = parse_packed_form(resource_id, form_data) {
        return Some(form);
    }

    fn parse_objects(
        form_data: &[u8],
        resources: &[PrcResourceEntry],
        raw: &[u8],
        mut off: usize,
        object_count: u16,
    ) -> Vec<FormPreviewObject> {
        let mut refs: Vec<(u16, String)> = Vec::new();
        for _ in 0..object_count {
            if off + 6 > form_data.len() {
                break;
            }
            let obj_id = read_u16_be(form_data, off).unwrap_or(0);
            let kind_bytes = &form_data[off + 2..off + 6];
            let kind = String::from_utf8_lossy(kind_bytes).into_owned();
            if obj_id != 0 && is_known_form_object_kind(&kind) {
                refs.push((obj_id, kind));
            }
            off += 6;
        }
        let mut objects = collect_objects_from_refs(&refs, resources, raw);
        if objects.is_empty() {
            let h = heuristic_object_refs(form_data);
            objects = collect_objects_from_refs(&h, resources, raw);
        }
        objects
    }

    // Preferred layout (Palm UIResDefs.r).
    let mut parsed: Option<(u16, i16, i16, i16, i16, u16, usize)> = None;
    if form_data.len() >= 32 {
        let x = read_i16_be(form_data, 0).unwrap_or(0);
        let y = read_i16_be(form_data, 2).unwrap_or(0);
        let w = read_i16_be(form_data, 4).unwrap_or(0);
        let h = read_i16_be(form_data, 6).unwrap_or(0);
        let form_id = read_u16_be(form_data, 18).unwrap_or(resource_id);
        let object_count = read_u16_be(form_data, 30).unwrap_or(0);
        if w > 0 && h > 0 && w <= 4096 && h <= 4096 {
            parsed = Some((form_id, x, y, w, h, object_count.min(1024), 32));
        }
    }
    // Fallback layout seen in some resources/toolchains.
    if parsed.is_none() && form_data.len() >= 14 {
        let form_id = read_u16_be(form_data, 0).unwrap_or(resource_id);
        let x = read_i16_be(form_data, 4).unwrap_or(0);
        let y = read_i16_be(form_data, 6).unwrap_or(0);
        let w = read_i16_be(form_data, 8).unwrap_or(0);
        let h = read_i16_be(form_data, 10).unwrap_or(0);
        let object_count = read_u16_be(form_data, 12).unwrap_or(0);
        if w > 0 && h > 0 && w <= 4096 && h <= 4096 {
            parsed = Some((form_id, x, y, w, h, object_count.min(1024), 14));
        }
    }

    let (form_id, x, y, w, h, object_count, object_off) = parsed?;
    let sane = (-256..=4096).contains(&x) && (-256..=4096).contains(&y);
    if !sane {
        return None;
    }

    let objects = parse_objects(form_data, resources, raw, object_off, object_count);

    Some(FormPreview {
        resource_id,
        form_id,
        x,
        y,
        w,
        h,
        object_count,
        objects,
    })
}

pub fn parse_form_previews(raw: &[u8]) -> Vec<FormPreview> {
    let mut out = Vec::new();
    let Some(info) = parse_prc(raw) else {
        return out;
    };
    for res in &info.resources {
        if res.kind != "tFRM" {
            continue;
        }
        let start = res.offset as usize;
        let end = start.saturating_add(res.size as usize);
        let Some(form_bytes) = raw.get(start..end) else {
            continue;
        };
        if let Some(form) = parse_single_form(res.id, form_bytes, &info.resources, raw) {
            out.push(form);
        } else {
            // Keep a placeholder so UI can still show that tFRM exists.
            out.push(FormPreview {
                resource_id: res.id,
                form_id: res.id,
                x: 0,
                y: 0,
                w: 160,
                h: 160,
                object_count: 0,
                objects: Vec::new(),
            });
        }
    }
    out
}
