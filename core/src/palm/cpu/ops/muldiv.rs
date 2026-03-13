use crate::palm::cpu::{core::CpuState68k, memory::MemoryMap};
use crate::palm::cpu::core::StopReason;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn ea_ext_words(mode: u16, reg: u16) -> Option<u32> {
    match mode {
        0 | 1 | 2 | 3 | 4 => Some(0), // Dn/An/(An)/(An)+/-(An)
        5 | 6 => Some(1),             // d16(An) / d8(An,Xn)
        7 => match reg {
            0 => Some(1), // abs.w
            1 => Some(2), // abs.l
            2 => Some(1), // d16(PC)
            3 => Some(1), // d8(PC,Xn)
            4 => Some(0), // #imm (not valid as dest, but keep parser moving)
            _ => None,
        },
        _ => None,
    }
}

fn read_ea_value(
    state: &mut CpuState68k,
    memory: &MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
    bytes: u32,
    pc_base: u32,
) -> Option<u32> {
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

    match mode {
        0 => Some(state.d[reg]),
        1 => Some(state.a[reg]),
        2 => {
            let addr = state.a[reg];
            match bytes {
                1 => memory.read_u8(addr).map(u32::from),
                2 => memory.read_u16_be(addr).map(u32::from),
                4 => memory.read_u32_be(addr),
                _ => None,
            }
        }
        3 => {
            let addr = state.a[reg];
            let v = match bytes {
                1 => memory.read_u8(addr).map(u32::from),
                2 => memory.read_u16_be(addr).map(u32::from),
                4 => memory.read_u32_be(addr),
                _ => None,
            };
            let inc = if bytes == 1 && reg == 7 { 2 } else { bytes };
            state.a[reg] = state.a[reg].wrapping_add(inc);
            v
        }
        5 => {
            let disp = memory.read_u16_be(*ext_pc)? as i16 as i32;
            *ext_pc = ext_pc.saturating_add(2);
            let addr = add_signed_u32(state.a[reg], disp);
            match bytes {
                1 => memory.read_u8(addr).map(u32::from),
                2 => memory.read_u16_be(addr).map(u32::from),
                4 => memory.read_u32_be(addr),
                _ => None,
            }
        }
        6 => {
            let ext = memory.read_u16_be(*ext_pc)?;
            *ext_pc = ext_pc.saturating_add(2);
            let addr = indexed_addr(state.a[reg], ext, state);
            match bytes {
                1 => memory.read_u8(addr).map(u32::from),
                2 => memory.read_u16_be(addr).map(u32::from),
                4 => memory.read_u32_be(addr),
                _ => None,
            }
        }
        7 => match reg {
            0 => {
                let aw = memory.read_u16_be(*ext_pc)? as i16 as i32;
                *ext_pc = ext_pc.saturating_add(2);
                let addr = aw as u32;
                match bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    4 => memory.read_u32_be(addr),
                    _ => None,
                }
            }
            1 => {
                let addr = memory.read_u32_be(*ext_pc)?;
                *ext_pc = ext_pc.saturating_add(4);
                match bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    4 => memory.read_u32_be(addr),
                    _ => None,
                }
            }
            2 => {
                let disp = memory.read_u16_be(*ext_pc)? as i16 as i32;
                *ext_pc = ext_pc.saturating_add(2);
                let addr = add_signed_u32(pc_base, disp);
                match bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    4 => memory.read_u32_be(addr),
                    _ => None,
                }
            }
            3 => {
                let ext = memory.read_u16_be(*ext_pc)?;
                *ext_pc = ext_pc.saturating_add(2);
                let addr = indexed_addr(pc_base, ext, state);
                match bytes {
                    1 => memory.read_u8(addr).map(u32::from),
                    2 => memory.read_u16_be(addr).map(u32::from),
                    4 => memory.read_u32_be(addr),
                    _ => None,
                }
            }
            4 => {
                let v = match bytes {
                    1 | 2 => memory.read_u16_be(*ext_pc).map(u32::from),
                    4 => memory.read_u32_be(*ext_pc),
                    _ => None,
                };
                *ext_pc = ext_pc.saturating_add(if bytes == 4 { 4 } else { 2 });
                v
            }
            _ => None,
        },
        _ => None,
    }
}

