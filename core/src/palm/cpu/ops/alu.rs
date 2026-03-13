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

fn set_ccr_nz(state: &mut CpuState68k, negative: bool, zero: bool) {
    state.sr &= !0x000F;
    if negative {
        state.sr |= 0x0008;
    }
    if zero {
        state.sr |= 0x0004;
    }
}

fn set_ccr_sub(state: &mut CpuState68k, src: u32, dst: u32, res: u32, bits: u32) {
    let sign = 1u32 << (bits - 1);
    let mask = if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    let s = src & mask;
    let d = dst & mask;
    let r = res & mask;
    let n = (r & sign) != 0;
    let z = r == 0;
    let v = ((d ^ s) & (d ^ r) & sign) != 0;
    let c = s > d;
    state.sr &= !0x000F;
    if n {
        state.sr |= 0x0008;
    }
    if z {
        state.sr |= 0x0004;
    }
    if v {
        state.sr |= 0x0002;
    }
    if c {
        state.sr |= 0x0001;
    }
}

fn set_ccr_add(state: &mut CpuState68k, src: u32, dst: u32, res: u32, bits: u32) {
    let sign = 1u32 << (bits - 1);
    let mask = if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    let s = src & mask;
    let d = dst & mask;
    let r = res & mask;
    let n = (r & sign) != 0;
    let z = r == 0;
    let v = ((!(d ^ s)) & (d ^ r) & sign) != 0;
    let c = (d as u64 + s as u64) > (mask as u64);
    state.sr &= !0x000F;
    if n {
        state.sr |= 0x0008;
    }
    if z {
        state.sr |= 0x0004;
    }
    if v {
        state.sr |= 0x0002;
    }
    if c {
        state.sr |= 0x0001;
    }
}

fn execute_clr_tst(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xFFC0) != 0x4200
        && (word & 0xFFC0) != 0x4240
        && (word & 0xFFC0) != 0x4280
        && (word & 0xFFC0) != 0x4A00
        && (word & 0xFFC0) != 0x4A40
        && (word & 0xFFC0) != 0x4A80
    {
        return Ok(false);
    }

    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let size_bits = (word >> 6) & 0x0003;
    let is_clr = (word & 0xFF00) == 0x4200;
    let mut instr_len = 2u32;

    if mode == 0 {
        if is_clr {
            match size_bits {
                0 => state.d[reg] &= 0xFFFF_FF00,
                1 => state.d[reg] &= 0xFFFF_0000,
                _ => state.d[reg] = 0,
            }
            set_ccr_nz(state, false, true);
        } else {
            match size_bits {
                0 => {
                    let v = (state.d[reg] & 0xFF) as u8;
                    set_ccr_nz(state, (v & 0x80) != 0, v == 0);
                }
                1 => {
                    let v = (state.d[reg] & 0xFFFF) as u16;
                    set_ccr_nz(state, (v & 0x8000) != 0, v == 0);
                }
                _ => {
                    let v = state.d[reg];
                    set_ccr_nz(state, (v & 0x8000_0000) != 0, v == 0);
                }
            }
        }
    } else if mode == 4 {
        let an = reg;
        if is_clr {
            match size_bits {
                0 => {
                    let dec = if an == 7 { 2 } else { 1 };
                    state.a[an] = state.a[an].wrapping_sub(dec);
                    let _ = memory.write_u8(state.a[an], 0);
                }
                1 => {
                    state.a[an] = state.a[an].wrapping_sub(2);
                    let _ = memory.write_u16_be(state.a[an], 0);
                }
                _ => {
                    state.a[an] = state.a[an].wrapping_sub(4);
                    let _ = memory.write_u32_be(state.a[an], 0);
                }
            }
            set_ccr_nz(state, false, true);
        }
    } else if mode == 2 {
        let an = reg;
        if is_clr {
            match size_bits {
                0 => {
                    let _ = memory.write_u8(state.a[an], 0);
                }
                1 => {
                    let _ = memory.write_u16_be(state.a[an], 0);
                }
                _ => {
                    let _ = memory.write_u32_be(state.a[an], 0);
                }
            }
            set_ccr_nz(state, false, true);
        }
    } else if mode == 5 {
        let Some(disp16) = memory.read_u16_be(pc + 2) else {
            return Err(StopReason::OutOfBounds { pc });
        };
        instr_len = 4;
        let ea = add_signed_u32(state.a[reg], (disp16 as i16) as i32);
        if is_clr {
            match size_bits {
                0 => {
                    let _ = memory.write_u8(ea, 0);
                }
                1 => {
                    let _ = memory.write_u16_be(ea, 0);
                }
                _ => {
                    let _ = memory.write_u32_be(ea, 0);
                }
            }
            set_ccr_nz(state, false, true);
        } else {
            match size_bits {
                0 => {
                    let v = memory.read_u8(ea).unwrap_or(0);
                    set_ccr_nz(state, (v & 0x80) != 0, v == 0);
                }
                1 => {
                    let v = memory.read_u16_be(ea).unwrap_or(0);
                    set_ccr_nz(state, (v & 0x8000) != 0, v == 0);
                }
                _ => {
                    let v = memory.read_u32_be(ea).unwrap_or(0);
                    set_ccr_nz(state, (v & 0x8000_0000) != 0, v == 0);
                }
            }
        }
    }

    state.pc = pc.saturating_add(instr_len);
    Ok(true)
}

