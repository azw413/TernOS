use crate::prc_app::cpu::core::StopReason;
use crate::prc_app::cpu::ops::cond::sr_cond_true;
use crate::prc_app::cpu::{core::CpuState68k, memory::MemoryMap};

fn ea_ext_words(mode: u16, reg: u16) -> Option<u32> {
    match mode {
        0 | 1 | 2 | 3 | 4 => Some(0), // Dn/An/(An)/(An)+/-(An)
        5 | 6 => Some(1),             // d16(An) / d8(An,Xn)
        7 => match reg {
            0 => Some(1), // abs.w
            1 => Some(2), // abs.l
            2 | 3 => Some(1),
            4 => Some(0),
            _ => None,
        },
        _ => None,
    }
}

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn set_ccr_nz(state: &mut CpuState68k, negative: bool, zero: bool) {
    // Keep X bit and upper status bits unchanged; update N/Z/V/C.
    state.sr &= !0x000F;
    if negative {
        state.sr |= 0x0008;
    }
    if zero {
        state.sr |= 0x0004;
    }
}

fn mem_read(memory: &MemoryMap, addr: u32, size_bytes: u32) -> Option<u32> {
    match size_bytes {
        1 => memory.read_u8(addr).map(u32::from),
        2 => memory.read_u16_be(addr).map(u32::from),
        4 => memory.read_u32_be(addr),
        _ => None,
    }
}

fn mem_write(memory: &mut MemoryMap, addr: u32, size_bytes: u32, value: u32) -> Option<()> {
    match size_bytes {
        1 => {
            memory.write_u8(addr, value as u8);
            Some(())
        }
        2 => {
            memory.write_u16_be(addr, value as u16);
            Some(())
        }
        4 => {
            memory.write_u32_be(addr, value);
            Some(())
        }
        _ => None,
    }
}

fn indexed_addr(base: u32, ext: u16, state: &CpuState68k) -> u32 {
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
}

fn ea_read_for_modify(
    state: &mut CpuState68k,
    memory: &MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
    size_bytes: u32,
) -> Option<(u32, u32)> {
    match mode {
        2 => {
            let addr = state.a[reg];
            Some((addr, mem_read(memory, addr, size_bytes)?))
        }
        3 => {
            let addr = state.a[reg];
            let v = mem_read(memory, addr, size_bytes)?;
            let inc = if size_bytes == 1 && reg == 7 { 2 } else { size_bytes };
            state.a[reg] = state.a[reg].wrapping_add(inc);
            Some((addr, v))
        }
        4 => {
            let dec = if size_bytes == 1 && reg == 7 { 2 } else { size_bytes };
            state.a[reg] = state.a[reg].wrapping_sub(dec);
            let addr = state.a[reg];
            Some((addr, mem_read(memory, addr, size_bytes)?))
        }
        5 => {
            let disp = memory.read_u16_be(*ext_pc)? as i16 as i32;
            *ext_pc = ext_pc.saturating_add(2);
            let addr = add_signed_u32(state.a[reg], disp);
            Some((addr, mem_read(memory, addr, size_bytes)?))
        }
        6 => {
            let ext = memory.read_u16_be(*ext_pc)?;
            *ext_pc = ext_pc.saturating_add(2);
            let addr = indexed_addr(state.a[reg], ext, state);
            Some((addr, mem_read(memory, addr, size_bytes)?))
        }
        7 => match reg {
            0 => {
                let aw = memory.read_u16_be(*ext_pc)? as i16 as i32 as u32;
                *ext_pc = ext_pc.saturating_add(2);
                Some((aw, mem_read(memory, aw, size_bytes)?))
            }
            1 => {
                let addr = memory.read_u32_be(*ext_pc)?;
                *ext_pc = ext_pc.saturating_add(4);
                Some((addr, mem_read(memory, addr, size_bytes)?))
            }
            _ => None,
        },
        _ => None,
    }
}

fn ea_write_byte(
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
    value: u8,
) -> Option<()> {
    match mode {
        0 => {
            state.d[reg] = (state.d[reg] & 0xFFFF_FF00) | (value as u32);
            Some(())
        }
        2 => {
            memory.write_u8(state.a[reg], value);
            Some(())
        }
        3 => {
            let addr = state.a[reg];
            memory.write_u8(addr, value);
            let inc = if reg == 7 { 2 } else { 1 };
            state.a[reg] = state.a[reg].wrapping_add(inc);
            Some(())
        }
        4 => {
            let dec = if reg == 7 { 2 } else { 1 };
            state.a[reg] = state.a[reg].wrapping_sub(dec);
            memory.write_u8(state.a[reg], value);
            Some(())
        }
        5 => {
            let disp = memory.read_u16_be(*ext_pc)? as i16 as i32;
            *ext_pc = ext_pc.saturating_add(2);
            let addr = add_signed_u32(state.a[reg], disp);
            memory.write_u8(addr, value);
            Some(())
        }
        6 => {
            let ext = memory.read_u16_be(*ext_pc)?;
            *ext_pc = ext_pc.saturating_add(2);
            let addr = indexed_addr(state.a[reg], ext, state);
            memory.write_u8(addr, value);
            Some(())
        }
        7 => match reg {
            0 => {
                let aw = memory.read_u16_be(*ext_pc)? as i16 as i32 as u32;
                *ext_pc = ext_pc.saturating_add(2);
                memory.write_u8(aw, value);
                Some(())
            }
            1 => {
                let addr = memory.read_u32_be(*ext_pc)?;
                *ext_pc = ext_pc.saturating_add(4);
                memory.write_u8(addr, value);
                Some(())
            }
            _ => None,
        },
        _ => None,
    }
}

