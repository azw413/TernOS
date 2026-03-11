extern crate alloc;

use alloc::vec::Vec;

use crate::prc_app::{cpu::core::CpuState68k, cpu::memory::MemoryMap, runtime};

pub fn parse_prc_resource_blobs(raw: &[u8]) -> Vec<runtime::ResourceBlob> {
    fn be_u16(data: &[u8], off: usize) -> Option<u16> {
        let bytes = data.get(off..off + 2)?;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }
    fn be_u32(data: &[u8], off: usize) -> Option<u32> {
        let bytes = data.get(off..off + 4)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    if raw.len() < 78 {
        return Vec::new();
    }
    let Some(attrs) = be_u16(raw, 32) else {
        return Vec::new();
    };
    if (attrs & 0x0001) == 0 {
        return Vec::new();
    }
    let Some(entry_count) = be_u16(raw, 76) else {
        return Vec::new();
    };
    let table_len = (entry_count as usize).saturating_mul(10);
    if 78 + table_len > raw.len() {
        return Vec::new();
    }

    let mut entries: Vec<(u32, u16, usize)> = Vec::new();
    for idx in 0..entry_count as usize {
        let off = 78 + idx * 10;
        let kind = match raw.get(off..off + 4) {
            Some(bytes) => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            None => continue,
        };
        let Some(id) = be_u16(raw, off + 4) else {
            continue;
        };
        let Some(start_u32) = be_u32(raw, off + 6) else {
            continue;
        };
        let start = start_u32 as usize;
        if start < raw.len() {
            entries.push((kind, id, start));
        }
    }
    entries.sort_by_key(|(_, _, start)| *start);

    let mut out = Vec::new();
    for idx in 0..entries.len() {
        let (kind, id, start) = entries[idx];
        let next_start = if idx + 1 < entries.len() {
            entries[idx + 1].2
        } else {
            raw.len()
        };
        let end = next_start.min(raw.len());
        if end <= start {
            continue;
        }
        let data = raw[start..end].to_vec();
        out.push(runtime::ResourceBlob { kind, id, data });
    }
    out
}

pub fn seed_prc_launch_registers(cpu: &mut CpuState68k, runtime: &runtime::PrcRuntimeContext) {
    // Probe-time launch contract: expose launch tuple in registers for apps
    // that read parameters directly in early startup glue.
    cpu.d[0] = runtime.launch_cmd as u32;
    cpu.d[1] = runtime.cmd_pbp;
    cpu.d[2] = runtime.launch_flags as u32;
    cpu.a[0] = runtime.cmd_pbp;
}

pub fn derive_prc_entry_from_code0(code0: &[u8], code1_len: u32) -> Option<u32> {
    fn fit_target(raw: u32, code1_len: u32) -> Option<u32> {
        if raw < code1_len {
            return Some(raw);
        }
        if (0x1000..0x1000 + code1_len).contains(&raw) {
            return Some(raw - 0x1000);
        }
        None
    }

    let mut i = 0usize;
    while i + 1 < code0.len() {
        let op = u16::from_be_bytes([code0[i], code0[i + 1]]);
        match op {
            0x4EF9 => {
                if i + 5 < code0.len() {
                    let raw =
                        u32::from_be_bytes([code0[i + 2], code0[i + 3], code0[i + 4], code0[i + 5]]);
                    if let Some(pc) = fit_target(raw, code1_len) {
                        return Some(pc);
                    }
                }
            }
            0x4EFA => {
                if i + 3 < code0.len() {
                    let disp = i16::from_be_bytes([code0[i + 2], code0[i + 3]]) as i32;
                    let base = (i as i32) + 4;
                    let target = base.saturating_add(disp);
                    if target >= 0 {
                        if let Some(pc) = fit_target(target as u32, code1_len) {
                            return Some(pc);
                        }
                    }
                }
            }
            _ => {}
        }
        i += 2;
    }
    None
}

pub fn derive_prc_entry_in_code1(code: &[u8]) -> u32 {
    if code.len() >= 8 {
        let head = u32::from_be_bytes([code[0], code[1], code[2], code[3]]);
        let w4 = u16::from_be_bytes([code[4], code[5]]);
        // Common Palm code#1 layout has a 4-byte header before function prologue.
        // ROM apps also use a small entry veneer starting with `PEA 4(PC)` (0x487A)
        // before the regular prologue.
        if (head == 0 || head == 1) && matches!(w4, 0x4E56 | 0x48E7 | 0x2F0E | 0x4E71 | 0x487A)
        {
            return 4;
        }
    }
    0
}

pub fn decode_data0_globals_into_memory(
    code0: &[u8],
    data0: &[u8],
    memory: &mut MemoryMap,
    data_start: u32,
    code_start: u32,
) -> Option<u32> {
    if code0.len() < 8 {
        return None;
    }
    let above_size = u32::from_be_bytes([code0[0], code0[1], code0[2], code0[3]]);
    let data_size = u32::from_be_bytes([code0[4], code0[5], code0[6], code0[7]]);
    let total = above_size.saturating_add(data_size);
    if total == 0 || data0.len() < 4 {
        return Some(data_size);
    }

    let mut i = 4usize;
    let data0_len = data0.len();

    let mut write_byte = |off: i32, value: u8| {
        if off < 0 {
            return;
        }
        let off_u = off as u32;
        if off_u >= total {
            return;
        }
        let _ = memory.write_u8(data_start.saturating_add(off_u), value);
    };

    // Decode three packed data chains (matches Pumpkin's emupalmos.c format).
    for _chain in 0..3 {
        if i + 4 > data0_len {
            return Some(data_size);
        }
        let offset =
            i32::from_be_bytes([data0[i], data0[i + 1], data0[i + 2], data0[i + 3]]);
        i += 4;
        let mut k = (data_size as i32).saturating_add(offset);
        loop {
            if i >= data0_len {
                return Some(data_size);
            }
            let b = data0[i];
            i += 1;
            if b == 0x00 {
                break;
            }
            if (b & 0x80) == 0x80 {
                let n = (b & 0x7F) as usize + 1;
                for _ in 0..n {
                    if i >= data0_len {
                        return Some(data_size);
                    }
                    write_byte(k, data0[i]);
                    i += 1;
                    k = k.saturating_add(1);
                }
            } else if (b & 0xC0) == 0x40 {
                let n = (b & 0x3F) as usize + 1;
                for _ in 0..n {
                    write_byte(k, 0x00);
                    k = k.saturating_add(1);
                }
            } else if (b & 0xE0) == 0x20 {
                let n = (b & 0x1F) as usize + 2;
                if i >= data0_len {
                    return Some(data_size);
                }
                let fill = data0[i];
                i += 1;
                for _ in 0..n {
                    write_byte(k, fill);
                    k = k.saturating_add(1);
                }
            } else if (b & 0xF0) == 0x10 {
                let n = (b & 0x0F) as usize + 1;
                for _ in 0..n {
                    write_byte(k, 0xFF);
                    k = k.saturating_add(1);
                }
            } else if b == 0x01 {
                let pat = [0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF];
                for v in pat {
                    write_byte(k, v);
                    k = k.saturating_add(1);
                }
                for _ in 0..3 {
                    if i >= data0_len {
                        return Some(data_size);
                    }
                    write_byte(k, data0[i]);
                    i += 1;
                    k = k.saturating_add(1);
                }
            } else if b == 0x02 {
                let pat = [0x00, 0x00, 0x00, 0x00, 0xFF];
                for v in pat {
                    write_byte(k, v);
                    k = k.saturating_add(1);
                }
                for _ in 0..3 {
                    if i >= data0_len {
                        return Some(data_size);
                    }
                    write_byte(k, data0[i]);
                    i += 1;
                    k = k.saturating_add(1);
                }
            } else if b == 0x03 {
                let pat = [0xA9, 0xF0, 0x00, 0x00];
                for v in pat {
                    write_byte(k, v);
                    k = k.saturating_add(1);
                }
                if i + 3 > data0_len {
                    return Some(data_size);
                }
                write_byte(k, data0[i]);
                k = k.saturating_add(1);
                write_byte(k, data0[i + 1]);
                k = k.saturating_add(1);
                write_byte(k, 0x00);
                k = k.saturating_add(1);
                write_byte(k, data0[i + 2]);
                k = k.saturating_add(1);
                i += 3;
            } else if b == 0x04 {
                let pat = [0xA9, 0xF0, 0x00];
                for v in pat {
                    write_byte(k, v);
                    k = k.saturating_add(1);
                }
                if i + 4 > data0_len {
                    return Some(data_size);
                }
                write_byte(k, data0[i]);
                k = k.saturating_add(1);
                write_byte(k, data0[i + 1]);
                k = k.saturating_add(1);
                write_byte(k, data0[i + 2]);
                k = k.saturating_add(1);
                write_byte(k, 0x00);
                k = k.saturating_add(1);
                write_byte(k, data0[i + 3]);
                k = k.saturating_add(1);
                i += 4;
            } else {
                return Some(data_size);
            }
        }
    }

    // Decode relocation xrefs (same format used by Pumpkin for 68k apps).
    if i < data0_len.saturating_sub(12) {
        for chain in 0..3u32 {
            if i + 4 > data0_len {
                break;
            }
            let count =
                u32::from_be_bytes([data0[i], data0[i + 1], data0[i + 2], data0[i + 3]]) as usize;
            i += 4;
            let mut offset = 0i32;
            let segment = data_start.saturating_add(data_size);
            let relocbase = match chain {
                0 => data_start.saturating_add(data_size),
                1 => code_start,
                _ => 0,
            };
            for _ in 0..count {
                if i >= data0_len {
                    break;
                }
                let b = data0[i];
                i += 1;
                if (b & 0x80) != 0 {
                    let d = (b as i8 as i32) << 1;
                    offset = offset.saturating_add(d);
                } else if (b & 0x40) != 0 {
                    if i >= data0_len {
                        break;
                    }
                    let b2 = data0[i];
                    i += 1;
                    let w = ((((b as i16) << 8) | (b2 as i16)) as i32) << 2 >> 1;
                    offset = offset.saturating_add(w);
                } else {
                    if i + 3 > data0_len {
                        break;
                    }
                    let l = (((b as i32) << 24)
                        | ((data0[i] as i32) << 16)
                        | ((data0[i + 1] as i32) << 8)
                        | (data0[i + 2] as i32))
                        << 2
                        >> 1;
                    i += 3;
                    offset = l;
                }
                let addr = segment.wrapping_add(offset as u32);
                if chain < 2 {
                    if let Some(value) = memory.read_u32_be(addr) {
                        let _ = memory.write_u32_be(addr, value.wrapping_add(relocbase));
                    }
                }
            }
        }
    }

    Some(data_size)
}
