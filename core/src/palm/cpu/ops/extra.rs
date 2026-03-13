use crate::palm::cpu::core::{CpuState68k, StopReason};
use crate::palm::cpu::memory::MemoryMap;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn set_logic_flags(state: &mut CpuState68k, value: u32, bits: u32) {
    let mask = if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    let sign = 1u32 << (bits - 1);
    let v = value & mask;
    let x = state.sr & 0x0010;
    state.sr = (state.sr & !0x001F) | x;
    if (v & sign) != 0 {
        state.sr |= 0x0008;
    }
    if v == 0 {
        state.sr |= 0x0004;
    }
}

fn read_ea(
    state: &mut CpuState68k,
    memory: &MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
    bytes: u32,
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
        4 => {
            let dec = if bytes == 1 && reg == 7 { 2 } else { bytes };
            state.a[reg] = state.a[reg].wrapping_sub(dec);
            let addr = state.a[reg];
            match bytes {
                1 => memory.read_u8(addr).map(u32::from),
                2 => memory.read_u16_be(addr).map(u32::from),
                4 => memory.read_u32_be(addr),
                _ => None,
            }
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
            _ => None,
        },
        _ => None,
    }
}

fn write_ea(
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
    bytes: u32,
    value: u32,
) -> bool {
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
        0 => {
            match bytes {
                1 => state.d[reg] = (state.d[reg] & 0xFFFF_FF00) | (value & 0xFF),
                2 => state.d[reg] = (state.d[reg] & 0xFFFF_0000) | (value & 0xFFFF),
                4 => state.d[reg] = value,
                _ => return false,
            }
            true
        }
        2 => {
            let addr = state.a[reg];
            match bytes {
                1 => {
                    memory.write_u8(addr, value as u8);
                }
                2 => {
                    memory.write_u16_be(addr, value as u16);
                }
                4 => {
                    memory.write_u32_be(addr, value);
                }
                _ => return false,
            }
            true
        }
        3 => {
            let addr = state.a[reg];
            match bytes {
                1 => {
                    memory.write_u8(addr, value as u8);
                }
                2 => {
                    memory.write_u16_be(addr, value as u16);
                }
                4 => {
                    memory.write_u32_be(addr, value);
                }
                _ => return false,
            }
            let inc = if bytes == 1 && reg == 7 { 2 } else { bytes };
            state.a[reg] = state.a[reg].wrapping_add(inc);
            true
        }
        4 => {
            let dec = if bytes == 1 && reg == 7 { 2 } else { bytes };
            state.a[reg] = state.a[reg].wrapping_sub(dec);
            let addr = state.a[reg];
            match bytes {
                1 => {
                    memory.write_u8(addr, value as u8);
                }
                2 => {
                    memory.write_u16_be(addr, value as u16);
                }
                4 => {
                    memory.write_u32_be(addr, value);
                }
                _ => return false,
            }
            true
        }
        5 => {
            let Some(disp16) = memory.read_u16_be(*ext_pc) else {
                return false;
            };
            *ext_pc = ext_pc.saturating_add(2);
            let addr = add_signed_u32(state.a[reg], (disp16 as i16) as i32);
            match bytes {
                1 => {
                    memory.write_u8(addr, value as u8);
                }
                2 => {
                    memory.write_u16_be(addr, value as u16);
                }
                4 => {
                    memory.write_u32_be(addr, value);
                }
                _ => return false,
            }
            true
        }
        6 => {
            let Some(ext) = memory.read_u16_be(*ext_pc) else {
                return false;
            };
            *ext_pc = ext_pc.saturating_add(2);
            let addr = indexed_addr(state.a[reg], ext, state);
            match bytes {
                1 => {
                    memory.write_u8(addr, value as u8);
                }
                2 => {
                    memory.write_u16_be(addr, value as u16);
                }
                4 => {
                    memory.write_u32_be(addr, value);
                }
                _ => return false,
            }
            true
        }
        7 => match reg {
            0 => {
                let Some(aw) = memory.read_u16_be(*ext_pc) else {
                    return false;
                };
                *ext_pc = ext_pc.saturating_add(2);
                let addr = (aw as i16 as i32) as u32;
                match bytes {
                    1 => {
                        memory.write_u8(addr, value as u8);
                    }
                    2 => {
                        memory.write_u16_be(addr, value as u16);
                    }
                    4 => {
                        memory.write_u32_be(addr, value);
                    }
                    _ => return false,
                }
                true
            }
            1 => {
                let Some(al) = memory.read_u32_be(*ext_pc) else {
                    return false;
                };
                *ext_pc = ext_pc.saturating_add(4);
                match bytes {
                    1 => {
                        memory.write_u8(al, value as u8);
                    }
                    2 => {
                        memory.write_u16_be(al, value as u16);
                    }
                    4 => {
                        memory.write_u32_be(al, value);
                    }
                    _ => return false,
                }
                true
            }
            _ => false,
        },
        _ => false,
    }
}

