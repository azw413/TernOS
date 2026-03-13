use crate::palm::cpu::core::{CpuState68k, StopReason};
use crate::palm::cpu::memory::MemoryMap;

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

fn set_logic_flags(state: &mut CpuState68k, value: u32, bits: u32) {
    let sign = 1u32 << (bits - 1);
    let mask = if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    let v = value & mask;
    // Preserve X. Clear N/Z/V/C then set N/Z.
    let x = state.sr & 0x0010;
    state.sr = (state.sr & !0x001F) | x;
    if (v & sign) != 0 {
        state.sr |= 0x0008;
    }
    if v == 0 {
        state.sr |= 0x0004;
    }
}

fn read_ea_data(
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

fn write_ea_data(
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
                let Some(addr) = memory.read_u32_be(*ext_pc) else {
                    return false;
                };
                *ext_pc = ext_pc.saturating_add(4);
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
            _ => false,
        },
        _ => false,
    }
}

fn execute_or_and(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let top = word & 0xF000;
    if top != 0x8000 && top != 0xC000 {
        return Ok(false);
    }
    let opmode = (word >> 6) & 0x0007;
    let dn = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;

    let (size_bytes, ea_to_dn) = match opmode {
        0 => (1u32, true),
        1 => (2u32, true),
        2 => (4u32, true),
        4 => (1u32, false),
        5 => (2u32, false),
        6 => (4u32, false),
        _ => return Ok(false),
    };

    if ea_to_dn {
        let mut ext_pc = pc.saturating_add(2);
        let Some(src_raw) =
            read_ea_data(state, memory, mode, reg, &mut ext_pc, size_bytes, pc.saturating_add(2))
        else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let mask = if size_bytes == 4 {
            u32::MAX
        } else {
            (1u32 << (size_bytes * 8)) - 1
        };
        let src = src_raw & mask;
        let dst = state.d[dn] & mask;
        let out = if top == 0x8000 { dst | src } else { dst & src };
        match size_bytes {
            1 => state.d[dn] = (state.d[dn] & 0xFFFF_FF00) | out,
            2 => state.d[dn] = (state.d[dn] & 0xFFFF_0000) | out,
            _ => state.d[dn] = out,
        }
        set_logic_flags(state, out, size_bytes * 8);
        state.pc = ext_pc;
        return Ok(true);
    }

    // Dn,<ea>
    let mut ext_pc = pc.saturating_add(2);
    let Some(dst_raw) =
        read_ea_data(state, memory, mode, reg, &mut ext_pc, size_bytes, pc.saturating_add(2))
    else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let mask = if size_bytes == 4 {
        u32::MAX
    } else {
        (1u32 << (size_bytes * 8)) - 1
    };
    let src = state.d[dn] & mask;
    let dst = dst_raw & mask;
    let out = if top == 0x8000 { dst | src } else { dst & src };
    let mut dst_ext_pc = pc.saturating_add(2);
    if !write_ea_data(state, memory, mode, reg, &mut dst_ext_pc, size_bytes, out) {
        return Err(StopReason::OutOfBounds { pc });
    }
    set_logic_flags(state, out, size_bytes * 8);
    state.pc = ext_pc;
    Ok(true)
}

fn execute_eor(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0xB000 {
        return Ok(false);
    }
    let opmode = (word >> 6) & 0x0007;
    let size_bytes = match opmode {
        4 => 1u32,
        5 => 2u32,
        6 => 4u32,
        _ => return Ok(false),
    };

    let dn = ((word >> 9) & 0x0007) as usize;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;

    let mut ext_pc = pc.saturating_add(2);
    let Some(dst_raw) =
        read_ea_data(state, memory, mode, reg, &mut ext_pc, size_bytes, pc.saturating_add(2))
    else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let mask = if size_bytes == 4 {
        u32::MAX
    } else {
        (1u32 << (size_bytes * 8)) - 1
    };
    let src = state.d[dn] & mask;
    let out = (dst_raw & mask) ^ src;
    let mut dst_ext_pc = pc.saturating_add(2);
    if !write_ea_data(state, memory, mode, reg, &mut dst_ext_pc, size_bytes, out) {
        return Err(StopReason::OutOfBounds { pc });
    }
    set_logic_flags(state, out, size_bytes * 8);
    state.pc = ext_pc;
    Ok(true)
}