fn execute_neg(word: u16, pc: u32, state: &mut CpuState68k) -> Result<bool, StopReason> {
    if (word & 0xFFC0) != 0x4400 && (word & 0xFFC0) != 0x4440 && (word & 0xFFC0) != 0x4480 {
        return Ok(false);
    }

    let dn = (word & 0x0007) as usize;
    match word & 0xFF00 {
        0x4400 => {
            let v = (state.d[dn] & 0xFF) as u8;
            let r = (0u8).wrapping_sub(v);
            state.d[dn] = (state.d[dn] & 0xFFFF_FF00) | (r as u32);
            set_ccr_nz(state, (r & 0x80) != 0, r == 0);
        }
        0x4440 => {
            let v = (state.d[dn] & 0xFFFF) as u16;
            let r = (0u16).wrapping_sub(v);
            state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (r as u32);
            set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
        }
        0x4480 => {
            let v = state.d[dn];
            let r = (0u32).wrapping_sub(v);
            state.d[dn] = r;
            set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
        }
        _ => {}
    }
    state.pc = pc.saturating_add(2);
    Ok(true)
}

fn execute_shift(word: u16, pc: u32, state: &mut CpuState68k) -> Result<bool, StopReason> {
    if (word & 0xF000) != 0xE000 {
        return Ok(false);
    }
    let size_bits = (word >> 6) & 0x0003;
    if size_bits == 0x0003 {
        return Ok(false);
    }

    let ir = (word & 0x0020) != 0;
    let op = (word >> 3) & 0x0003;
    let left = (word & 0x0100) != 0;
    let dest = (word & 0x0007) as usize;
    let count_src = ((word >> 9) & 0x0007) as usize;
    let mut count = if ir {
        (state.d[count_src] & 0x3F) as u32
    } else {
        let c = count_src as u32;
        if c == 0 { 8 } else { c }
    };
    let (mask, sign_bit, width) = match size_bits {
        0 => (0xFFu32, 0x80u32, 8u32),
        1 => (0xFFFFu32, 0x8000u32, 16u32),
        2 => (0xFFFF_FFFFu32, 0x8000_0000u32, 32u32),
        _ => (0, 0, 0),
    };
    if width == 0 {
        return Ok(false);
    }

    count %= 64;
    let cur = match size_bits {
        0 => state.d[dest] & 0xFF,
        1 => state.d[dest] & 0xFFFF,
        _ => state.d[dest],
    };
    let mut out = cur;
    if count != 0 {
        out = match op {
            0 => {
                if left {
                    (cur << count) & mask
                } else {
                    let ext = if (cur & sign_bit) != 0 { cur | (!mask) } else { cur };
                    (ext as i32 >> count) as u32 & mask
                }
            }
            1 => {
                if left {
                    (cur << count) & mask
                } else {
                    (cur >> count) & mask
                }
            }
            _ => cur,
        };
    }

    match size_bits {
        0 => state.d[dest] = (state.d[dest] & 0xFFFF_FF00) | (out & 0xFF),
        1 => state.d[dest] = (state.d[dest] & 0xFFFF_0000) | (out & 0xFFFF),
        _ => state.d[dest] = out,
    }
    set_ccr_nz(state, (out & sign_bit) != 0, out == 0);
    state.pc = pc.saturating_add(2);
    Ok(true)
}

