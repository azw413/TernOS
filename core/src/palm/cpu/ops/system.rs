use crate::palm::cpu::core::{CpuState68k, StopReason};
use crate::palm::cpu::memory::MemoryMap;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn read_ea_word(
    state: &mut CpuState68k,
    memory: &MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
) -> Option<u16> {
    match mode {
        0 => Some((state.d[reg] & 0xFFFF) as u16),
        2 => memory.read_u16_be(state.a[reg]),
        3 => {
            let v = memory.read_u16_be(state.a[reg]);
            state.a[reg] = state.a[reg].wrapping_add(2);
            v
        }
        4 => {
            state.a[reg] = state.a[reg].wrapping_sub(2);
            memory.read_u16_be(state.a[reg])
        }
        5 => {
            let disp = memory.read_u16_be(*ext_pc)? as i16 as i32;
            *ext_pc = ext_pc.saturating_add(2);
            memory.read_u16_be(add_signed_u32(state.a[reg], disp))
        }
        7 => match reg {
            0 => {
                let aw = memory.read_u16_be(*ext_pc)? as i16 as i32;
                *ext_pc = ext_pc.saturating_add(2);
                memory.read_u16_be(aw as u32)
            }
            1 => {
                let al = memory.read_u32_be(*ext_pc)?;
                *ext_pc = ext_pc.saturating_add(4);
                memory.read_u16_be(al)
            }
            _ => None,
        },
        _ => None,
    }
}

fn write_ea_word(
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
    mode: u16,
    reg: usize,
    ext_pc: &mut u32,
    value: u16,
) -> bool {
    match mode {
        0 => {
            state.d[reg] = (state.d[reg] & 0xFFFF_0000) | (value as u32);
            true
        }
        2 => {
            memory.write_u16_be(state.a[reg], value);
            true
        }
        3 => {
            memory.write_u16_be(state.a[reg], value);
            state.a[reg] = state.a[reg].wrapping_add(2);
            true
        }
        4 => {
            state.a[reg] = state.a[reg].wrapping_sub(2);
            memory.write_u16_be(state.a[reg], value);
            true
        }
        5 => {
            let Some(disp16) = memory.read_u16_be(*ext_pc) else {
                return false;
            };
            *ext_pc = ext_pc.saturating_add(2);
            let addr = add_signed_u32(state.a[reg], (disp16 as i16) as i32);
            memory.write_u16_be(addr, value);
            true
        }
        7 => match reg {
            0 => {
                let Some(aw) = memory.read_u16_be(*ext_pc) else {
                    return false;
                };
                *ext_pc = ext_pc.saturating_add(2);
                let addr = (aw as i16 as i32) as u32;
                memory.write_u16_be(addr, value);
                true
            }
            1 => {
                let Some(al) = memory.read_u32_be(*ext_pc) else {
                    return false;
                };
                *ext_pc = ext_pc.saturating_add(4);
                memory.write_u16_be(al, value);
                true
            }
            _ => false,
        },
        _ => false,
    }
}

fn execute_core_system(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    match word {
        0x4E70 | 0x4E71 => {
            // RESET / NOP: no-op in emulator.
            state.pc = pc.saturating_add(2);
            return Ok(true);
        }
        0x4E72 => {
            // STOP #imm: for now don't halt executor, just load SR.
            let Some(new_sr) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            state.sr = new_sr;
            state.pc = pc.saturating_add(4);
            return Ok(true);
        }
        0x4E73 => {
            // RTE: pop SR then PC.
            let Some(sr) = memory.read_u16_be(state.a[7]) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            state.a[7] = state.a[7].wrapping_add(2);
            let Some(new_pc) = memory.read_u32_be(state.a[7]) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            state.a[7] = state.a[7].wrapping_add(4);
            state.sr = sr;
            state.pc = new_pc;
            return Ok(true);
        }
        0x4E76 => {
            // TRAPV: stop as trap only if V set, else continue.
            if (state.sr & 0x0002) != 0 {
                return Err(StopReason::Trap { vector: 7, pc });
            }
            state.pc = pc.saturating_add(2);
            return Ok(true);
        }
        0x4E77 => {
            // RTR: pop CCR low byte from word, then PC.
            let Some(ccr_word) = memory.read_u16_be(state.a[7]) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            state.a[7] = state.a[7].wrapping_add(2);
            let Some(new_pc) = memory.read_u32_be(state.a[7]) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            state.a[7] = state.a[7].wrapping_add(4);
            state.sr = (state.sr & !0x001F) | (ccr_word & 0x001F);
            state.pc = new_pc;
            return Ok(true);
        }
        _ => {}
    }
    Ok(false)
}

fn execute_move_usp(word: u16, pc: u32, state: &mut CpuState68k) -> Result<bool, StopReason> {
    if (word & 0xFFF8) == 0x4E60 {
        // MOVE An,USP
        let an = (word & 0x0007) as usize;
        state.a[7] = state.a[an];
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }
    if (word & 0xFFF8) == 0x4E68 {
        // MOVE USP,An
        let an = (word & 0x0007) as usize;
        state.a[an] = state.a[7];
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }
    Ok(false)
}

fn execute_move_sr_ccr(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;

    // MOVE from SR/CCR to <ea>.
    if (word & 0xFFC0) == 0x40C0 || (word & 0xFFC0) == 0x42C0 {
        let v = if (word & 0xFFC0) == 0x40C0 {
            state.sr
        } else {
            state.sr & 0x001F
        };
        let mut ext_pc = pc.saturating_add(2);
        if !write_ea_word(state, memory, mode, reg, &mut ext_pc, v) {
            return Err(StopReason::OutOfBounds { pc });
        }
        state.pc = ext_pc;
        return Ok(true);
    }

    // MOVE <ea> to SR/CCR.
    if (word & 0xFFC0) == 0x46C0 || (word & 0xFFC0) == 0x44C0 {
        let mut ext_pc = pc.saturating_add(2);
        let Some(v) = read_ea_word(state, memory, mode, reg, &mut ext_pc) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        if (word & 0xFFC0) == 0x46C0 {
            state.sr = v;
        } else {
            state.sr = (state.sr & !0x001F) | (v & 0x001F);
        }
        state.pc = ext_pc;
        return Ok(true);
    }

    Ok(false)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // Immediate to CCR/SR optional system forms.
    // ORI/ANDI/EORI #imm,CCR
    if matches!(word, 0x003C | 0x023C | 0x0A3C) {
        let Some(imm) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let ccr = state.sr & 0x001F;
        let out = match word {
            0x003C => ccr | (imm & 0x001F),
            0x023C => ccr & (imm & 0x001F),
            _ => ccr ^ (imm & 0x001F),
        };
        state.sr = (state.sr & !0x001F) | out;
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }
    // ORI/ANDI/EORI #imm,SR
    if matches!(word, 0x007C | 0x027C | 0x0A7C) {
        let Some(imm) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        state.sr = match word {
            0x007C => state.sr | imm,
            0x027C => state.sr & imm,
            _ => state.sr ^ imm,
        };
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }

    if execute_core_system(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_move_usp(word, pc, state)? {
        return Ok(true);
    }
    if execute_move_sr_ccr(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}
