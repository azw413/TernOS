use crate::prc_app::cpu::core::{CpuState68k, StopReason};
use crate::prc_app::cpu::memory::MemoryMap;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn ea_ext_words(mode: u16, reg: u16) -> Option<u32> {
    match mode {
        0 | 1 | 2 | 3 | 4 => Some(0),
        5 | 6 => Some(1),
        7 => match reg {
            0 => Some(1),
            1 => Some(2),
            2 => Some(1),
            3 => Some(1),
            4 => Some(0),
            _ => None,
        },
        _ => None,
    }
}

fn set_ccr_nz(state: &mut CpuState68k, negative: bool, zero: bool) {
    state.sr &= !0x000F;
    if negative {
        state.sr |= 0x0008;
    }
    if zero {
        state.sr |= 0x0004;
    }
}

fn set_sr_z_only(state: &mut CpuState68k, zero: bool) {
    state.sr &= !0x0004;
    if zero {
        state.sr |= 0x0004;
    }
}

fn set_ccr_sub(state: &mut CpuState68k, src: u32, dst: u32, res: u32, bits: u32) {
    let sign = 1u32 << (bits - 1);
    let mask = if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    let s = src & mask;
    let d = dst & mask;
    let r = res & mask;
    let n = (r & sign) != 0;
    let z = r == 0;
    let v = ((d ^ s) & (d ^ r) & sign) != 0;
    let c = s > d;
    state.sr &= !0x000F;
    if n {
        state.sr |= 0x0008;
    }
    if z {
        state.sr |= 0x0004;
    }
    if v {
        state.sr |= 0x0002;
    }
    if c {
        state.sr |= 0x0001;
    }
}

fn execute_add_sub_class(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0xD000 && (word & 0xF000) != 0x9000 {
        return Ok(false);
    }
    // ADDX/SUBX are handled in xops to keep semantics isolated.
    if (word & 0xF130) == 0xD100 || (word & 0xF130) == 0x9100 {
        return Ok(false);
    }

    let mode = (word >> 3) & 0x0007;
    let reg = word & 0x0007;
    let ea_words = ea_ext_words(mode, reg).unwrap_or(0);
    let opmode = (word >> 6) & 0x0007;
    let dst = ((word >> 9) & 0x0007) as usize;

    // ADDA/SUBA minimal support for startup glue.
    if opmode == 0x3 || opmode == 0x7 {
        let src_long = opmode == 0x7;
        let src = match (mode, reg) {
            (0, r) => {
                let v = state.d[r as usize];
                if src_long {
                    Some(v)
                } else {
                    Some((v as u16 as i16) as i32 as u32)
                }
            }
            (1, r) => {
                let v = state.a[r as usize];
                if src_long {
                    Some(v)
                } else {
                    Some((v as u16 as i16) as i32 as u32)
                }
            }
            (7, 4) => {
                if src_long {
                    memory.read_u32_be(pc.saturating_add(2))
                } else {
                    memory
                        .read_u16_be(pc.saturating_add(2))
                        .map(|v| (v as i16 as i32) as u32)
                }
            }
            _ => None,
        };
        if let Some(src_v) = src {
            if (word & 0xF000) == 0xD000 {
                state.a[dst] = state.a[dst].wrapping_add(src_v);
            } else {
                state.a[dst] = state.a[dst].wrapping_sub(src_v);
            }
        }
        let ext_words = if mode == 7 && reg == 4 {
            if src_long { 2 } else { 1 }
        } else {
            ea_words
        };
        state.pc = pc.saturating_add(2 + ext_words * 2);
        return Ok(true);
    }

    // Minimal execution for ADD <Dn>,Dn (word/long destination in Dn).
    if (word & 0xF000) == 0xD000 && mode == 0 {
        let src = reg as usize;
        match opmode {
            0x0 => {
                let s = (state.d[src] & 0xFF) as u8;
                let d = (state.d[dst] & 0xFF) as u8;
                let r = d.wrapping_add(s);
                state.d[dst] = (state.d[dst] & 0xFFFF_FF00) | (r as u32);
                set_ccr_nz(state, (r & 0x80) != 0, r == 0);
            }
            0x1 => {
                let s = (state.d[src] & 0xFFFF) as u16;
                let d = (state.d[dst] & 0xFFFF) as u16;
                let r = d.wrapping_add(s);
                state.d[dst] = (state.d[dst] & 0xFFFF_0000) | (r as u32);
                set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
            }
            0x2 => {
                let r = state.d[dst].wrapping_add(state.d[src]);
                state.d[dst] = r;
                set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
            }
            0x3 => {
                let s = (state.d[src] & 0xFFFF) as u16;
                let s = (s as i16 as i32) as u32;
                state.a[dst] = state.a[dst].wrapping_add(s);
            }
            0x7 => {
                state.a[dst] = state.a[dst].wrapping_add(state.d[src]);
            }
            _ => {}
        }
    } else if (word & 0xF000) == 0xD000 && mode == 1 {
        let src_an = reg as usize;
        match opmode {
            0x1 => {
                let s = (state.a[src_an] & 0xFFFF) as u16;
                let d = (state.d[dst] & 0xFFFF) as u16;
                let r = d.wrapping_add(s);
                state.d[dst] = (state.d[dst] & 0xFFFF_0000) | (r as u32);
                set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
            }
            0x2 => {
                let r = state.d[dst].wrapping_add(state.a[src_an]);
                state.d[dst] = r;
                set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
            }
            _ => {}
        }
    } else if (word & 0xF000) == 0xD000 && mode == 2 {
        let an = reg as usize;
        let src = ((word >> 9) & 0x0007) as usize;
        let opmode = (word >> 6) & 0x0007;
        match opmode {
            0x4 => {
                if let Some(d) = memory.read_u8(state.a[an]) {
                    let s = (state.d[src] & 0xFF) as u8;
                    let r = d.wrapping_add(s);
                    memory.write_u8(state.a[an], r);
                    set_ccr_nz(state, (r & 0x80) != 0, r == 0);
                }
            }
            0x5 => {
                if let Some(d) = memory.read_u16_be(state.a[an]) {
                    let s = (state.d[src] & 0xFFFF) as u16;
                    let r = d.wrapping_add(s);
                    memory.write_u16_be(state.a[an], r);
                    set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
                }
            }
            0x6 => {
                if let Some(d) = memory.read_u32_be(state.a[an]) {
                    let r = d.wrapping_add(state.d[src]);
                    memory.write_u32_be(state.a[an], r);
                    set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
                }
            }
            _ => {}
        }
    }

    state.pc = pc.saturating_add(2 + ea_words * 2);
    Ok(true)
}

