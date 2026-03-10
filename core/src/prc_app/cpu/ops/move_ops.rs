use crate::prc_app::cpu::core::{CpuState68k, StopReason};
use crate::prc_app::cpu::memory::MemoryMap;

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

pub fn execute(
    word: u16,
    pc: u32,
    state: &mut CpuState68k,
    memory: &mut MemoryMap,
) -> Result<bool, StopReason> {
        // MOVE.[B/W/L] generic length handling (covers many data motion opcodes).
        {
            let sz = (word >> 12) & 0x0003;
            // MOVE encodings live in opcode class 00 (top two bits clear) with
            // high nibble 0x1/0x2/0x3 selecting size. Without the class check,
            // opcodes like 0x5xxx (ADDQ/SUBQ/DBcc) get misdecoded as MOVE.
            if (word & 0xC000) == 0 && (sz == 0x1 || sz == 0x2 || sz == 0x3) {
                let src_mode = (word >> 3) & 0x0007;
                let src_reg = word & 0x0007;
                let dst_mode = (word >> 6) & 0x0007;
                let dst_reg = (word >> 9) & 0x0007;

                let src_words = if src_mode == 7 && src_reg == 4 {
                    // Immediate source: byte/word => 1 ext word, long => 2.
                    if sz == 0x2 { 2 } else { 1 }
                } else {
                    ea_ext_words(src_mode, src_reg).unwrap_or(0)
                };
                let dst_words = if dst_mode == 7 && dst_reg == 4 {
                    0
                } else {
                    ea_ext_words(dst_mode, dst_reg).unwrap_or(0)
                };
                let size_bytes = match sz {
                    0x1 => 1u32,
                    0x3 => 2u32,
                    0x2 => 4u32,
                    _ => 0u32,
                };
                // Minimal execution semantics for MOVE Dn -> Dn.
                if src_mode == 0 && dst_mode == 0 {
                    let src = src_reg as usize;
                    let dst = dst_reg as usize;
                    match sz {
                        0x1 => {
                            let v = (state.d[src] & 0xFF) as u8;
                            state.d[dst] = (state.d[dst] & 0xFFFF_FF00) | (v as u32);
                            set_ccr_nz(state, (v & 0x80) != 0, v == 0);
                        }
                        0x3 => {
                            let v = (state.d[src] & 0xFFFF) as u16;
                            state.d[dst] = (state.d[dst] & 0xFFFF_0000) | (v as u32);
                            set_ccr_nz(state, (v & 0x8000) != 0, v == 0);
                        }
                        0x2 => {
                            let v = state.d[src];
                            state.d[dst] = v;
                            set_ccr_nz(state, (v & 0x8000_0000) != 0, v == 0);
                        }
                        _ => {}
                    }
                } else if dst_mode == 1 {
                    // MOVEA.[W/L] <ea>,An
                    let an = dst_reg as usize;
                    match (sz, src_mode) {
                        (0x3, 0) => {
                            // MOVEA.W Dn,An
                            let src = src_reg as usize;
                            let w = (state.d[src] & 0xFFFF) as u16;
                            state.a[an] = (w as i16 as i32) as u32;
                        }
                        (0x2, 0) => {
                            // MOVEA.L Dn,An
                            let src = src_reg as usize;
                            state.a[an] = state.d[src];
                        }
                        (0x3, 1) => {
                            // MOVEA.W An,Am
                            let src = src_reg as usize;
                            let w = (state.a[src] & 0xFFFF) as u16;
                            state.a[an] = (w as i16 as i32) as u32;
                        }
                        (0x2, 1) => {
                            // MOVEA.L An,Am
                            let src = src_reg as usize;
                            state.a[an] = state.a[src];
                        }
                        (0x3, 2) => {
                            // MOVEA.W (An),Am
                            let src_an = src_reg as usize;
                            if let Some(v) = memory.read_u16_be(state.a[src_an]) {
                                state.a[an] = (v as i16 as i32) as u32;
                            }
                        }
                        (0x2, 2) => {
                            // MOVEA.L (An),Am
                            let src_an = src_reg as usize;
                            if let Some(v) = memory.read_u32_be(state.a[src_an]) {
                                state.a[an] = v;
                            }
                        }
                        (0x3, 3) => {
                            // MOVEA.W (An)+,Am
                            let src_an = src_reg as usize;
                            if let Some(v) = memory.read_u16_be(state.a[src_an]) {
                                state.a[an] = (v as i16 as i32) as u32;
                                state.a[src_an] = state.a[src_an].wrapping_add(2);
                            }
                        }
                        (0x2, 3) => {
                            // MOVEA.L (An)+,Am
                            let src_an = src_reg as usize;
                            if let Some(v) = memory.read_u32_be(state.a[src_an]) {
                                state.a[an] = v;
                                state.a[src_an] = state.a[src_an].wrapping_add(4);
                            }
                        }
                        (0x3, 5) => {
                            // MOVEA.W (d16,An),Am
                            let src_an = src_reg as usize;
                            if let Some(disp16) = memory.read_u16_be(pc + 2) {
                                let addr = add_signed_u32(state.a[src_an], (disp16 as i16) as i32);
                                if let Some(v) = memory.read_u16_be(addr) {
                                    state.a[an] = (v as i16 as i32) as u32;
                                }
                            }
                        }
                        (0x2, 5) => {
                            // MOVEA.L (d16,An),Am
                            let src_an = src_reg as usize;
                            if let Some(disp16) = memory.read_u16_be(pc + 2) {
                                let addr = add_signed_u32(state.a[src_an], (disp16 as i16) as i32);
                                if let Some(v) = memory.read_u32_be(addr) {
                                    state.a[an] = v;
                                }
                            }
                        }
                        (0x3, 7) if src_reg == 0 => {
                            // MOVEA.W (abs.w),Am
                            if let Some(aw) = memory.read_u16_be(pc + 2) {
                                let addr = (aw as i16 as i32) as u32;
                                if let Some(v) = memory.read_u16_be(addr) {
                                    state.a[an] = (v as i16 as i32) as u32;
                                }
                            }
                        }
                        (0x2, 7) if src_reg == 0 => {
                            // MOVEA.L (abs.w),Am
                            if let Some(aw) = memory.read_u16_be(pc + 2) {
                                let addr = (aw as i16 as i32) as u32;
                                if let Some(v) = memory.read_u32_be(addr) {
                                    state.a[an] = v;
                                }
                            }
                        }
                        (0x3, 7) if src_reg == 1 => {
                            // MOVEA.W (abs.l),Am
                            if let Some(addr) = memory.read_u32_be(pc + 2) {
                                if let Some(v) = memory.read_u16_be(addr) {
                                    state.a[an] = (v as i16 as i32) as u32;
                                }
                            }
                        }
                        (0x2, 7) if src_reg == 1 => {
                            // MOVEA.L (abs.l),Am
                            if let Some(addr) = memory.read_u32_be(pc + 2) {
                                if let Some(v) = memory.read_u32_be(addr) {
                                    state.a[an] = v;
                                }
                            }
                        }
                        (0x3, 7) if src_reg == 2 => {
                            // MOVEA.W (d16,PC),Am
                            if let Some(disp16) = memory.read_u16_be(pc + 2) {
                                // For d16(PC), 68k uses the extension-word address as PC base.
                                let addr = add_signed_u32(pc.saturating_add(2), (disp16 as i16) as i32);
                                if let Some(v) = memory.read_u16_be(addr) {
                                    state.a[an] = (v as i16 as i32) as u32;
                                }
                            }
                        }
                        (0x2, 7) if src_reg == 2 => {
                            // MOVEA.L (d16,PC),Am
                            if let Some(disp16) = memory.read_u16_be(pc + 2) {
                                // For d16(PC), 68k uses the extension-word address as PC base.
                                let addr = add_signed_u32(pc.saturating_add(2), (disp16 as i16) as i32);
                                if let Some(v) = memory.read_u32_be(addr) {
                                    state.a[an] = v;
                                }
                            }
                        }
                        _ => {}
                    }
                } else if sz == 0x1 && src_mode == 3 && dst_mode == 0 {
                    // MOVE.B (An)+,Dn
                    let an = src_reg as usize;
                    let dst = dst_reg as usize;
                    let addr = state.a[an];
                    let v = memory.read_u8(addr).unwrap_or(0);
                    state.d[dst] = (state.d[dst] & 0xFFFF_FF00) | (v as u32);
                    state.a[an] = state.a[an].wrapping_add(1);
                    set_ccr_nz(state, (v & 0x80) != 0, v == 0);
                } else {
                    // Broader MOVE support for common startup glue patterns.
                    let src_reg_idx = src_reg as usize;
                    let dst_reg_idx = dst_reg as usize;

                    let mut src_ext_pc = pc.saturating_add(2);
                    let mut dst_ext_pc =
                        pc.saturating_add(2 + (ea_ext_words(src_mode, src_reg).unwrap_or(0) * 2));

                    let read_src = |state: &mut CpuState68k,
                                    memory: &MemoryMap,
                                    mode: u16,
                                    reg: usize,
                                    ext_pc: &mut u32,
                                    bytes: u32|
                     -> Option<u32> {
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
                            1 => Some(state.a[reg]),
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
                                // Byte postinc on A7 is 2.
                                let inc = if bytes == 1 && reg == 7 { 2 } else { bytes };
                                state.a[reg] = state.a[reg].wrapping_add(inc);
                                v
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
                                3 => {
                                    let ext = memory.read_u16_be(*ext_pc)?;
                                    *ext_pc = ext_pc.saturating_add(2);
                                    // Approximate PC-relative indexed base at post-extension PC.
                                    let addr = indexed_addr(*ext_pc, ext, state);
                                    match bytes {
                                        1 => memory.read_u8(addr).map(u32::from),
                                        2 => memory.read_u16_be(addr).map(u32::from),
                                        4 => memory.read_u32_be(addr),
                                        _ => None,
                                    }
                                }
                                4 => {
                                    // immediate
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
                    };

                    let write_dst = |state: &mut CpuState68k,
                                     memory: &mut MemoryMap,
                                     mode: u16,
                                     reg: usize,
                                     ext_pc: &mut u32,
                                     bytes: u32,
                                     value: u32| {
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
                            0 => match bytes {
                                1 => {
                                    state.d[reg] = (state.d[reg] & 0xFFFF_FF00) | (value & 0xFF);
                                }
                                2 => {
                                    state.d[reg] = (state.d[reg] & 0xFFFF_0000) | (value & 0xFFFF);
                                }
                                4 => {
                                    state.d[reg] = value;
                                }
                                _ => {}
                            },
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
                                    _ => {}
                                }
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
                                    _ => {}
                                }
                                let inc = if bytes == 1 && reg == 7 { 2 } else { bytes };
                                state.a[reg] = state.a[reg].wrapping_add(inc);
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
                                    _ => {}
                                }
                            }
                            5 => {
                                if let Some(disp16) = memory.read_u16_be(*ext_pc) {
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
                                        _ => {}
                                    }
                                }
                            }
                            6 => {
                                if let Some(ext) = memory.read_u16_be(*ext_pc) {
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
                                        _ => {}
                                    }
                                }
                            }
                            7 => match reg {
                                0 => {
                                    if let Some(aw) = memory.read_u16_be(*ext_pc) {
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
                                            _ => {}
                                        }
                                    }
                                }
                                1 => {
                                    if let Some(addr) = memory.read_u32_be(*ext_pc) {
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
                                            _ => {}
                                        }
                                    }
                                }
                                3 => {
                                    if let Some(ext) = memory.read_u16_be(*ext_pc) {
                                        *ext_pc = ext_pc.saturating_add(2);
                                        let addr = indexed_addr(*ext_pc, ext, state);
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
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    };

                    if size_bytes != 0 {
                        if let Some(raw) = read_src(
                            state,
                            memory,
                            src_mode,
                            src_reg_idx,
                            &mut src_ext_pc,
                            size_bytes,
                        ) {
                            let masked = match size_bytes {
                                1 => raw & 0xFF,
                                2 => raw & 0xFFFF,
                                _ => raw,
                            };
                            write_dst(
                                state,
                                memory,
                                dst_mode,
                                dst_reg_idx,
                                &mut dst_ext_pc,
                                size_bytes,
                                masked,
                            );
                            match size_bytes {
                                1 => set_ccr_nz(state, (masked & 0x80) != 0, (masked & 0xFF) == 0),
                                2 => {
                                    set_ccr_nz(state, (masked & 0x8000) != 0, (masked & 0xFFFF) == 0)
                                }
                                4 => set_ccr_nz(state, (masked & 0x8000_0000) != 0, masked == 0),
                                _ => {}
                            }
                        }
                    }
                }
                state.pc = pc.saturating_add(2 + (src_words + dst_words) * 2);
                return Ok(true);
            }
        }


    Ok(false)
}
