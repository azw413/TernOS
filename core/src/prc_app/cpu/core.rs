extern crate alloc;

use crate::prc_app::cpu::decode::{DecodedOp, decode_word};
use crate::prc_app::cpu::memory::MemoryMap;

#[derive(Clone, Debug, Default)]
pub struct CpuState68k {
    pub d: [u32; 8],
    pub a: [u32; 8],
    pub pc: u32,
    pub sr: u16,
    pub call_stack: alloc::vec::Vec<u32>,
    pub frame_stack: alloc::vec::Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    ATrap { trap_word: u16, pc: u32 },
    Trap15 { pc: u32 },
    Trap { vector: u8, pc: u32 },
    OutOfBounds { pc: u32 },
    UnknownOpcode { pc: u32, word: u16 },
    ReturnUnderflow { pc: u32 },
    EntryReturn { pc: u32 },
    StepLimit { pc: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trap15Action {
    Stop,
    Continue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecConfig {
    pub step_limit: usize,
    pub max_events: usize,
    pub trap15_action: Trap15Action,
    pub stop_on_atrap: bool,
    pub stop_on_unknown: bool,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            step_limit: 4096,
            max_events: 64,
            trap15_action: Trap15Action::Stop,
            stop_on_atrap: true,
            stop_on_unknown: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepEvent {
    ATrap { trap_word: u16, pc: u32 },
    Trap15 { pc: u32, selector: Option<u16> },
    Trap { vector: u8, pc: u32 },
}

#[derive(Clone, Debug, Default)]
pub struct ExecTrace {
    pub steps: usize,
    pub events: alloc::vec::Vec<StepEvent>,
    pub unknown_count: u32,
    pub unknown_samples: alloc::vec::Vec<(u32, u16)>,
    pub pc_samples: alloc::vec::Vec<(u32, u32)>,
    pub a2_changes: alloc::vec::Vec<(usize, u32, u32)>,
    pub recent_pcs: alloc::vec::Vec<u32>,
    pub stop: Option<StopReason>,
}

pub fn run_until_stop(state: &mut CpuState68k, memory: &mut MemoryMap, step_limit: usize) -> ExecTrace {
    let cfg = ExecConfig {
        step_limit,
        ..ExecConfig::default()
    };
    run_with_config(state, memory, cfg)
}

pub fn run_with_config(state: &mut CpuState68k, memory: &mut MemoryMap, cfg: ExecConfig) -> ExecTrace {
    fn sr_cond_true(sr: u16, cond: u8) -> bool {
        let c = (sr & 0x0001) != 0;
        let v = (sr & 0x0002) != 0;
        let z = (sr & 0x0004) != 0;
        let n = (sr & 0x0008) != 0;
        match cond {
            0x0 => true,               // T
            0x1 => false,              // F
            0x2 => !c && !z,           // HI
            0x3 => c || z,             // LS
            0x4 => !c,                 // CC
            0x5 => c,                  // CS
            0x6 => !z,                 // NE
            0x7 => z,                  // EQ
            0x8 => !v,                 // VC
            0x9 => v,                  // VS
            0xA => !n,                 // PL
            0xB => n,                  // MI
            0xC => n == v,             // GE
            0xD => n != v,             // LT
            0xE => !z && (n == v),     // GT
            0xF => z || (n != v),      // LE
            _ => false,
        }
    }

    fn branch_disp(memory: &MemoryMap, pc: u32, op: u16) -> Option<(i32, u32)> {
        let disp8 = (op & 0x00FF) as u8;
        if disp8 == 0 {
            let ext = memory.read_u16_be(pc + 2)? as i16 as i32;
            Some((ext, 4))
        } else {
            Some(((disp8 as i8) as i32, 2))
        }
    }

    fn target_pc(base_pc: u32, disp: i32) -> Option<u32> {
        let target = (base_pc as i64) + (disp as i64);
        if target < 0 {
            None
        } else {
            Some(target as u32)
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

    fn set_sr_z_only(state: &mut CpuState68k, zero: bool) {
        state.sr &= !0x0004;
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

    fn ea_ext_words(mode: u16, reg: u16) -> Option<u32> {
        match mode {
            0 | 1 | 2 | 3 | 4 => Some(0), // Dn/An/(An)/(An)+/-(An)
            5 | 6 => Some(1),             // d16(An) / d8(An,Xn)
            7 => match reg {
                0 => Some(1), // abs.w
                1 => Some(2), // abs.l
                2 => Some(1), // d16(PC)
                3 => Some(1), // d8(PC,Xn)
                4 => Some(0), // #imm (not valid as dest, but keep parser moving)
                _ => None,
            },
            _ => None,
        }
    }

    let mut trace = ExecTrace::default();
    let mut last_a2 = state.a[2];
    trace.a2_changes.push((0, state.pc, last_a2));
    while trace.steps < cfg.step_limit {
        let pc = state.pc;
        if trace.recent_pcs.len() == 16 {
            trace.recent_pcs.remove(0);
        }
        trace.recent_pcs.push(pc);
        if state.a[2] != last_a2 {
            last_a2 = state.a[2];
            if trace.a2_changes.len() < 64 {
                trace.a2_changes.push((trace.steps, pc, last_a2));
            }
        }
        if let Some((_, count)) = trace.pc_samples.iter_mut().find(|(p, _)| *p == pc) {
            *count = count.saturating_add(1);
        } else if trace.pc_samples.len() < 64 {
            trace.pc_samples.push((pc, 1));
        }
        let word = match memory.read_u16_be(pc) {
            Some(w) => w,
            None => {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            }
        };
        trace.steps = trace.steps.saturating_add(1);

        // BRA/BSR/Bcc
        if (word & 0xF000) == 0x6000 {
            let cond = ((word >> 8) & 0x000F) as u8;
            let (disp, instr_len) = match branch_disp(memory, pc, word) {
                Some(v) => v,
                None => {
                    trace.stop = Some(StopReason::OutOfBounds { pc });
                    return trace;
                }
            };
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
                let Some(dst) = target_pc(next_pc, disp) else {
                    trace.stop = Some(StopReason::OutOfBounds { pc });
                    return trace;
                };
                state.pc = dst;
            } else {
                state.pc = next_pc;
            }
            continue;
        }

        // JMP abs.l / (d16,PC) / (An)
        if word == 0x4EF9 {
            let Some(dst) = memory.read_u32_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            state.pc = dst;
            continue;
        }
        if word == 0x4EFA {
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let disp = (disp16 as i16) as i32;
            // For d16(PC), 68k uses the extension-word address as the PC base.
            let base = pc.saturating_add(2);
            let Some(dst) = target_pc(base, disp) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            state.pc = dst;
            continue;
        }
        if word == 0x4EFB {
            // JMP (d8,PC,Xn) brief extension form.
            let Some(ext) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let disp8 = (ext & 0x00FF) as u8 as i8 as i32;
            let idx_is_a = (ext & 0x8000) != 0;
            let idx_reg = ((ext >> 12) & 0x0007) as usize;
            let idx_long = (ext & 0x0800) != 0;
            let idx_raw = if idx_is_a { state.a[idx_reg] } else { state.d[idx_reg] };
            let idx = if idx_long {
                idx_raw as i32
            } else {
                (idx_raw as u16 as i16) as i32
            };
            let base = pc.saturating_add(2);
            let Some(dst) = target_pc(base, disp8.saturating_add(idx)) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            state.pc = dst;
            continue;
        }
        if (word & 0xFFF8) == 0x4ED0 {
            let an = (word & 0x0007) as usize;
            state.pc = state.a[an];
            continue;
        }

        // JSR abs.l / (d16,PC) / (An)
        if word == 0x4EB9 {
            let Some(dst) = memory.read_u32_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let ret = pc.saturating_add(6);
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], ret);
            state.call_stack.push(ret);
            state.pc = dst;
            continue;
        }
        if word == 0x4EBA {
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let disp = (disp16 as i16) as i32;
            // For d16(PC), 68k uses the extension-word address as the PC base.
            let base = pc.saturating_add(2);
            let Some(dst) = target_pc(base, disp) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let ret = pc.saturating_add(4);
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], ret);
            state.call_stack.push(ret);
            state.pc = dst;
            continue;
        }
        if (word & 0xFFF8) == 0x4E90 {
            let an = (word & 0x0007) as usize;
            let ret = pc.saturating_add(2);
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], ret);
            state.call_stack.push(ret);
            state.pc = state.a[an];
            continue;
        }

        // RTS
        if word == 0x4E75 {
            let ret = if let Some(v) = state.call_stack.pop() {
                state.a[7] = state.a[7].wrapping_add(4);
                v
            } else if let Some(v) = memory.read_u32_be(state.a[7]) {
                state.a[7] = state.a[7].wrapping_add(4);
                v
            } else {
                trace.stop = Some(StopReason::ReturnUnderflow { pc });
                return trace;
            };
            if ret == u32::MAX {
                trace.stop = Some(StopReason::EntryReturn { pc });
                return trace;
            }
            state.pc = ret;
            continue;
        }

        // LINK An,#disp16
        if (word & 0xFFF8) == 0x4E50 {
            let an = (word & 0x0007) as usize;
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let disp = (disp16 as i16) as i32;
            let old_an = state.a[an];
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], old_an);
            state.a[an] = state.a[7];
            state.a[7] = add_signed_u32(state.a[7], disp);
            state.frame_stack.push(old_an);
            state.pc = pc.saturating_add(4);
            continue;
        }

        // UNLK An
        if (word & 0xFFF8) == 0x4E58 {
            let an = (word & 0x0007) as usize;
            let frame = state.a[an];
            state.a[7] = frame;
            let restored = memory.read_u32_be(state.a[7]).or_else(|| state.frame_stack.pop());
            if let Some(old_an) = restored {
                state.a[an] = old_an;
            }
            state.a[7] = state.a[7].wrapping_add(4);
            state.pc = pc.saturating_add(2);
            continue;
        }

        // MOVEM.[W/L] subset used by app prolog/epilog.
        if (word & 0xFB80) == 0x4880 || (word & 0xFB80) == 0x4C80 {
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
            let is_mem_to_regs = (word & 0x0400) != 0; // 0x4Cxx direction
            let size_bytes = if (word & 0x0040) != 0 { 4u32 } else { 2u32 };
            let Some(mask) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
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

            match (is_mem_to_regs, mode) {
                // MOVEM <reglist>,-(An)
                (false, 4) => {
                    let mut addr = state.a[reg];
                    for idx in (0..16usize).rev() {
                        if (mask & (1u16 << idx)) == 0 {
                            continue;
                        }
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
                _ => {}
            }
            state.pc = pc.saturating_add(4);
            continue;
        }

        // PEA (An)
        if (word & 0xFFF8) == 0x4850 {
            let an = (word & 0x0007) as usize;
            let addr = state.a[an];
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], addr);
            state.pc = pc.saturating_add(2);
            continue;
        }
        // PEA (d8,An,Xn)
        if (word & 0xFFF8) == 0x4870 {
            let an = (word & 0x0007) as usize;
            let Some(ext) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
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
            continue;
        }
        // PEA (d16,An)
        if (word & 0xFFF8) == 0x4868 {
            let an = (word & 0x0007) as usize;
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let addr = add_signed_u32(state.a[an], (disp16 as i16) as i32);
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], addr);
            state.pc = pc.saturating_add(4);
            continue;
        }
        // PEA (d16,PC)
        if word == 0x487A {
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let addr = add_signed_u32(pc.saturating_add(4), (disp16 as i16) as i32);
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], addr);
            state.pc = pc.saturating_add(4);
            continue;
        }
        // PEA abs.l
        if word == 0x4879 {
            let Some(addr) = memory.read_u32_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], addr);
            state.pc = pc.saturating_add(6);
            continue;
        }
        // PEA abs.w
        if word == 0x4878 {
            let Some(aw) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let addr = (aw as i16 as i32) as u32;
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], addr);
            state.pc = pc.saturating_add(4);
            continue;
        }

        // CLR family / TST family register-direct.
        if (word & 0xFFC0) == 0x4200
            || (word & 0xFFC0) == 0x4240
            || (word & 0xFFC0) == 0x4280
            || (word & 0xFFC0) == 0x4A00
            || (word & 0xFFC0) == 0x4A40
            || (word & 0xFFC0) == 0x4A80
        {
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
            if mode == 0 {
                let op_hi = word & 0xFF00;
                match op_hi {
                    0x4200 => {
                        // CLR.B Dn
                        state.d[reg] &= 0xFFFF_FF00;
                        set_ccr_nz(state, false, true);
                    }
                    0x4240 => {
                        // CLR.W Dn
                        state.d[reg] &= 0xFFFF_0000;
                        set_ccr_nz(state, false, true);
                    }
                    0x4280 => {
                        // CLR.L Dn
                        state.d[reg] = 0;
                        set_ccr_nz(state, false, true);
                    }
                    0x4A00 => {
                        // TST.B Dn
                        let v = (state.d[reg] & 0xFF) as u8;
                        set_ccr_nz(state, (v & 0x80) != 0, v == 0);
                    }
                    0x4A40 => {
                        // TST.W Dn
                        let v = (state.d[reg] & 0xFFFF) as u16;
                        set_ccr_nz(state, (v & 0x8000) != 0, v == 0);
                    }
                    0x4A80 => {
                        // TST.L Dn
                        let v = state.d[reg];
                        set_ccr_nz(state, (v & 0x8000_0000) != 0, v == 0);
                    }
                    _ => {}
                }
            } else if mode == 4 {
                // CLR.<size> -(An), primarily used for stack argument setup.
                let an = reg;
                match word & 0xFF00 {
                    0x4200 => {
                        let dec = if an == 7 { 2 } else { 1 };
                        state.a[an] = state.a[an].wrapping_sub(dec);
                        let _ = memory.write_u8(state.a[an], 0);
                        set_ccr_nz(state, false, true);
                    }
                    0x4240 => {
                        state.a[an] = state.a[an].wrapping_sub(2);
                        let _ = memory.write_u16_be(state.a[an], 0);
                        set_ccr_nz(state, false, true);
                    }
                    0x4280 => {
                        state.a[an] = state.a[an].wrapping_sub(4);
                        let _ = memory.write_u32_be(state.a[an], 0);
                        set_ccr_nz(state, false, true);
                    }
                    _ => {}
                }
            }
            state.pc = pc.saturating_add(2);
            continue;
        }

        // NEG family register-direct (flow + minimal CCR updates).
        if (word & 0xFFC0) == 0x4400 || (word & 0xFFC0) == 0x4440 || (word & 0xFFC0) == 0x4480
        {
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
            continue;
        }

        // EXT.W Dn / EXT.L Dn
        if (word & 0xFFF8) == 0x4880 {
            let dn = (word & 0x0007) as usize;
            let b = (state.d[dn] & 0xFF) as u8;
            let w = (b as i8 as i16) as u16;
            state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (w as u32);
            set_ccr_nz(state, (w & 0x8000) != 0, w == 0);
            state.pc = pc.saturating_add(2);
            continue;
        }
        if (word & 0xFFF8) == 0x48C0 {
            let dn = (word & 0x0007) as usize;
            let w = (state.d[dn] & 0xFFFF) as u16;
            let l = (w as i16 as i32) as u32;
            state.d[dn] = l;
            set_ccr_nz(state, (l & 0x8000_0000) != 0, l == 0);
            state.pc = pc.saturating_add(2);
            continue;
        }

        // MOVEQ #imm,Dn
        if (word & 0xF100) == 0x7000 {
            let dn = ((word >> 9) & 0x0007) as usize;
            let imm8 = (word & 0x00FF) as u8;
            let v = (imm8 as i8 as i32) as u32;
            state.d[dn] = v;
            set_ccr_nz(state, (v & 0x8000_0000) != 0, v == 0);
            state.pc = pc.saturating_add(2);
            continue;
        }

        // Immediate op class: ORI/ANDI/SUBI/ADDI/EORI/CMPI #imm,<ea>
        {
            let base = word & 0xFF00;
            let is_imm_op = matches!(base, 0x0000 | 0x0200 | 0x0400 | 0x0600 | 0x0A00 | 0x0C00);
            if is_imm_op {
                let size_bits = (word >> 6) & 0x0003;
                let imm_words = match size_bits {
                    0 | 1 => 1, // byte/word immediate stored in one extension word
                    2 => 2,     // long immediate
                    3 => 1,     // tolerate odd encodings seen in app code paths during probing
                    _ => 0,
                };
                if imm_words != 0 {
                    let mode = (word >> 3) & 0x0007;
                    let reg = word & 0x0007;
                    // Minimal execution semantics for ANDI to Dn (used in Noah loop).
                    if base == 0x0200 && mode == 0 {
                        let dn = reg as usize;
                        match size_bits {
                            0 | 1 | 3 => {
                                let Some(imm) = memory.read_u16_be(pc + 2) else {
                                    trace.stop = Some(StopReason::OutOfBounds { pc });
                                    return trace;
                                };
                                let val = (state.d[dn] as u16) & imm;
                                state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (val as u32);
                                set_ccr_nz(state, (val & 0x8000) != 0, val == 0);
                            }
                            2 => {
                                let Some(imm) = memory.read_u32_be(pc + 2) else {
                                    trace.stop = Some(StopReason::OutOfBounds { pc });
                                    return trace;
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
                        continue;
                    }
                }
            }
        }

        // MOVE.[B/W/L] generic length handling (covers many data motion opcodes).
        {
            let sz = (word >> 12) & 0x0003;
            if sz == 0x1 || sz == 0x2 || sz == 0x3 {
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
                                let addr = add_signed_u32(pc.saturating_add(4), (disp16 as i16) as i32);
                                if let Some(v) = memory.read_u16_be(addr) {
                                    state.a[an] = (v as i16 as i32) as u32;
                                }
                            }
                        }
                        (0x2, 7) if src_reg == 2 => {
                            // MOVEA.L (d16,PC),Am
                            if let Some(disp16) = memory.read_u16_be(pc + 2) {
                                let addr = add_signed_u32(pc.saturating_add(4), (disp16 as i16) as i32);
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
                continue;
            }
        }

        // ADD/SUB register/memory classes (length-only handling).
        if (word & 0xF000) == 0xD000 || (word & 0xF000) == 0x9000 {
            let mode = (word >> 3) & 0x0007;
            let reg = word & 0x0007;
            let ea_words = ea_ext_words(mode, reg).unwrap_or(0);
            // Minimal execution for ADD <Dn>,Dn (word/long destination in Dn).
            if (word & 0xF000) == 0xD000 && mode == 0 {
                let src = reg as usize;
                let dst = ((word >> 9) & 0x0007) as usize;
                let opmode = (word >> 6) & 0x0007;
                match opmode {
                    0x0 => {
                        let s = (state.d[src] & 0xFF) as u8;
                        let d = (state.d[dst] & 0xFF) as u8;
                        let r = d.wrapping_add(s);
                        state.d[dst] = (state.d[dst] & 0xFFFF_FF00) | (r as u32);
                        set_ccr_nz(state, (r & 0x80) != 0, r == 0);
                    }
                    0x1 => {
                        let s = (state.d[src] & 0xFFFF) as u16;
                        let d = (state.d[dst] & 0xFFFF) as u16;
                        let r = d.wrapping_add(s);
                        state.d[dst] = (state.d[dst] & 0xFFFF_0000) | (r as u32);
                        set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
                    }
                    0x2 => {
                        let r = state.d[dst].wrapping_add(state.d[src]);
                        state.d[dst] = r;
                        set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
                    }
                    0x3 => {
                        // ADDA.W Dn,An
                        let s = (state.d[src] & 0xFFFF) as u16;
                        let s = (s as i16 as i32) as u32;
                        state.a[dst] = state.a[dst].wrapping_add(s);
                    }
                    0x7 => {
                        // ADDA.L Dn,An
                        state.a[dst] = state.a[dst].wrapping_add(state.d[src]);
                    }
                    _ => {}
                }
            }
            state.pc = pc.saturating_add(2 + ea_words * 2);
            continue;
        }

        // DBcc Dn,<disp16>
        if (word & 0xF0F8) == 0x50C8 {
            let cond = ((word >> 8) & 0x000F) as u8;
            let dn = (word & 0x0007) as usize;
            let Some(disp16) = memory.read_u16_be(pc + 2) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            let disp = (disp16 as i16) as i32;
            let next_pc = pc.saturating_add(4);
            if !sr_cond_true(state.sr, cond) {
                let count = (state.d[dn] & 0xFFFF) as u16;
                let new_count = count.wrapping_sub(1);
                state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (new_count as u32);
                if new_count != 0xFFFF {
                    let Some(dst) = target_pc(next_pc, disp) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    state.pc = dst;
                } else {
                    state.pc = next_pc;
                }
            } else {
                state.pc = next_pc;
            }
            continue;
        }

        // Dynamic bit operations: BTST/BCHG/BCLR/BSET Dn,<ea>.
        if (word & 0xF100) == 0x0100 {
            let bit_src_dn = ((word >> 9) & 0x0007) as usize;
            let op_kind = (word >> 6) & 0x0003; // 0=BTST 1=BCHG 2=BCLR 3=BSET
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
            match mode {
                0 => {
                    // Register destination: bit index modulo 32.
                    let bit = (state.d[bit_src_dn] & 31) as u32;
                    let mask = 1u32 << bit;
                    let old = state.d[reg];
                    set_sr_z_only(state, (old & mask) == 0);
                    let new_val = match op_kind {
                        0 => old,
                        1 => old ^ mask,
                        2 => old & !mask,
                        _ => old | mask,
                    };
                    if op_kind != 0 {
                        state.d[reg] = new_val;
                    }
                    state.pc = pc.saturating_add(2);
                    continue;
                }
                2 => {
                    // Memory destination (An): bit index modulo 8.
                    let bit = (state.d[bit_src_dn] & 7) as u8;
                    let mask = 1u8 << bit;
                    let addr = state.a[reg];
                    let old = memory.read_u8(addr).unwrap_or(0);
                    set_sr_z_only(state, (old & mask) == 0);
                    let new_val = match op_kind {
                        0 => old,
                        1 => old ^ mask,
                        2 => old & !mask,
                        _ => old | mask,
                    };
                    if op_kind != 0 {
                        memory.write_u8(addr, new_val);
                    }
                    state.pc = pc.saturating_add(2);
                    continue;
                }
                _ => {
                    // Unsupported EA for now: advance conservatively so execution can continue.
                    let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
                    state.pc = pc.saturating_add(2 + ea_words * 2);
                    continue;
                }
            }
        }

        // CMP <ea>,Dn (byte/word/long)
        if (word & 0xF000) == 0xB000 {
            let dn = ((word >> 9) & 0x0007) as usize;
            let opmode = (word >> 6) & 0x0007;
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
            let size = match opmode {
                0 => Some(1u32),
                1 => Some(2u32),
                2 => Some(4u32),
                _ => None,
            };
            if let Some(size_bytes) = size {
                let mut ext_pc = pc.saturating_add(2);
                let src = match mode {
                    0 => Some(state.d[reg]),
                    1 => Some(state.a[reg]),
                    2 => {
                        let addr = state.a[reg];
                        match size_bytes {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        }
                    }
                    3 => {
                        let addr = state.a[reg];
                        let v = match size_bytes {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        };
                        let inc = if size_bytes == 1 && reg == 7 {
                            2
                        } else {
                            size_bytes
                        };
                        state.a[reg] = state.a[reg].wrapping_add(inc);
                        v
                    }
                    5 => {
                        let disp_w = match memory.read_u16_be(ext_pc) {
                            Some(v) => v,
                            None => return trace,
                        };
                        let disp = disp_w as i16 as i32;
                        ext_pc = ext_pc.saturating_add(2);
                        let addr = add_signed_u32(state.a[reg], disp);
                        match size_bytes {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        }
                    }
                    6 => {
                        let ext = match memory.read_u16_be(ext_pc) {
                            Some(v) => v,
                            None => return trace,
                        };
                        ext_pc = ext_pc.saturating_add(2);
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
                        let addr = add_signed_u32(state.a[reg], disp8.saturating_add(idx));
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
                                None => return trace,
                            };
                            let aw = aw_w as i16 as i32;
                            ext_pc = ext_pc.saturating_add(2);
                            let addr = aw as u32;
                            match size_bytes {
                                1 => memory.read_u8(addr).map(u32::from),
                                2 => memory.read_u16_be(addr).map(u32::from),
                                _ => memory.read_u32_be(addr),
                            }
                        }
                        1 => {
                            let addr = match memory.read_u32_be(ext_pc) {
                                Some(v) => v,
                                None => return trace,
                            };
                            ext_pc = ext_pc.saturating_add(4);
                            match size_bytes {
                                1 => memory.read_u8(addr).map(u32::from),
                                2 => memory.read_u16_be(addr).map(u32::from),
                                _ => memory.read_u32_be(addr),
                            }
                        }
                        2 => {
                            let disp_w = match memory.read_u16_be(ext_pc) {
                                Some(v) => v,
                                None => return trace,
                            };
                            let disp = disp_w as i16 as i32;
                            ext_pc = ext_pc.saturating_add(2);
                            let addr = add_signed_u32(pc.saturating_add(2), disp);
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
                if let Some(src_raw) = src {
                    let (src_v, dst_v, bits) = match size_bytes {
                        1 => (src_raw & 0xFF, state.d[dn] & 0xFF, 8u32),
                        2 => (src_raw & 0xFFFF, state.d[dn] & 0xFFFF, 16u32),
                        _ => (src_raw, state.d[dn], 32u32),
                    };
                    let res = dst_v.wrapping_sub(src_v);
                    set_ccr_sub(state, src_v, dst_v, res, bits);
                }
                let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
                state.pc = pc.saturating_add(2 + ea_words * 2);
                continue;
            }
        }

        // ADDQ/SUBQ (length-only handling).
        if (word & 0xF000) == 0x5000 {
            let mode = (word >> 3) & 0x0007;
            let reg = word & 0x0007;
            let ea_words = ea_ext_words(mode, reg).unwrap_or(0);
            // Minimal execution for ADDQ/SUBQ on Dn (needed for loop counters).
            if mode == 0 {
                let dn = reg as usize;
                let mut q = ((word >> 9) & 0x0007) as u32;
                if q == 0 {
                    q = 8;
                }
                let is_sub = ((word >> 8) & 0x1) != 0;
                let size_bits = (word >> 6) & 0x3;
                match size_bits {
                    0 => {
                        let d = (state.d[dn] & 0xFF) as u8;
                        let r = if is_sub { d.wrapping_sub(q as u8) } else { d.wrapping_add(q as u8) };
                        state.d[dn] = (state.d[dn] & 0xFFFF_FF00) | (r as u32);
                        set_ccr_nz(state, (r & 0x80) != 0, r == 0);
                    }
                    1 => {
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
            } else if mode == 1 {
                // ADDQ/SUBQ on address register An: always address arithmetic, no CCR updates.
                let an = reg as usize;
                let mut q = ((word >> 9) & 0x0007) as u32;
                if q == 0 {
                    q = 8;
                }
                let is_sub = ((word >> 8) & 0x1) != 0;
                state.a[an] = if is_sub {
                    state.a[an].wrapping_sub(q)
                } else {
                    state.a[an].wrapping_add(q)
                };
            }
            state.pc = pc.saturating_add(2 + ea_words * 2);
            continue;
        }

        // LEA <ea>,Am (addressing modes commonly used in startup stubs).
        if (word & 0xF1C0) == 0x41C0 {
            let am = ((word >> 9) & 0x0007) as usize;
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
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
            let (addr_opt, advance) = match (mode, reg) {
                (2, an) => (Some(state.a[an]), 2u32), // (An)
                (3, an) => {
                    let a = state.a[an];
                    state.a[an] = state.a[an].wrapping_add(4);
                    (Some(a), 2u32)
                } // (An)+
                (5, an) => {
                    let Some(disp16) = memory.read_u16_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    (Some(add_signed_u32(state.a[an], (disp16 as i16) as i32)), 4u32)
                } // (d16,An)
                (6, an) => {
                    let Some(ext) = memory.read_u16_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    (Some(indexed_addr(state.a[an], ext, state)), 4u32)
                } // (d8,An,Xn)
                (7, 0) => {
                    let Some(aw) = memory.read_u16_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    (Some((aw as i16 as i32) as u32), 4u32)
                } // (abs.w)
                (7, 1) => {
                    let Some(al) = memory.read_u32_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    (Some(al), 6u32)
                } // (abs.l)
                (7, 2) => {
                    let Some(disp16) = memory.read_u16_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    (Some(add_signed_u32(pc.saturating_add(4), (disp16 as i16) as i32)), 4u32)
                } // (d16,PC)
                (7, 3) => {
                    let Some(ext) = memory.read_u16_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    (Some(indexed_addr(pc.saturating_add(4), ext, state)), 4u32)
                } // (d8,PC,Xn)
                _ => (None, 2u32),
            };
            if let Some(addr) = addr_opt {
                state.a[am] = addr;
            }
            state.pc = pc.saturating_add(advance);
            continue;
        }

        match decode_word(word) {
            DecodedOp::ATrap(trap_word) => {
                state.pc = state.pc.saturating_add(2);
                trace.events.push(StepEvent::ATrap { trap_word, pc });
                if cfg.stop_on_atrap || trace.events.len() >= cfg.max_events {
                    trace.stop = Some(StopReason::ATrap { trap_word, pc });
                    return trace;
                }
            }
            DecodedOp::Trap(15) => {
                let selector = memory.read_u16_be(pc.saturating_add(2));
                if cfg.trap15_action == Trap15Action::Continue {
                    // PalmOS glue often encodes A-trap selectors after TRAP #15.
                    // In continue mode, consume selector words and surface them as A-traps.
                    if let Some(sel) = selector {
                        if (sel & 0xF000) == 0xA000 {
                            state.pc = state.pc.saturating_add(4);
                            trace.events.push(StepEvent::Trap15 {
                                pc,
                                selector: Some(sel),
                            });
                            trace.events.push(StepEvent::ATrap {
                                trap_word: sel,
                                pc: pc.saturating_add(2),
                            });
                            if cfg.stop_on_atrap || trace.events.len() >= cfg.max_events {
                                trace.stop = Some(StopReason::ATrap {
                                    trap_word: sel,
                                    pc: pc.saturating_add(2),
                                });
                                return trace;
                            }
                            continue;
                        }
                    }
                }
                state.pc = state.pc.saturating_add(2);
                trace.events.push(StepEvent::Trap15 { pc, selector });
                if cfg.trap15_action == Trap15Action::Stop || trace.events.len() >= cfg.max_events {
                    trace.stop = Some(StopReason::Trap15 { pc });
                    return trace;
                }
            }
            DecodedOp::Trap(vector) => {
                state.pc = state.pc.saturating_add(2);
                trace.events.push(StepEvent::Trap { vector, pc });
                trace.stop = Some(StopReason::Trap { vector, pc });
                return trace;
            }
            DecodedOp::Unknown(_) => {
                if cfg.stop_on_unknown {
                    trace.stop = Some(StopReason::UnknownOpcode { pc, word });
                    return trace;
                }
                trace.unknown_count = trace.unknown_count.saturating_add(1);
                if trace.unknown_samples.len() < 16 {
                    trace.unknown_samples.push((pc, word));
                }
                state.pc = state.pc.saturating_add(2);
                continue;
            }
        }

        if state.pc < memory.base || state.pc > memory.end() {
            trace.stop = Some(StopReason::OutOfBounds { pc: state.pc });
            return trace;
        }
    }
    trace.stop = Some(StopReason::StepLimit { pc: state.pc });
    trace
}