fn execute_div(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // Distinguish DIVU (opmode 0b011) vs DIVS (opmode 0b111) via bit 8.
    let divu = (word & 0xF1C0) == 0x80C0;
    let divs = (word & 0xF1C0) == 0x81C0;
    if !(divu || divs) {
        return Ok(false);
    }

    let dn = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let divisor_opt: Option<u16> = match mode {
        0 => Some((state.d[reg] & 0xFFFF) as u16), // Dn
        1 => Some((state.a[reg] & 0xFFFF) as u16), // An
        2 => memory.read_u16_be(state.a[reg]),      // (An)
        _ => None,
    };
    let Some(divisor) = divisor_opt else {
        return Err(StopReason::UnknownOpcode { pc, word });
    };
    if divisor == 0 {
        return Err(StopReason::UnknownOpcode { pc, word });
    }

    if divu {
        let dividend = state.d[dn];
        let q = dividend / (divisor as u32);
        let r = dividend % (divisor as u32);
        if q > 0xFFFF {
            // Overflow: V set, C clear; destination remains unchanged.
            state.sr |= 0x0002;
            state.sr &= !0x0001;
        } else {
            state.d[dn] = ((r & 0xFFFF) << 16) | (q & 0xFFFF);
            state.sr &= !0x000F;
            if (q & 0x8000) != 0 {
                state.sr |= 0x0008;
            }
            if (q & 0xFFFF) == 0 {
                state.sr |= 0x0004;
            }
        }
    } else {
        let divisor_s = divisor as i16 as i32;
        let dividend_s = state.d[dn] as i32;
        let q = dividend_s / divisor_s;
        let r = dividend_s % divisor_s;
        if !(-32768..=32767).contains(&q) {
            // Overflow: V set, C clear; destination remains unchanged.
            state.sr |= 0x0002;
            state.sr &= !0x0001;
        } else {
            let q16 = (q as i16 as u16) as u32;
            let r16 = (r as i16 as u16) as u32;
            state.d[dn] = (r16 << 16) | q16;
            state.sr &= !0x000F;
            if (q16 & 0x8000) != 0 {
                state.sr |= 0x0008;
            }
            if (q16 & 0xFFFF) == 0 {
                state.sr |= 0x0004;
            }
        }
    }
    let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
    state.pc = pc.saturating_add(2 + ea_words * 2);
    Ok(true)
}

fn execute_mul(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let mulu = (word & 0xF1C0) == 0xC0C0;
    let muls = (word & 0xF1C0) == 0xC1C0;
    if !(mulu || muls) {
        return Ok(false);
    }
    let dn = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let mut ext_pc = pc.saturating_add(2);
    let Some(src_word) =
        read_ea_value(state, memory, mode, reg, &mut ext_pc, 2, pc.saturating_add(2))
    else {
        return Err(StopReason::UnknownOpcode { pc, word });
    };
    let result = if mulu {
        (state.d[dn] & 0xFFFF).wrapping_mul(src_word & 0xFFFF)
    } else {
        let lhs = (state.d[dn] as u16 as i16) as i32;
        let rhs = (src_word as u16 as i16) as i32;
        lhs.wrapping_mul(rhs) as u32
    };
    state.d[dn] = result;
    state.sr &= !0x000F;
    if (result & 0x8000_0000) != 0 {
        state.sr |= 0x0008;
    }
    if result == 0 {
        state.sr |= 0x0004;
    }
    state.pc = ext_pc;
    Ok(true)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if execute_div(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_mul(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}

