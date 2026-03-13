extern crate alloc;

use alloc::{vec, vec::Vec};

use crate::palm::prc::parse_prc;
use crate::palm::runtime;

#[derive(Clone, Debug)]
pub struct PrcBitmap {
    pub resource_id: u16,
    pub width: u16,
    pub height: u16,
    pub row_bytes: u16,
    pub bits: Vec<u8>,
}

fn read_u16_be(data: &[u8], off: usize) -> Option<u16> {
    let b0 = *data.get(off)?;
    let b1 = *data.get(off + 1)?;
    Some(u16::from_be_bytes([b0, b1]))
}

fn parse_bitmap_blob(resource_id: u16, data: &[u8]) -> Option<PrcBitmap> {
    if data.len() < 16 {
        return None;
    }
    let width = read_u16_be(data, 0)?;
    let height = read_u16_be(data, 2)?;
    let row_bytes_raw = read_u16_be(data, 4)?;
    let flags = read_u16_be(data, 6).unwrap_or(0);
    let row_bytes = row_bytes_raw & 0x3FFF;
    let pixel_size = *data.get(8)?;
    let version = *data.get(9)?;
    let compressed = (flags & 0x8000) != 0;
    let compression = if compressed {
        if version <= 1 {
            0 // BitmapCompressionTypeScanLine
        } else {
            *data.get(13).unwrap_or(&0)
        }
    } else {
        0xFF // BitmapCompressionTypeNone
    };
    if width == 0 || height == 0 || row_bytes == 0 {
        return None;
    }
    if pixel_size != 1 {
        return None;
    }
    let bits_len = row_bytes as usize * height as usize;
    // BitmapType v0-v2 uses a 16-byte header. v3 typically uses 24 bytes.
    // Older code treated byte[10] as header size, but that's not reliable and
    // causes valid v3 bitmaps to be skipped.
    let header_size = if version >= 3 { 24usize } else { 16usize };
    let bits = if !compressed {
        data.get(header_size..header_size.saturating_add(bits_len))
            .or_else(|| data.get(16..16usize.saturating_add(bits_len)))?
            .to_vec()
    } else {
        decompress_bitmap_blob(data, version, compression, row_bytes, width, height)?
    };
    Some(PrcBitmap {
        resource_id,
        width,
        height,
        row_bytes,
        bits,
    })
}

fn decompress_bitmap_blob(
    data: &[u8],
    version: u8,
    compression: u8,
    row_bytes: u16,
    _width: u16,
    height: u16,
) -> Option<Vec<u8>> {
    let expected_len = row_bytes as usize * height as usize;
    if expected_len == 0 {
        return None;
    }
    let header_size = if version >= 3 { 24usize } else { 16usize };
    let mut src = data.get(header_size..)?;
    // PalmOS compressed payloads include a size prefix for legacy versions.
    src = match version {
        0 | 1 | 2 => src.get(2..)?,
        3 => src.get(4..)?,
        _ => src,
    };
    let mut out = vec![0u8; expected_len];
    match compression {
        // BitmapCompressionTypeScanLine
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
            Some(out)
        }
        // BitmapCompressionTypeRLE
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
            Some(out)
        }
        // BitmapCompressionTypePackBits (8-bit stream; still valid for 1bpp bytes)
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
                } else {
                    // -128: no-op
                }
            }
            Some(out)
        }
        _ => None,
    }
}

pub fn parse_prc_bitmaps(raw: &[u8]) -> Vec<PrcBitmap> {
    let Some(info) = parse_prc(raw) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for res in info.resources.iter() {
        if res.kind != "Tbmp" && res.kind != "tAIB" {
            continue;
        }
        let start = res.offset as usize;
        let end = start.saturating_add(res.size as usize);
        let Some(blob) = raw.get(start..end) else {
            continue;
        };
        if let Some(parsed) = parse_bitmap_blob(res.id, blob) {
            out.push(parsed);
        }
    }
    out
}

pub fn parse_prc_bitmaps_from_resource_blobs(resources: &[runtime::ResourceBlob]) -> Vec<PrcBitmap> {
    let mut out = Vec::new();
    for res in resources {
        if res.kind != u32::from_be_bytes(*b"Tbmp") && res.kind != u32::from_be_bytes(*b"tAIB") {
            continue;
        }
        if let Some(parsed) = parse_bitmap_blob(res.id, &res.data) {
            out.push(parsed);
        }
    }
    out
}