fn execute_dynamic_bit(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF100) != 0x0100 {
        return Ok(false);
    }

    let bit_src_dn = ((word >> 9) & 0x0007) as usize;
    let op_kind = (word >> 6) & 0x0003; // 0=BTST 1=BCHG 2=BCLR 3=BSET
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;

    match mode {
        0 => {
            let bit = (state.d[bit_src_dn] & 31) as u32;
            let mask = 1u32 << bit;
            let old = state.d[reg];
            set_sr_z_only(state, (old & mask) == 0);
            let new_val = match op_kind {
                0 => old,
                1 => old ^ mask,
                2 => old & !mask,
                _ => old | mask,
            };
            if op_kind != 0 {
                state.d[reg] = new_val;
            }
            state.pc = pc.saturating_add(2);
            Ok(true)
        }
        2 => {
            let bit = (state.d[bit_src_dn] & 7) as u8;
            let mask = 1u8 << bit;
            let addr = state.a[reg];
            let old = memory.read_u8(addr).unwrap_or(0);
            set_sr_z_only(state, (old & mask) == 0);
            let new_val = match op_kind {
                0 => old,
                1 => old ^ mask,
                2 => old & !mask,
                _ => old | mask,
            };
            if op_kind != 0 {
                memory.write_u8(addr, new_val);
            }
            state.pc = pc.saturating_add(2);
            Ok(true)
        }
        _ => {
            let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
            state.pc = pc.saturating_add(2 + ea_words * 2);
            Ok(true)
        }
    }
}