fn execute_dbcc(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF0F8) != 0x50C8 {
        return Ok(false);
    }
    let cond = ((word >> 8) & 0x000F) as u8;
    let dn = (word & 0x0007) as usize;
    let Some(disp16) = memory.read_u16_be(pc + 2) else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let disp = (disp16 as i16) as i32;
    let next_pc = pc.saturating_add(4);

    if !sr_cond_true(state.sr, cond) {
        let count = (state.d[dn] & 0xFFFF) as u16;
        let new_count = count.wrapping_sub(1);
        state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (new_count as u32);
        if new_count != 0xFFFF {
            // DBcc displacement is based from the post-extension PC.
            let target = (next_pc as i64) + (disp as i64);
            if target < 0 {
                return Err(StopReason::OutOfBounds { pc });
            }
            state.pc = target as u32;
        } else {
            state.pc = next_pc;
        }
    } else {
        state.pc = next_pc;
    }
    Ok(true)
}

fn execute_scc(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF0C0) != 0x50C0 {
        return Ok(false);
    }
    // Exclude DBcc and TRAPcc forms from Scc.
    if (word & 0xF0F8) == 0x50C8 || (word & 0xF0F8) == 0x50F8 {
        return Ok(false);
    }

    let cond = ((word >> 8) & 0x000F) as u8;
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let value = if sr_cond_true(state.sr, cond) { 0xFF } else { 0x00 };

    // Scc destination is data alterable EA; An direct is invalid.
    if mode == 1 {
        return Ok(false);
    }

    let mut ext_pc = pc.saturating_add(2);
    if ea_write_byte(state, memory, mode, reg, &mut ext_pc, value).is_none() {
        return Ok(false);
    }
    state.pc = ext_pc;
    Ok(true)
}

fn execute_trapcc(word: u16, pc: u32, state: &mut CpuState68k) -> Result<bool, StopReason> {
    if (word & 0xF0F8) != 0x50F8 {
        return Ok(false);
    }
    let cond = ((word >> 8) & 0x000F) as u8;
    let reg = word & 0x0007;
    let ext = match reg {
        2 => 2u32, // #<word>
        3 => 4u32, // #<long>
        4 => 0u32, // no immediate
        _ => return Ok(false),
    };

    // We currently treat TRAPcc as non-fatal and just consume the instruction.
    // This preserves control-flow in apps that use TRAPcc for assertions.
    let _taken = sr_cond_true(state.sr, cond);
    state.pc = pc.saturating_add(2 + ext);
    Ok(true)
}

fn execute_addq_subq(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0x5000 {
        return Ok(false);
    }

    // Exclude Scc/DBcc/TRAPcc encodings.
    if (word & 0x00C0) == 0x00C0 {
        return Ok(false);
    }

    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let mut q = ((word >> 9) & 0x0007) as u32;
    if q == 0 {
        q = 8;
    }
    let is_sub = ((word >> 8) & 0x1) != 0;
    let size_bits = (word >> 6) & 0x0003;
    let size_bytes = match size_bits {
        0 => 1u32,
        1 => 2u32,
        2 => 4u32,
        _ => return Ok(false),
    };

    if mode == 0 {
        // Dn destination.
        let dn = reg;
        match size_bytes {
            1 => {
                let d = (state.d[dn] & 0xFF) as u8;
                let r = if is_sub { d.wrapping_sub(q as u8) } else { d.wrapping_add(q as u8) };
                state.d[dn] = (state.d[dn] & 0xFFFF_FF00) | (r as u32);
                set_ccr_nz(state, (r & 0x80) != 0, r == 0);
            }
            2 => {
                let d = (state.d[dn] & 0xFFFF) as u16;
                let r = if is_sub { d.wrapping_sub(q as u16) } else { d.wrapping_add(q as u16) };
                state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (r as u32);
                set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
            }
            _ => {
                let d = state.d[dn];
                let r = if is_sub { d.wrapping_sub(q) } else { d.wrapping_add(q) };
                state.d[dn] = r;
                set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
            }
        }
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }

    if mode == 1 {
        // An destination: address arithmetic only, no CCR updates.
        state.a[reg] = if is_sub {
            state.a[reg].wrapping_sub(q)
        } else {
            state.a[reg].wrapping_add(q)
        };
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }

    let mut ext_pc = pc.saturating_add(2);
    let Some((addr, cur)) = ea_read_for_modify(state, memory, mode, reg, &mut ext_pc, size_bytes)
    else {
        return Ok(false);
    };

    let res = match size_bytes {
        1 => {
            let d = cur as u8;
            let r = if is_sub { d.wrapping_sub(q as u8) } else { d.wrapping_add(q as u8) };
            set_ccr_nz(state, (r & 0x80) != 0, r == 0);
            r as u32
        }
        2 => {
            let d = cur as u16;
            let r = if is_sub { d.wrapping_sub(q as u16) } else { d.wrapping_add(q as u16) };
            set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
            r as u32
        }
        _ => {
            let d = cur;
            let r = if is_sub { d.wrapping_sub(q) } else { d.wrapping_add(q) };
            set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
            r
        }
    };

    mem_write(memory, addr, size_bytes, res).ok_or(StopReason::OutOfBounds { pc })?;
    state.pc = ext_pc;
    Ok(true)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0x5000 {
        return Ok(false);
    }

    if execute_dbcc(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_scc(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_trapcc(word, pc, state)? {
        return Ok(true);
    }
    if execute_addq_subq(word, pc, state, memory)? {
        return Ok(true);
    }

    // Keep decoder moving for unimplemented 0x5xxx forms.
    let mode = (word >> 3) & 0x0007;
    let reg = word & 0x0007;
    if let Some(ea_words) = ea_ext_words(mode, reg) {
        state.pc = pc.saturating_add(2 + ea_words * 2);
        return Ok(true);
    }

    Ok(false)
}
