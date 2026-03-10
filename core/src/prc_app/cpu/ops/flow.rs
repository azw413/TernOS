use crate::prc_app::cpu::core::{CpuState68k, StopReason};
use crate::prc_app::cpu::memory::MemoryMap;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
    }
}

fn indexed_addr(base: u32, ext: u16, state: &CpuState68k) -> u32 {
    let idx_is_a = (ext & 0x8000) != 0;
    let idx_reg = ((ext >> 12) & 0x0007) as usize;
    let idx_long = (ext & 0x0800) != 0;
    let disp8 = (ext & 0x00FF) as u8 as i8 as i32;
    let idx_raw = if idx_is_a {
        state.a[idx_reg]
    } else {
        state.d[idx_reg]
    };
    let idx = if idx_long {
        idx_raw as i32
    } else {
        (idx_raw as u16 as i16) as i32
    };
    add_signed_u32(base, disp8.saturating_add(idx))
}

fn read_control_ea_target(
    word: u16,
    pc: u32,
    state: &CpuState68k,
    memory: &MemoryMap,
) -> Option<(u32, u32)> {
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    match mode {
        2 => Some((state.a[reg], 2)), // (An)
        5 => {
            let disp16 = memory.read_u16_be(pc + 2)? as i16 as i32;
            Some((add_signed_u32(state.a[reg], disp16), 4)) // (d16,An)
        }
        6 => {
            let ext = memory.read_u16_be(pc + 2)?;
            Some((indexed_addr(state.a[reg], ext, state), 4)) // (d8,An,Xn)
        }
        7 => match reg {
            0 => {
                let aw = memory.read_u16_be(pc + 2)? as i16 as i32;
                Some((aw as u32, 4)) // (abs.w)
            }
            1 => Some((memory.read_u32_be(pc + 2)?, 6)), // (abs.l)
            2 => {
                let disp16 = memory.read_u16_be(pc + 2)? as i16 as i32;
                Some((add_signed_u32(pc.saturating_add(2), disp16), 4)) // (d16,PC)
            }
            3 => {
                let ext = memory.read_u16_be(pc + 2)?;
                Some((indexed_addr(pc.saturating_add(2), ext, state), 4)) // (d8,PC,Xn)
            }
            _ => None,
        },
        _ => None,
    }
}

fn execute_jmp(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // JMP <ea> control forms.
    if (word & 0xFFC0) == 0x4EC0 {
        let Some((dst, _len)) = read_control_ea_target(word, pc, state, memory) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        state.pc = dst;
        return Ok(true);
    }
    Ok(false)
}

fn execute_jsr(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // JSR <ea> control forms.
    if (word & 0xFFC0) == 0x4E80 {
        let Some((dst, instr_len)) = read_control_ea_target(word, pc, state, memory) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let ret = pc.saturating_add(instr_len);
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], ret);
        state.call_stack.push(ret);
        state.pc = dst;
        return Ok(true);
    }
    Ok(false)
}

fn execute_rts(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if word != 0x4E75 {
        return Ok(false);
    }
    let expected_ret = state.call_stack.pop();
    let mut ret = if let Some(v) = memory.read_u32_be(state.a[7]) {
        state.a[7] = state.a[7].wrapping_add(4);
        v
    } else if let Some(expected) = expected_ret {
        expected
    } else {
        return Err(StopReason::ReturnUnderflow { pc });
    };
    if let Some(expected_ret) = expected_ret {
        // Prefer tracked callsite when the stack return is clearly invalid.
        if ret != expected_ret && memory.contains_addr(expected_ret) && !memory.contains_addr(ret) {
            ret = expected_ret;
        }
    }
    if ret == u32::MAX {
        return Err(StopReason::EntryReturn { pc });
    }
    state.pc = ret;
    Ok(true)
}

fn execute_link_unlk(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // LINK An,#disp16
    if (word & 0xFFF8) == 0x4E50 {
        let an = (word & 0x0007) as usize;
        let Some(disp16) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let disp = (disp16 as i16) as i32;
        let old_an = state.a[an];
        state.frame_stack.push(old_an);
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], old_an);
        state.a[an] = state.a[7];
        state.a[7] = add_signed_u32(state.a[7], disp);
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }

    // UNLK An
    if (word & 0xFFF8) == 0x4E58 {
        let an = (word & 0x0007) as usize;
        let frame = state.a[an];
        if frame == 0 || !memory.contains_addr(frame) {
            // Permissive fallback for malformed frames: restore A6 from our
            // tracked frame history but do not smash SP.
            state.a[an] = state.frame_stack.pop().unwrap_or(state.a[an]);
            state.pc = pc.saturating_add(2);
            return Ok(true);
        }
        state.a[7] = frame;
        let mut popped_frame_stack = false;
        let old_an = if let Some(v) = memory.read_u32_be(state.a[7]) {
            v
        } else {
            popped_frame_stack = true;
            state.frame_stack.pop().unwrap_or(0)
        };
        state.a[an] = old_an;
        state.a[7] = state.a[7].wrapping_add(4);
        if !popped_frame_stack && !state.frame_stack.is_empty() {
            let _ = state.frame_stack.pop();
        }
        state.pc = pc.saturating_add(2);
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
    if execute_jmp(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_jsr(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_rts(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_link_unlk(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}