fn execute_cmp(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0xB000 {
        return Ok(false);
    }
    // EOR and CMPM (opmodes 4..6) are handled in logic/xops.
    let opmode = (word >> 6) & 0x0007;
    if (4..=6).contains(&opmode) {
        return Ok(false);
    }

    let dn = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;

    if opmode == 0x3 || opmode == 0x7 {
        let src_long = opmode == 0x7;
        let src = match (mode, reg) {
            (0, r) => {
                let v = state.d[r];
                if src_long {
                    Some(v)
                } else {
                    Some((v as u16 as i16) as i32 as u32)
                }
            }
            (1, r) => {
                let v = state.a[r];
                if src_long {
                    Some(v)
                } else {
                    Some((v as u16 as i16) as i32 as u32)
                }
            }
            _ => None,
        };
        if let Some(src_v) = src {
            let dst_v = state.a[dn];
            let res = dst_v.wrapping_sub(src_v);
            set_ccr_sub(state, src_v, dst_v, res, 32);
        }
        let ext_words = ea_ext_words(mode as u16, reg as u16).unwrap_or(0);
        state.pc = pc.saturating_add(2 + ext_words * 2);
        return Ok(true);
    }

    let size = match opmode {
        0 => Some(1u32),
        1 => Some(2u32),
        2 => Some(4u32),
        _ => None,
    };

    if let Some(size_bytes) = size {
        let ext_pc = pc.saturating_add(2);
        let src = match mode {
            0 => Some(state.d[reg]),
            1 => Some(state.a[reg]),
            2 => {
                let addr = state.a[reg];
                match size_bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    _ => memory.read_u32_be(addr),
                }
            }
            3 => {
                let addr = state.a[reg];
                let v = match size_bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    _ => memory.read_u32_be(addr),
                };
                let inc = if size_bytes == 1 && reg == 7 { 2 } else { size_bytes };
                state.a[reg] = state.a[reg].wrapping_add(inc);
                v
            }
            5 => {
                let disp_w = memory
                    .read_u16_be(ext_pc)
                    .ok_or(StopReason::OutOfBounds { pc })?;
                let disp = disp_w as i16 as i32;
                let addr = add_signed_u32(state.a[reg], disp);
                match size_bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    _ => memory.read_u32_be(addr),
                }
            }
            6 => {
                let ext = memory
                    .read_u16_be(ext_pc)
                    .ok_or(StopReason::OutOfBounds { pc })?;
                let da = (ext & 0x8000) != 0;
                let idx_reg = ((ext >> 12) & 0x0007) as usize;
                let idx_long = (ext & 0x0800) != 0;
                let disp8 = (ext & 0x00FF) as u8 as i8 as i32;
                let idx_raw = if da { state.a[idx_reg] } else { state.d[idx_reg] };
                let idx = if idx_long {
                    idx_raw as i32
                } else {
                    (idx_raw as u16 as i16) as i32
                };
                let addr = add_signed_u32(state.a[reg], disp8.saturating_add(idx));
                match size_bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    _ => memory.read_u32_be(addr),
                }
            }
            7 => match reg {
                0 => {
                    let aw_w = memory
                        .read_u16_be(ext_pc)
                        .ok_or(StopReason::OutOfBounds { pc })?;
                    let aw = aw_w as i16 as i32;
                    let addr = aw as u32;
                    match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    }
                }
                1 => {
                    let addr = memory
                        .read_u32_be(ext_pc)
                        .ok_or(StopReason::OutOfBounds { pc })?;
                    match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    }
                }
                2 => {
                    let disp_w = memory
                        .read_u16_be(ext_pc)
                        .ok_or(StopReason::OutOfBounds { pc })?;
                    let disp = disp_w as i16 as i32;
                    let addr = add_signed_u32(pc.saturating_add(2), disp);
                    match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    }
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(src_raw) = src {
            let (src_v, dst_v, bits) = match size_bytes {
                1 => (src_raw & 0xFF, state.d[dn] & 0xFF, 8u32),
                2 => (src_raw & 0xFFFF, state.d[dn] & 0xFFFF, 16u32),
                _ => (src_raw, state.d[dn], 32u32),
            };
            let res = dst_v.wrapping_sub(src_v);
            set_ccr_sub(state, src_v, dst_v, res, bits);
        }
        let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
        state.pc = pc.saturating_add(2 + ea_words * 2);
        return Ok(true);
    }

    Ok(false)
}

fn execute_lea(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF1C0) != 0x41C0 {
        return Ok(false);
    }

    let am = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;

    let indexed_addr = |base: u32, ext: u16, state: &CpuState68k| -> u32 {
        let da = (ext & 0x8000) != 0;
        let idx_reg = ((ext >> 12) & 0x0007) as usize;
        let idx_long = (ext & 0x0800) != 0;
        let disp8 = (ext & 0x00FF) as u8 as i8 as i32;
        let idx_raw = if da { state.a[idx_reg] } else { state.d[idx_reg] };
        let idx = if idx_long {
            idx_raw as i32
        } else {
            (idx_raw as u16 as i16) as i32
        };
        add_signed_u32(base, disp8.saturating_add(idx))
    };

    let (addr_opt, advance) = match (mode, reg) {
        (2, an) => (Some(state.a[an]), 2u32),
        (3, an) => {
            let a = state.a[an];
            state.a[an] = state.a[an].wrapping_add(4);
            (Some(a), 2u32)
        }
        (5, an) => {
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            (Some(add_signed_u32(state.a[an], (disp16 as i16) as i32)), 4u32)
        }
        (6, an) => {
            let Some(ext) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            (Some(indexed_addr(state.a[an], ext, state)), 4u32)
        }
        (7, 0) => {
            let Some(aw) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            (Some((aw as i16 as i32) as u32), 4u32)
        }
        (7, 1) => {
            let Some(al) = memory.read_u32_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            (Some(al), 6u32)
        }
        (7, 2) => {
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            (
                Some(add_signed_u32(pc.saturating_add(2), (disp16 as i16) as i32)),
                4u32,
            )
        }
        (7, 3) => {
            let Some(ext) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            (Some(indexed_addr(pc.saturating_add(2), ext, state)), 4u32)
        }
        _ => (None, 2u32),
    };

    if let Some(addr) = addr_opt {
        state.a[am] = addr;
    }
    state.pc = pc.saturating_add(advance);
    Ok(true)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if execute_add_sub_class(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_dynamic_bit(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_cmp(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_lea(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}
