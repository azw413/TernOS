use crate::prc_app::cpu::core::{CpuState68k, StopReason};
use crate::prc_app::cpu::memory::MemoryMap;

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

fn execute_pea(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    // PEA (An)
    if (word & 0xFFF8) == 0x4850 {
        let an = (word & 0x0007) as usize;
        let addr = state.a[an];
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], addr);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }

    // PEA (d8,An,Xn)
    if (word & 0xFFF8) == 0x4870 {
        let an = (word & 0x0007) as usize;
        let Some(ext) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
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
        let addr = add_signed_u32(state.a[an], disp8.saturating_add(idx));
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], addr);
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }

    // PEA (d16,An)
    if (word & 0xFFF8) == 0x4868 {
        let an = (word & 0x0007) as usize;
        let Some(disp16) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let addr = add_signed_u32(state.a[an], (disp16 as i16) as i32);
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], addr);
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }

    // PEA (d16,PC)
    if word == 0x487A {
        let Some(disp16) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let addr = add_signed_u32(pc.saturating_add(2), (disp16 as i16) as i32);
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], addr);
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }

    // PEA abs.l
    if word == 0x4879 {
        let Some(addr) = memory.read_u32_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], addr);
        state.pc = pc.saturating_add(6);
        return Ok(true);
    }

    // PEA abs.w
    if word == 0x4878 {
        let Some(aw) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        let addr = (aw as i16 as i32) as u32;
        state.a[7] = state.a[7].wrapping_sub(4);
        memory.write_u32_be(state.a[7], addr);
        state.pc = pc.saturating_add(4);
        return Ok(true);
    }

    Ok(false)
}

fn execute_swap_ext(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
) -> Result<bool, StopReason> {
    // SWAP Dn
    if (word & 0xFFF8) == 0x4840 {
        let dn = (word & 0x0007) as usize;
        let v = state.d[dn];
        let r = v.rotate_right(16);
        state.d[dn] = r;
        set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }

    // EXT.W Dn
    if (word & 0xFFF8) == 0x4880 {
        let dn = (word & 0x0007) as usize;
        let b = (state.d[dn] & 0xFF) as u8;
        let w = (b as i8 as i16) as u16;
        state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (w as u32);
        set_ccr_nz(state, (w & 0x8000) != 0, w == 0);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }

    // EXT.L Dn
    if (word & 0xFFF8) == 0x48C0 {
        let dn = (word & 0x0007) as usize;
        let w = (state.d[dn] & 0xFFFF) as u16;
        let l = (w as i16 as i32) as u32;
        state.d[dn] = l;
        set_ccr_nz(state, (l & 0x8000_0000) != 0, l == 0);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }

    Ok(false)
}

fn execute_moveq(word: u16, pc: u32, state: &mut CpuState68k) -> Result<bool, StopReason> {
    if (word & 0xF100) != 0x7000 {
        return Ok(false);
    }
    let dn = ((word >> 9) & 0x0007) as usize;
    let imm8 = (word & 0x00FF) as u8;
    let v = (imm8 as i8 as i32) as u32;
    state.d[dn] = v;
    set_ccr_nz(state, (v & 0x8000_0000) != 0, v == 0);
    state.pc = pc.saturating_add(2);
    Ok(true)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if execute_pea(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_swap_ext(word, pc, state)? {
        return Ok(true);
    }
    if execute_moveq(word, pc, state)? {
        return Ok(true);
    }
    Ok(false)
}
