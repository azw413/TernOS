extern crate alloc;

use alloc::vec::Vec;

use crate::prc_app::{cpu::core::CpuState68k, runtime};

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

    let mut out = Vec::new();
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
        if start >= raw.len() {
            continue;
        }
        let next_start = if idx + 1 < entry_count as usize {
            be_u32(raw, 78 + (idx + 1) * 10 + 6).unwrap_or(raw.len() as u32) as usize
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

    // Common Palm code#0 launcher header starts with a code#1 entry offset.
    if code0.len() >= 4 {
        let header_entry = u32::from_be_bytes([code0[0], code0[1], code0[2], code0[3]]);
        if let Some(pc) = fit_target(header_entry, code1_len) {
            if pc != 0 {
                return Some(pc);
            }
        }
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
        if (head == 0 || head == 1) && matches!(w4, 0x4E56 | 0x48E7 | 0x2F0E | 0x4E71) {
            return 4;
        }
    }
    0
}
