use crate::prc_app::cpu::core::{CpuState68k, StopReason};
use crate::prc_app::cpu::memory::MemoryMap;

fn add_signed_u32(v: u32, delta: i32) -> u32 {
    if delta >= 0 {
        v.wrapping_add(delta as u32)
    } else {
        v.wrapping_sub((-delta) as u32)
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

fn bcd_add(src: u8, dst: u8, x: u8) -> (u8, bool) {
    let s = ((src >> 4) & 0x0F) * 10 + (src & 0x0F);
    let d = ((dst >> 4) & 0x0F) * 10 + (dst & 0x0F);
    let mut v = d as u16 + s as u16 + x as u16;
    let carry = v > 99;
    if carry {
        v -= 100;
    }
    let tens = ((v / 10) as u8) & 0x0F;
    let ones = ((v % 10) as u8) & 0x0F;
    ((tens << 4) | ones, carry)
}

fn bcd_sub(src: u8, dst: u8, x: u8) -> (u8, bool) {
    let s = ((src >> 4) & 0x0F) * 10 + (src & 0x0F);
    let d = ((dst >> 4) & 0x0F) * 10 + (dst & 0x0F);
    let sub = s as i16 + x as i16;
    let mut v = d as i16 - sub;
    let borrow = v < 0;
    if borrow {
        v += 100;
    }
    let vu = v as u16;
    let tens = ((vu / 10) as u8) & 0x0F;
    let ones = ((vu % 10) as u8) & 0x0F;
    ((tens << 4) | ones, borrow)
}

fn execute_exg(word: u16, pc: u32, state: &mut CpuState68k) -> Result<bool, StopReason> {
    let rx = ((word >> 9) & 0x0007) as usize;
    let ry = (word & 0x0007) as usize;
    if (word & 0xF1F8) == 0xC140 {
        state.d.swap(rx, ry);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }
    if (word & 0xF1F8) == 0xC148 {
        state.a.swap(rx, ry);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }
    if (word & 0xF1F8) == 0xC188 {
        core::mem::swap(&mut state.d[rx], &mut state.a[ry]);
        state.pc = pc.saturating_add(2);
        return Ok(true);
    }
    Ok(false)
}

fn execute_addx_subx(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let is_addx = (word & 0xF130) == 0xD100;
    let is_subx = (word & 0xF130) == 0x9100;
    if !(is_addx || is_subx) {
        return Ok(false);
    }
    let size = (word >> 6) & 0x0003;
    if size == 0x0003 {
        return Ok(false);
    }
    let bytes = match size {
        0 => 1u32,
        1 => 2u32,
        2 => 4u32,
        _ => unreachable!(),
    };
    let rm_predec = (word & 0x0008) != 0;
    let dst = ((word >> 9) & 0x0007) as usize;
    let src = (word & 0x0007) as usize;
    let x = if (state.sr & 0x0010) != 0 { 1u32 } else { 0u32 };
    let mask = if bytes == 4 {
        u32::MAX
    } else {
        (1u32 << (bytes * 8)) - 1
    };
    let sign = 1u32 << (bytes * 8 - 1);
    let prev_z = (state.sr & 0x0004) != 0;

    let (src_v, dst_v) = if !rm_predec {
        let s = state.d[src] & mask;
        let d = state.d[dst] & mask;
        (s, d)
    } else {
        let dec = if bytes == 1 && src == 7 { 2 } else { bytes };
        state.a[src] = state.a[src].wrapping_sub(dec);
        let decd = if bytes == 1 && dst == 7 { 2 } else { bytes };
        state.a[dst] = state.a[dst].wrapping_sub(decd);
        let s = match bytes {
            1 => memory.read_u8(state.a[src]).map(u32::from).unwrap_or(0),
            2 => memory.read_u16_be(state.a[src]).map(u32::from).unwrap_or(0),
            _ => memory.read_u32_be(state.a[src]).unwrap_or(0),
        } & mask;
        let d = match bytes {
            1 => memory.read_u8(state.a[dst]).map(u32::from).unwrap_or(0),
            2 => memory.read_u16_be(state.a[dst]).map(u32::from).unwrap_or(0),
            _ => memory.read_u32_be(state.a[dst]).unwrap_or(0),
        } & mask;
        (s, d)
    };

    let (res, carry, overflow) = if is_addx {
        let sum = dst_v as u64 + src_v as u64 + x as u64;
        let r = (sum as u32) & mask;
        let c = sum > (mask as u64);
        let v = ((!(dst_v ^ src_v)) & (dst_v ^ r) & sign) != 0;
        (r, c, v)
    } else {
        let sub = src_v.wrapping_add(x);
        let r = dst_v.wrapping_sub(sub) & mask;
        let c = sub > dst_v;
        let v = ((dst_v ^ sub) & (dst_v ^ r) & sign) != 0;
        (r, c, v)
    };

    if !rm_predec {
        match bytes {
            1 => state.d[dst] = (state.d[dst] & 0xFFFF_FF00) | res,
            2 => state.d[dst] = (state.d[dst] & 0xFFFF_0000) | res,
            _ => state.d[dst] = res,
        }
    } else {
        match bytes {
            1 => {
                memory.write_u8(state.a[dst], (res & 0xFF) as u8);
            }
            2 => {
                memory.write_u16_be(state.a[dst], (res & 0xFFFF) as u16);
            }
            _ => {
                memory.write_u32_be(state.a[dst], res);
            }
        };
    }

    // Preserve upper status; update X/N/Z/V/C with ADDX/SUBX Z chaining.
    let mut sr = state.sr & !0x001F;
    if carry {
        sr |= 0x0011; // X + C
    }
    if (res & sign) != 0 {
        sr |= 0x0008;
    }
    if res == 0 {
        if prev_z {
            sr |= 0x0004;
        }
    } else {
        sr &= !0x0004;
    }
    if overflow {
        sr |= 0x0002;
    }
    state.sr = sr;
    state.pc = pc.saturating_add(2);
    Ok(true)
}

fn execute_cmpm(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xF138) != 0xB108 {
        return Ok(false);
    }
    let size = (word >> 6) & 0x0003;
    let bytes = match size {
        0 => 1u32,
        1 => 2u32,
        2 => 4u32,
        _ => return Ok(false),
    };
    let ax = ((word >> 9) & 0x0007) as usize;
    let ay = (word & 0x0007) as usize;

    let src = match bytes {
        1 => memory.read_u8(state.a[ay]).map(u32::from).unwrap_or(0),
        2 => memory.read_u16_be(state.a[ay]).map(u32::from).unwrap_or(0),
        _ => memory.read_u32_be(state.a[ay]).unwrap_or(0),
    };
    let dst = match bytes {
        1 => memory.read_u8(state.a[ax]).map(u32::from).unwrap_or(0),
        2 => memory.read_u16_be(state.a[ax]).map(u32::from).unwrap_or(0),
        _ => memory.read_u32_be(state.a[ax]).unwrap_or(0),
    };
    let inc_y = if bytes == 1 && ay == 7 { 2 } else { bytes };
    let inc_x = if bytes == 1 && ax == 7 { 2 } else { bytes };
    state.a[ay] = state.a[ay].wrapping_add(inc_y);
    state.a[ax] = state.a[ax].wrapping_add(inc_x);

    let bits = bytes * 8;
    let mask = if bits == 32 { u32::MAX } else { (1u32 << bits) - 1 };
    let s = src & mask;
    let d = dst & mask;
    let r = d.wrapping_sub(s) & mask;
    set_ccr_sub(state, s, d, r, bits);
    state.pc = pc.saturating_add(2);
    Ok(true)
}

fn execute_abcd_sbcd(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    let is_abcd = (word & 0xF1F0) == 0xC100;
    let is_sbcd = (word & 0xF1F0) == 0x8100;
    if !(is_abcd || is_sbcd) {
        return Ok(false);
    }
    let mem_predec = (word & 0x0008) != 0;
    let rx = ((word >> 9) & 0x0007) as usize;
    let ry = (word & 0x0007) as usize;
    let x = if (state.sr & 0x0010) != 0 { 1u8 } else { 0u8 };
    let prev_z = (state.sr & 0x0004) != 0;

    let (src, dst) = if !mem_predec {
        ((state.d[ry] & 0xFF) as u8, (state.d[rx] & 0xFF) as u8)
    } else {
        let dec_y = if ry == 7 { 2 } else { 1 };
        let dec_x = if rx == 7 { 2 } else { 1 };
        state.a[ry] = state.a[ry].wrapping_sub(dec_y);
        state.a[rx] = state.a[rx].wrapping_sub(dec_x);
        (
            memory.read_u8(state.a[ry]).unwrap_or(0),
            memory.read_u8(state.a[rx]).unwrap_or(0),
        )
    };

    let (out, carry) = if is_abcd {
        bcd_add(src, dst, x)
    } else {
        bcd_sub(src, dst, x)
    };

    if !mem_predec {
        state.d[rx] = (state.d[rx] & 0xFFFF_FF00) | (out as u32);
    } else {
        memory.write_u8(state.a[rx], out);
    }

    // X/C + N/Z (Z chained), clear V.
    let mut sr = state.sr & !0x001F;
    if carry {
        sr |= 0x0011;
    }
    if (out & 0x80) != 0 {
        sr |= 0x0008;
    }
    if out == 0 {
        if prev_z {
            sr |= 0x0004;
        }
    } else {
        sr &= !0x0004;
    }
    state.sr = sr;
    state.pc = pc.saturating_add(2);
    Ok(true)
}

fn execute_nbcd(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if (word & 0xFFC0) != 0x4800 {
        return Ok(false);
    }
    let mode = (word >> 3) & 0x0007;
    let reg = (word & 0x0007) as usize;
    let x = if (state.sr & 0x0010) != 0 { 1u8 } else { 0u8 };
    let prev_z = (state.sr & 0x0004) != 0;

    let (dst, advance) = match mode {
        0 => ((state.d[reg] & 0xFF) as u8, 2u32),
        2 => (memory.read_u8(state.a[reg]).unwrap_or(0), 2u32),
        3 => {
            let v = memory.read_u8(state.a[reg]).unwrap_or(0);
            state.a[reg] = state.a[reg].wrapping_add(if reg == 7 { 2 } else { 1 });
            (v, 2u32)
        }
        4 => {
            state.a[reg] = state.a[reg].wrapping_sub(if reg == 7 { 2 } else { 1 });
            (memory.read_u8(state.a[reg]).unwrap_or(0), 2u32)
        }
        5 => {
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                return Err(StopReason::OutOfBounds { pc });
            };
            let addr = add_signed_u32(state.a[reg], (disp16 as i16) as i32);
            (memory.read_u8(addr).unwrap_or(0), 4u32)
        }
        _ => {
            state.pc = pc.saturating_add(2);
            return Ok(true);
        }
    };
    let (out, borrow) = bcd_sub(dst, 0, x);
    match mode {
        0 => state.d[reg] = (state.d[reg] & 0xFFFF_FF00) | (out as u32),
        2 => {
            memory.write_u8(state.a[reg], out);
        }
        3 => {
            memory.write_u8(state.a[reg].wrapping_sub(if reg == 7 { 2 } else { 1 }), out);
        }
        4 => {
            memory.write_u8(state.a[reg], out);
        }
        5 => {
            let disp16 = memory.read_u16_be(pc + 2).unwrap_or(0);
            let addr = add_signed_u32(state.a[reg], (disp16 as i16) as i32);
            memory.write_u8(addr, out);
        }
        _ => {}
    }
    let mut sr = state.sr & !0x001F;
    if borrow {
        sr |= 0x0011;
    }
    if (out & 0x80) != 0 {
        sr |= 0x0008;
    }
    if out == 0 {
        if prev_z {
            sr |= 0x0004;
        }
    } else {
        sr &= !0x0004;
    }
    state.sr = sr;
    state.pc = pc.saturating_add(advance);
    Ok(true)
}

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
    if execute_exg(word, pc, state)? {
        return Ok(true);
    }
    if execute_addx_subx(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_cmpm(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_abcd_sbcd(word, pc, state, memory)? {
        return Ok(true);
    }
    if execute_nbcd(word, pc, state, memory)? {
        return Ok(true);
    }
    Ok(false)
}
