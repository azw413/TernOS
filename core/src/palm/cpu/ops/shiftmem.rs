use crate::palm::cpu::core::{CpuState68k, StopReason};
use crate::palm::cpu::memory::MemoryMap;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn ea_target_addr(
    state: &mut CpuState68k,
    memory: &MemoryMap,
    pc: u32,
    mode: u16,
    reg: usize,
) -> Option<(u32, u32)> {
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
        2 => Some((state.a[reg], 2)),
        3 => {
            let addr = state.a[reg];
            state.a[reg] = state.a[reg].wrapping_add(2);
            Some((addr, 2))
        }
        4 => {
            state.a[reg] = state.a[reg].wrapping_sub(2);
            Some((state.a[reg], 2))
        }
        5 => {
            let disp16 = memory.read_u16_be(pc + 2)? as i16 as i32;
            Some((add_signed_u32(state.a[reg], disp16), 4))
        }
        6 => {
            let ext = memory.read_u16_be(pc + 2)?;
            Some((indexed_addr(state.a[reg], ext, state), 4))
        }
        7 => match reg {
            0 => {
                let aw = memory.read_u16_be(pc + 2)? as i16 as i32;
                Some((aw as u32, 4))
            }
            1 => Some((memory.read_u32_be(pc + 2)?, 6)),
            _ => None,
        },
        _ => None,
    }
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // Memory AS/LS/ROX/RO by 1 bit.
    if (word & 0xF0C0) != 0xE0C0 {
        return Ok(false);
    }
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let Some((addr, advance)) = ea_target_addr(state, memory, pc, mode, reg) else {
        return Ok(false);
    };
    let Some(cur) = memory.read_u16_be(addr) else {
        return Err(StopReason::OutOfBounds { pc });
    };
    let kind = ((word >> 9) & 0x0007) as u8;
    let x_in = if (state.sr & 0x0010) != 0 { 1u16 } else { 0u16 };
    let mut c = false;
    let out = match kind {
        0 => {
            // ASR
            c = (cur & 0x0001) != 0;
            ((cur as i16) >> 1) as u16
        }
        1 => {
            // ASL
            c = (cur & 0x8000) != 0;
            cur.wrapping_shl(1)
        }
        2 => {
            // LSR
            c = (cur & 0x0001) != 0;
            cur >> 1
        }
        3 => {
            // LSL
            c = (cur & 0x8000) != 0;
            cur.wrapping_shl(1)
        }
        4 => {
            // ROXR
            c = (cur & 0x0001) != 0;
            (cur >> 1) | (x_in << 15)
        }
        5 => {
            // ROXL
            c = (cur & 0x8000) != 0;
            (cur << 1) | x_in
        }
        6 => {
            // ROR
            c = (cur & 0x0001) != 0;
            (cur >> 1) | ((cur & 0x0001) << 15)
        }
        7 => {
            // ROL
            c = (cur & 0x8000) != 0;
            (cur << 1) | ((cur >> 15) & 1)
        }
        _ => cur,
    };
    memory.write_u16_be(addr, out);

    // Preserve upper SR, set N/Z/C and clear V. X follows C for all except ROx?
    let mut sr = state.sr & !0x001F;
    if (out & 0x8000) != 0 {
        sr |= 0x0008;
    }
    if out == 0 {
        sr |= 0x0004;
    }
    if c {
        sr |= 0x0001;
    }
    // Keep X untouched for pure rotate (ROR/ROL), update X for others.
    if kind != 6 && kind != 7 {
        sr &= !0x0010;
        if c {
            sr |= 0x0010;
        }
    } else {
        sr |= state.sr & 0x0010;
    }
    state.sr = sr;
    state.pc = pc.saturating_add(advance);
    Ok(true)
}