fn execute_immediate(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let base = word & 0xFF00;
    let is_imm_op = matches!(base, 0x0000 | 0x0200 | 0x0400 | 0x0600 | 0x0A00 | 0x0C00);
    if !is_imm_op {
        return Ok(false);
    }

    let size_bits = (word >> 6) & 0x0003;
    let imm_words = match size_bits {
        0 | 1 => 1,
        2 => 2,
        3 => 1,
        _ => 0,
    };
    if imm_words == 0 {
        return Ok(false);
    }

    let mode = (word >> 3) & 0x0007;
    let reg = word & 0x0007;

    if base == 0x0C00 {
        // CMPI #imm,<ea>: compute <ea> - imm and set CCR.
        let mut ext_pc = pc.saturating_add(2);
        let size_bytes = match size_bits {
            0 => 1u32,
            1 | 3 => 2u32,
            2 => 4u32,
            _ => 0u32,
        };
        if size_bytes != 0 {
            let imm = match size_bytes {
                1 => match memory.read_u16_be(ext_pc) {
                    Some(v) => {
                        ext_pc = ext_pc.saturating_add(2);
                        (v & 0x00FF) as u32
                    }
                    None => return Err(StopReason::OutOfBounds { pc }),
                },
                2 => match memory.read_u16_be(ext_pc) {
                    Some(v) => {
                        ext_pc = ext_pc.saturating_add(2);
                        v as u32
                    }
                    None => return Err(StopReason::OutOfBounds { pc }),
                },
                _ => match memory.read_u32_be(ext_pc) {
                    Some(v) => {
                        ext_pc = ext_pc.saturating_add(4);
                        v
                    }
                    None => return Err(StopReason::OutOfBounds { pc }),
                },
            };
            let dst = match mode {
                0 => Some(state.d[reg as usize]),
                1 => Some(state.a[reg as usize]),
                2 => {
                    let addr = state.a[reg as usize];
                    match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    }
                }
                3 => {
                    let an = reg as usize;
                    let addr = state.a[an];
                    let v = match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    };
                    let inc = if size_bytes == 1 && an == 7 { 2 } else { size_bytes };
                    state.a[an] = state.a[an].wrapping_add(inc);
                    v
                }
                4 => {
                    let an = reg as usize;
                    let dec = if size_bytes == 1 && an == 7 { 2 } else { size_bytes };
                    state.a[an] = state.a[an].wrapping_sub(dec);
                    let addr = state.a[an];
                    match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    }
                }
                5 => {
                    let disp_w = match memory.read_u16_be(ext_pc) {
                        Some(v) => v,
                        None => return Err(StopReason::OutOfBounds { pc }),
                    };
                    ext_pc = ext_pc.saturating_add(2);
                    let addr = add_signed_u32(state.a[reg as usize], disp_w as i16 as i32);
                    match size_bytes {
                        1 => memory.read_u8(addr).map(u32::from),
                        2 => memory.read_u16_be(addr).map(u32::from),
                        _ => memory.read_u32_be(addr),
                    }
                }
                7 => match reg {
                    0 => {
                        let aw_w = match memory.read_u16_be(ext_pc) {
                            Some(v) => v,
                            None => return Err(StopReason::OutOfBounds { pc }),
                        };
                        ext_pc = ext_pc.saturating_add(2);
                        let addr = (aw_w as i16 as i32) as u32;
                        match size_bytes {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        }
                    }
                    1 => {
                        let addr = match memory.read_u32_be(ext_pc) {
                            Some(v) => v,
                            None => return Err(StopReason::OutOfBounds { pc }),
                        };
                        ext_pc = ext_pc.saturating_add(4);
                        match size_bytes {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        }
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some(dst_raw) = dst {
                let (imm_v, dst_v, bits) = match size_bytes {
                    1 => (imm & 0xFF, dst_raw & 0xFF, 8u32),
                    2 => (imm & 0xFFFF, dst_raw & 0xFFFF, 16u32),
                    _ => (imm, dst_raw, 32u32),
                };
                let res = dst_v.wrapping_sub(imm_v);
                set_ccr_sub(state, imm_v, dst_v, res, bits);
            }
            state.pc = ext_pc;
            return Ok(true);
        }
    }

    // Execute immediate ops to memory for simple address-indirect forms.
    // ROM apps use `ADDI.L #imm,(A7)` in the code#1 startup veneer.
    if mode == 2 && matches!(base, 0x0000 | 0x0200 | 0x0400 | 0x0600 | 0x0A00) {
        let size_bytes = match size_bits {
            0 => 1u32,
            1 | 3 => 2u32,
            2 => 4u32,
            _ => 0u32,
        };
        if size_bytes != 0 {
            let mut ext_pc = pc.saturating_add(2);
            let imm = match size_bytes {
                1 => match memory.read_u16_be(ext_pc) {
                    Some(v) => {
                        ext_pc = ext_pc.saturating_add(2);
                        (v & 0x00FF) as u32
                    }
                    None => return Err(StopReason::OutOfBounds { pc }),
                },
                2 => match memory.read_u16_be(ext_pc) {
                    Some(v) => {
                        ext_pc = ext_pc.saturating_add(2);
                        v as u32
                    }
                    None => return Err(StopReason::OutOfBounds { pc }),
                },
                _ => match memory.read_u32_be(ext_pc) {
                    Some(v) => {
                        ext_pc = ext_pc.saturating_add(4);
                        v
                    }
                    None => return Err(StopReason::OutOfBounds { pc }),
                },
            };
            let addr = state.a[reg as usize];
            let cur = match size_bytes {
                1 => memory.read_u8(addr).map(u32::from),
                2 => memory.read_u16_be(addr).map(u32::from),
                _ => memory.read_u32_be(addr),
            }
            .ok_or(StopReason::OutOfBounds { pc })?;
            let (imm_v, cur_v, bits, out) = match size_bytes {
                1 => {
                    let i = imm & 0xFF;
                    let c = cur & 0xFF;
                    let o = match base {
                        0x0000 => c | i,
                        0x0200 => c & i,
                        0x0400 => c.wrapping_sub(i) & 0xFF,
                        0x0600 => c.wrapping_add(i) & 0xFF,
                        0x0A00 => c ^ i,
                        _ => c,
                    };
                    (i, c, 8u32, o)
                }
                2 => {
                    let i = imm & 0xFFFF;
                    let c = cur & 0xFFFF;
                    let o = match base {
                        0x0000 => c | i,
                        0x0200 => c & i,
                        0x0400 => c.wrapping_sub(i) & 0xFFFF,
                        0x0600 => c.wrapping_add(i) & 0xFFFF,
                        0x0A00 => c ^ i,
                        _ => c,
                    };
                    (i, c, 16u32, o)
                }
                _ => {
                    let i = imm;
                    let c = cur;
                    let o = match base {
                        0x0000 => c | i,
                        0x0200 => c & i,
                        0x0400 => c.wrapping_sub(i),
                        0x0600 => c.wrapping_add(i),
                        0x0A00 => c ^ i,
                        _ => c,
                    };
                    (i, c, 32u32, o)
                }
            };
            let _ = match size_bytes {
                1 => memory.write_u8(addr, out as u8),
                2 => memory.write_u16_be(addr, out as u16),
                _ => memory.write_u32_be(addr, out),
            };
            match base {
                0x0400 => set_ccr_sub(state, imm_v, cur_v, out, bits),
                0x0600 => set_ccr_add(state, imm_v, cur_v, out, bits),
                _ => {
                    let sign = if bits == 8 {
                        (out & 0x80) != 0
                    } else if bits == 16 {
                        (out & 0x8000) != 0
                    } else {
                        (out & 0x8000_0000) != 0
                    };
                    set_ccr_nz(state, sign, out == 0);
                }
            }
            state.pc = ext_pc;
            return Ok(true);
        }
    }

    if mode == 0 {
        // Register-immediate ops on Dn used heavily by app logic.
        let dn = reg as usize;
        let apply_word = |state: &mut CpuState68k, v: u16| {
            state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (v as u32);
        };
        let apply_byte = |state: &mut CpuState68k, v: u8| {
            state.d[dn] = (state.d[dn] & 0xFFFF_FF00) | (v as u32);
        };
        if size_bits == 2 {
            if let Some(imm) = memory.read_u32_be(pc + 2) {
                let cur = state.d[dn];
                let out = match base {
                    0x0000 => cur | imm,
                    0x0400 => cur.wrapping_sub(imm),
                    0x0600 => cur.wrapping_add(imm),
                    0x0A00 => cur ^ imm,
                    _ => cur,
                };
                if matches!(base, 0x0000 | 0x0400 | 0x0600 | 0x0A00) {
                    state.d[dn] = out;
                    match base {
                        0x0400 => set_ccr_sub(state, imm, cur, out, 32),
                        0x0600 => set_ccr_add(state, imm, cur, out, 32),
                        _ => set_ccr_nz(state, (out & 0x8000_0000) != 0, out == 0),
                    }
                }
            }
        } else if let Some(immw) = memory.read_u16_be(pc + 2) {
            match size_bits {
                0 => {
                    let cur = (state.d[dn] & 0xFF) as u8;
                    let imm = (immw & 0x00FF) as u8;
                    let out = match base {
                        0x0000 => cur | imm,
                        0x0400 => cur.wrapping_sub(imm),
                        0x0600 => cur.wrapping_add(imm),
                        0x0A00 => cur ^ imm,
                        _ => cur,
                    };
                    if matches!(base, 0x0000 | 0x0400 | 0x0600 | 0x0A00) {
                        apply_byte(state, out);
                        match base {
                            0x0400 => set_ccr_sub(state, imm as u32, cur as u32, out as u32, 8),
                            0x0600 => set_ccr_add(state, imm as u32, cur as u32, out as u32, 8),
                            _ => set_ccr_nz(state, (out & 0x80) != 0, out == 0),
                        }
                    }
                }
                _ => {
                    let cur = (state.d[dn] & 0xFFFF) as u16;
                    let imm = immw;
                    let out = match base {
                        0x0000 => cur | imm,
                        0x0400 => cur.wrapping_sub(imm),
                        0x0600 => cur.wrapping_add(imm),
                        0x0A00 => cur ^ imm,
                        _ => cur,
                    };
                    if matches!(base, 0x0000 | 0x0400 | 0x0600 | 0x0A00) {
                        apply_word(state, out);
                        match base {
                            0x0400 => set_ccr_sub(state, imm as u32, cur as u32, out as u32, 16),
                            0x0600 => set_ccr_add(state, imm as u32, cur as u32, out as u32, 16),
                            _ => set_ccr_nz(state, (out & 0x8000) != 0, out == 0),
                        }
                    }
                }
            }
        }
    }

    // Minimal execution semantics for ANDI to Dn (used in Noah loop).
    if base == 0x0200 && mode == 0 {
        let dn = reg as usize;
        match size_bits {
            0 | 1 | 3 => {
                let Some(imm) = memory.read_u16_be(pc + 2) else {
                    return Err(StopReason::OutOfBounds { pc });
                };
                let val = (state.d[dn] as u16) & imm;
                state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (val as u32);
                set_ccr_nz(state, (val & 0x8000) != 0, val == 0);
            }
            2 => {
                let Some(imm) = memory.read_u32_be(pc + 2) else {
                    return Err(StopReason::OutOfBounds { pc });
                };
                let val = state.d[dn] & imm;
                state.d[dn] = val;
                set_ccr_nz(state, (val & 0x8000_0000) != 0, val == 0);
            }
            _ => {}
        }
    }

    if let Some(ea_words) = ea_ext_words(mode, reg) {
        let bytes = 2 + (imm_words * 2) + (ea_words * 2);
        state.pc = pc.saturating_add(bytes);
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
    if execute_clr_tst(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_neg(word, pc, state)? {
        return Ok(true);
    }
    if execute_shift(word, pc, state)? {
        return Ok(true);
    }
    if execute_immediate(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}
