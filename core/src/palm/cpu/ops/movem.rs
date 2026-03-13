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

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // MOVEM.[W/L] subset used by app prolog/epilog.
    // Guard by addressing mode first so EXT.W/EXT.L (0x488x/0x48Cx) are
    // not mis-decoded as MOVEM and accidentally skip the following opcode.
    let movem_mode = (word >> 3) & 0x0007;
    if ((word & 0xFB80) == 0x4880 || (word & 0xFB80) == 0x4C80)
        && matches!(movem_mode, 2 | 3 | 4 | 5)
    {
        let mode = movem_mode;
        let reg = (word & 0x0007) as usize;
        let is_mem_to_regs = (word & 0x0400) != 0; // 0x4Cxx direction
        let size_bytes = if (word & 0x0040) != 0 { 4u32 } else { 2u32 };
        let Some(mask) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let reg_read = |state: &CpuState68k, idx: usize| -> u32 {
            if idx < 8 { state.d[idx] } else { state.a[idx - 8] }
        };
        let reg_write = |state: &mut CpuState68k, idx: usize, v: u32, sz: u32| {
            if idx < 8 {
                match sz {
                    2 => state.d[idx] = v as i16 as i32 as u32,
                    _ => state.d[idx] = v,
                }
            } else {
                // An from word is sign-extended.
                state.a[idx - 8] = match sz {
                    2 => v as i16 as i32 as u32,
                    _ => v,
                };
            }
        };

        let mut pc_ext = pc.saturating_add(4);
        let disp16 = if mode == 5 {
            memory.read_u16_be(pc_ext).map(|v| {
                pc_ext = pc_ext.saturating_add(2);
                (v as i16) as i32
            })
        } else {
            None
        };
        let ea_base = |state: &CpuState68k, mode: u16, reg: usize, disp16: Option<i32>| -> u32 {
            match mode {
                2 | 3 | 4 => state.a[reg],
                5 => add_signed_u32(state.a[reg], disp16.unwrap_or(0)),
                _ => state.a[reg],
            }
        };

        match (is_mem_to_regs, mode) {
            // MOVEM <reglist>,-(An)
            (false, 4) => {
                let mut addr = state.a[reg];
                // For predecrement register->memory, MOVEM uses reversed
                // register mask encoding (bit 0 => A7 ... bit 15 => D0).
                // Iterate mask bits ascending so transfer order matches 68k.
                for bit in 0..16usize {
                    if (mask & (1u16 << bit)) == 0 {
                        continue;
                    }
                    let idx = 15usize.saturating_sub(bit);
                    let v = reg_read(state, idx);
                    addr = addr.wrapping_sub(size_bytes);
                    if size_bytes == 2 {
                        memory.write_u16_be(addr, v as u16);
                    } else {
                        memory.write_u32_be(addr, v);
                    }
                }
                state.a[reg] = addr;
            }
            // MOVEM <reglist>,(An) or (d16,An)
            (false, 2 | 5) => {
                let mut addr = ea_base(state, mode, reg, disp16);
                for idx in 0..16usize {
                    if (mask & (1u16 << idx)) == 0 {
                        continue;
                    }
                    let v = reg_read(state, idx);
                    if size_bytes == 2 {
                        memory.write_u16_be(addr, v as u16);
                    } else {
                        memory.write_u32_be(addr, v);
                    }
                    addr = addr.wrapping_add(size_bytes);
                }
            }
            // MOVEM (An)+,<reglist>
            (true, 3) => {
                let mut addr = state.a[reg];
                for idx in 0..16usize {
                    if (mask & (1u16 << idx)) == 0 {
                        continue;
                    }
                    let v = if size_bytes == 2 {
                        memory.read_u16_be(addr).map(|x| x as u32)
                    } else {
                        memory.read_u32_be(addr)
                    };
                    let v = v.unwrap_or(0);
                    reg_write(state, idx, v, size_bytes);
                    addr = addr.wrapping_add(size_bytes);
                }
                state.a[reg] = addr;
            }
            // MOVEM (An) or (d16,An),<reglist>
            (true, 2 | 5) => {
                let mut addr = ea_base(state, mode, reg, disp16);
                for idx in 0..16usize {
                    if (mask & (1u16 << idx)) == 0 {
                        continue;
                    }
                    let v = if size_bytes == 2 {
                        memory.read_u16_be(addr).map(|x| x as u32)
                    } else {
                        memory.read_u32_be(addr)
                    };
                    let v = v.unwrap_or(0);
                    reg_write(state, idx, v, size_bytes);
                    addr = addr.wrapping_add(size_bytes);
                }
            }
            _ => {}
        }
        let ext_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
        state.pc = pc.saturating_add(4 + ext_words * 2);
        return Ok(true);
    }


    Ok(false)
}
