use crate::palm::cpu::{core::CpuState68k, memory::MemoryMap};
use crate::palm::cpu::core::StopReason;
use crate::palm::cpu::ops::cond::{sr_cond_true, target_pc};

fn branch_disp(memory: &MemoryMap, pc: u32, op: u16) -> Option<(i32, u32)> {
    let disp8 = (op & 0x00FF) as u8;
    if disp8 == 0 {
        let ext = memory.read_u16_be(pc + 2)? as i16 as i32;
        Some((ext, 4))
    } else {
        Some(((disp8 as i8) as i32, 2))
    }
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0x6000 {
        return Ok(false);
    }

    let cond = ((word >> 8) & 0x000F) as u8;
    let (disp, instr_len) =
        branch_disp(memory, pc, word).ok_or(StopReason::OutOfBounds { pc })?;

    // 68k branch displacement is based off PC+2 (the extension-word address).
    // This matters for Bcc/BSR with 16-bit displacement.
    let base_pc = pc.saturating_add(2);
    let next_pc = pc.saturating_add(instr_len);

    let taken = if cond == 0x0 {
        true // BRA
    } else if cond == 0x1 {
        // BSR
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], next_pc);
        state.call_stack.push(next_pc);
        true
    } else {
        sr_cond_true(state.sr, cond)
    };

    if taken {
        state.pc = target_pc(base_pc, disp).ok_or(StopReason::OutOfBounds { pc })?;
    } else {
        state.pc = next_pc;
    }

    Ok(true)
}