fn execute_not_negx_tas(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let top = word & 0xFF00;
    let is_negx = matches!(top, 0x4000 | 0x4040 | 0x4080);
    let is_not = matches!(top, 0x4600 | 0x4640 | 0x4680);
    let is_tas = (word & 0xFFC0) == 0x4AC0;
    if !(is_negx || is_not || is_tas) {
        return Ok(false);
    }

    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let bytes = if is_tas {
        1
    } else {
        match (word >> 6) & 0x0003 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => return Ok(false),
        }
    };
    let mut ext_pc = pc.saturating_add(2);
    let Some(dst_raw) = read_ea(state, memory, mode, reg, &mut ext_pc, bytes) else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let mask = if bytes == 4 {
        u32::MAX
    } else {
        (1u32 << (bytes * 8)) - 1
    };
    let dst = dst_raw & mask;

    if is_tas {
        // TAS flags reflect original byte, then set bit 7.
        set_logic_flags(state, dst, 8);
        let out = dst | 0x80;
        let mut write_pc = pc.saturating_add(2);
        if !write_ea(state, memory, mode, reg, &mut write_pc, 1, out) {
            return Err(StopReason::OutOfBounds { pc });
        }
        state.pc = ext_pc;
        return Ok(true);
    }

    let out = if is_not {
        (!dst) & mask
    } else {
        let x = if (state.sr & 0x0010) != 0 { 1u32 } else { 0u32 };
        (0u32).wrapping_sub(dst).wrapping_sub(x) & mask
    };
    let mut write_pc = pc.saturating_add(2);
    if !write_ea(state, memory, mode, reg, &mut write_pc, bytes, out) {
        return Err(StopReason::OutOfBounds { pc });
    }

    if is_not {
        set_logic_flags(state, out, bytes * 8);
    } else {
        // NEGX flags: N/Z/V/C and X; Z is chained.
        let bits = bytes * 8;
        let sign = 1u32 << (bits - 1);
        let prev_z = (state.sr & 0x0004) != 0;
        let x = if (state.sr & 0x0010) != 0 { 1u32 } else { 0u32 };
        let sub = dst.wrapping_add(x) & mask;
        let c = sub != 0;
        let v = ((dst ^ sub) & (dst ^ out) & sign) != 0;
        let mut sr = state.sr & !0x001F;
        if c {
            sr |= 0x0011;
        }
        if (out & sign) != 0 {
            sr |= 0x0008;
        }
        if out == 0 {
            if prev_z {
                sr |= 0x0004;
            }
        } else {
            sr &= !0x0004;
        }
        if v {
            sr |= 0x0002;
        }
        state.sr = sr;
    }

    state.pc = ext_pc;
    Ok(true)
}

fn execute_chk(word: u16, pc: u32, state: &mut CpuState68k, memory: &MemoryMap) -> Result<bool, StopReason> {
    // CHK <ea>,Dn (word)
    if (word & 0xF1C0) != 0x4180 {
        return Ok(false);
    }
    let dn = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let mut ext_pc = pc.saturating_add(2);
    let Some(bound_w) = read_ea(state, memory, mode, reg, &mut ext_pc, 2) else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let bound = (bound_w as u16) as i16 as i32;
    let val = (state.d[dn] as u16 as i16) as i32;
    // N reflects lower bound failure.
    state.sr &= !0x0008;
    if val < 0 {
        state.sr |= 0x0008;
        return Err(StopReason::Trap { vector: 6, pc });
    }
    if val > bound {
        return Err(StopReason::Trap { vector: 6, pc });
    }
    state.pc = ext_pc;
    Ok(true)
}

fn execute_illegal(word: u16, pc: u32) -> Result<bool, StopReason> {
    if word == 0x4AFC {
        return Err(StopReason::Trap { vector: 4, pc });
    }
    Ok(false)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if execute_not_negx_tas(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_chk(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_illegal(word, pc)? {
        return Ok(true);
    }
    Ok(false)
}