fn execute_static_bit(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0x0000 || (word & 0x0F00) != 0x0800 {
        return Ok(false);
    }
    let op_kind = ((word >> 6) & 0x0003) as u8; // 0=BTST 1=BCHG 2=BCLR 3=BSET
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let Some(bit_imm) = memory.read_u16_be(pc + 2) else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let mut ext_pc = pc.saturating_add(4);

    if mode == 0 {
        let bit = (bit_imm as u32) & 31;
        let mask = 1u32 << bit;
        let old = state.d[reg];
        // Z only, preserve all other flags.
        state.sr &= !0x0004;
        if (old & mask) == 0 {
            state.sr |= 0x0004;
        }
        let new_val = match op_kind {
            0 => old,
            1 => old ^ mask,
            2 => old & !mask,
            _ => old | mask,
        };
        if op_kind != 0 {
            state.d[reg] = new_val;
        }
        state.pc = ext_pc;
        return Ok(true);
    }

    // Byte memory forms use low 3 bits.
    let bit = (bit_imm as u8) & 7;
    let mask = 1u8 << bit;
    let Some(old_raw) = read_ea_data(
        state,
        memory,
        mode,
        reg,
        &mut ext_pc,
        1,
        pc.saturating_add(2),
    ) else {
        // For unsupported EAs, still consume extension and keep running.
        let extra = ea_ext_words(mode, reg as u16).unwrap_or(0);
        state.pc = pc.saturating_add(4 + extra * 2);
        return Ok(true);
    };
    let old = old_raw as u8;
    state.sr &= !0x0004;
    if (old & mask) == 0 {
        state.sr |= 0x0004;
    }
    let new_val = match op_kind {
        0 => old,
        1 => old ^ mask,
        2 => old & !mask,
        _ => old | mask,
    };
    if op_kind != 0 {
        let mut dst_ext_pc = pc.saturating_add(4);
        if !write_ea_data(
            state,
            memory,
            mode,
            reg,
            &mut dst_ext_pc,
            1,
            new_val as u32,
        ) {
            return Err(StopReason::OutOfBounds { pc });
        }
    }
    state.pc = ext_pc;
    Ok(true)
}

fn execute_movep(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // MOVEP.{W/L} Dn,(d16,Ay) and (d16,Ay),Dn
    if (word & 0xF138) != 0x0108 {
        return Ok(false);
    }
    let dn = ((word >> 9) & 0x0007) as usize;
    let ay = (word & 0x0007) as usize;
    let opmode = (word >> 6) & 0x0003;
    let Some(disp16) = memory.read_u16_be(pc + 2) else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let base = add_signed_u32(state.a[ay], (disp16 as i16) as i32);
    match opmode {
        0 => {
            // (d16,Ay) -> Dn, word
            let b0 = memory.read_u8(base).unwrap_or(0) as u32;
            let b1 = memory.read_u8(base.wrapping_add(2)).unwrap_or(0) as u32;
            let w = (b0 << 8) | b1;
            state.d[dn] = (state.d[dn] & 0xFFFF_0000) | w;
        }
        1 => {
            // (d16,Ay) -> Dn, long
            let b0 = memory.read_u8(base).unwrap_or(0) as u32;
            let b1 = memory.read_u8(base.wrapping_add(2)).unwrap_or(0) as u32;
            let b2 = memory.read_u8(base.wrapping_add(4)).unwrap_or(0) as u32;
            let b3 = memory.read_u8(base.wrapping_add(6)).unwrap_or(0) as u32;
            state.d[dn] = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
        }
        2 => {
            // Dn -> (d16,Ay), word
            let v = state.d[dn] & 0xFFFF;
            memory.write_u8(base, ((v >> 8) & 0xFF) as u8);
            memory.write_u8(base.wrapping_add(2), (v & 0xFF) as u8);
        }
        3 => {
            // Dn -> (d16,Ay), long
            let v = state.d[dn];
            memory.write_u8(base, ((v >> 24) & 0xFF) as u8);
            memory.write_u8(base.wrapping_add(2), ((v >> 16) & 0xFF) as u8);
            memory.write_u8(base.wrapping_add(4), ((v >> 8) & 0xFF) as u8);
            memory.write_u8(base.wrapping_add(6), (v & 0xFF) as u8);
        }
        _ => {}
    }
    state.pc = pc.saturating_add(4);
    Ok(true)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if execute_static_bit(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_movep(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_eor(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_or_and(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}
