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
                true
            } else {
                sr_cond_true(state.sr, cond)
            };
            if taken {
                let Some(dst) = target_pc(base_pc, disp) else {
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
            state.pc = dst;
            continue;
        }
        if (word & 0xFFF8) == 0x4E90 {
            let an = (word & 0x0007) as usize;
            let ret = pc.saturating_add(2);
            state.a[7] = state.a[7].wrapping_sub(4);
            memory.write_u32_be(state.a[7], ret);
            state.pc = state.a[an];
            continue;
        }

        // RTS
        if word == 0x4E75 {
            let ret = if let Some(v) = memory.read_u32_be(state.a[7]) {
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
            state.pc = pc.saturating_add(4);
            continue;
        }

        // UNLK An
        if (word & 0xFFF8) == 0x4E58 {
            let an = (word & 0x0007) as usize;
            let frame = state.a[an];
            state.a[7] = frame;
            let Some(old_an) = memory.read_u32_be(state.a[7]) else {
                trace.stop = Some(StopReason::OutOfBounds { pc });
                return trace;
            };
            state.a[an] = old_an;
            state.a[7] = state.a[7].wrapping_add(4);
            state.pc = pc.saturating_add(2);
            continue;
        }

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
        // SWAP Dn
        if (word & 0xFFF8) == 0x4840 {
            let dn = (word & 0x0007) as usize;
            let v = state.d[dn];
            let r = v.rotate_right(16);
            state.d[dn] = r;
            set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
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
            // For d16(PC), 68k uses the extension-word address as PC base.
            let addr = add_signed_u32(pc.saturating_add(2), (disp16 as i16) as i32);
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

        // CLR/TST family (register-direct + key memory forms).
        if (word & 0xFFC0) == 0x4200
            || (word & 0xFFC0) == 0x4240
            || (word & 0xFFC0) == 0x4280
            || (word & 0xFFC0) == 0x4A00
            || (word & 0xFFC0) == 0x4A40
            || (word & 0xFFC0) == 0x4A80
        {
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
            let size_bits = (word >> 6) & 0x0003; // 0=byte, 1=word, 2=long
            let is_clr = (word & 0xFF00) == 0x4200;
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
                // CLR.<size> -(An), primarily used for stack argument setup.
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
                // CLR.<size> (An), used by Palm startup relocation glue.
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

        // Register shift/rotate class (AS/LS/ROX/RO on Dn).
        // Needed by Palm app glue paths (e.g. `LSR.W #1,D0` = 0xE248).
        if (word & 0xF000) == 0xE000 {
            let size_bits = (word >> 6) & 0x0003;
            if size_bits != 0x0003 {
                let ir = (word & 0x0020) != 0; // 0=immediate count, 1=count in Dn
                let op = (word >> 3) & 0x0003; // 0=AS,1=LS,2=ROX,3=RO
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
                if width != 0 {
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
                                // Arithmetic shift
                                if left {
                                    (cur << count) & mask
                                } else {
                                    // Sign-extend to 32 then shift right.
                                    let ext = if (cur & sign_bit) != 0 {
                                        cur | (!mask)
                                    } else {
                                        cur
                                    };
                                    (ext as i32 >> count) as u32 & mask
                                }
                            }
                            1 => {
                                // Logical shift
                                if left {
                                    (cur << count) & mask
                                } else {
                                    (cur >> count) & mask
                                }
                            }
                            _ => cur, // ROX/RO not needed yet; keep fail-fast for wrong semantics.
                        };
                    }
                    match size_bits {
                        0 => state.d[dest] = (state.d[dest] & 0xFFFF_FF00) | (out & 0xFF),
                        1 => state.d[dest] = (state.d[dest] & 0xFFFF_0000) | (out & 0xFFFF),
                        _ => state.d[dest] = out,
                    }
                    set_ccr_nz(state, (out & sign_bit) != 0, out == 0);
                    state.pc = pc.saturating_add(2);
                    continue;
                }
            }
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
                                    None => {
                                        trace.stop = Some(StopReason::OutOfBounds { pc });
                                        return trace;
                                    }
                                },
                                2 => match memory.read_u16_be(ext_pc) {
                                    Some(v) => {
                                        ext_pc = ext_pc.saturating_add(2);
                                        v as u32
                                    }
                                    None => {
                                        trace.stop = Some(StopReason::OutOfBounds { pc });
                                        return trace;
                                    }
                                },
                                _ => match memory.read_u32_be(ext_pc) {
                                    Some(v) => {
                                        ext_pc = ext_pc.saturating_add(4);
                                        v
                                    }
                                    None => {
                                        trace.stop = Some(StopReason::OutOfBounds { pc });
                                        return trace;
                                    }
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
                                        None => {
                                            trace.stop = Some(StopReason::OutOfBounds { pc });
                                            return trace;
                                        }
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
                                            None => {
                                                trace.stop = Some(StopReason::OutOfBounds { pc });
                                                return trace;
                                            }
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
                                            None => {
                                                trace.stop = Some(StopReason::OutOfBounds { pc });
                                                return trace;
                                            }
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
                            continue;
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
                                    0x0000 => cur | imm,            // ORI.L
                                    0x0400 => cur.wrapping_sub(imm), // SUBI.L
                                    0x0600 => cur.wrapping_add(imm), // ADDI.L
                                    0x0A00 => cur ^ imm,            // EORI.L
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
                                        0x0000 => cur | imm,            // ORI.B
                                        0x0400 => cur.wrapping_sub(imm), // SUBI.B
                                        0x0600 => cur.wrapping_add(imm), // ADDI.B
                                        0x0A00 => cur ^ imm,            // EORI.B
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
                                        0x0000 => cur | imm,            // ORI.W
                                        0x0400 => cur.wrapping_sub(imm), // SUBI.W
                                        0x0600 => cur.wrapping_add(imm), // ADDI.W
                                        0x0A00 => cur ^ imm,            // EORI.W
                                        _ => cur,
                                    };
                                    if matches!(base, 0x0000 | 0x0400 | 0x0600 | 0x0A00) {
                                        apply_word(state, out);
                                        match base {
                                            0x0400 => {
                                                set_ccr_sub(state, imm as u32, cur as u32, out as u32, 16)
                                            }
                                            0x0600 => {
                                                set_ccr_add(state, imm as u32, cur as u32, out as u32, 16)
                                            }
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

        // DIVU/DIVS.W <ea>,Dn (minimal; needed by PRC UI handlers using SysRandom scaling)
        {
            // Distinguish DIVU (opmode 0b011) vs DIVS (opmode 0b111) via bit 8.
            let divu = (word & 0xF1C0) == 0x80C0;
            let divs = (word & 0xF1C0) == 0x81C0;
            if divu || divs {
                let dn = ((word >> 9) & 0x0007) as usize;
                let mode = (word >> 3) & 0x0007;
                let reg = (word & 0x0007) as usize;
                let divisor_opt: Option<u16> = match mode {
                    0 => Some((state.d[reg] & 0xFFFF) as u16), // Dn
                    1 => Some((state.a[reg] & 0xFFFF) as u16), // An
                    2 => memory.read_u16_be(state.a[reg]),      // (An)
                    _ => None,
                };
                let Some(divisor) = divisor_opt else {
                    trace.stop = Some(StopReason::UnknownOpcode { pc, word });
                    return trace;
                };
                if divisor == 0 {
                    trace.stop = Some(StopReason::UnknownOpcode { pc, word });
                    return trace;
                }
                if divu {
                    let dividend = state.d[dn];
                    let q = dividend / (divisor as u32);
                    let r = dividend % (divisor as u32);
                    if q > 0xFFFF {
                        // Overflow: V set, C clear; destination remains unchanged.
                        state.sr |= 0x0002;
                        state.sr &= !0x0001;
                    } else {
                        state.d[dn] = ((r & 0xFFFF) << 16) | (q & 0xFFFF);
                        state.sr &= !0x000F;
                        if (q & 0x8000) != 0 {
                            state.sr |= 0x0008;
                        }
                        if (q & 0xFFFF) == 0 {
                            state.sr |= 0x0004;
                        }
                    }
                } else {
                    let divisor_s = divisor as i16 as i32;
                    let dividend_s = state.d[dn] as i32;
                    let q = dividend_s / divisor_s;
                    let r = dividend_s % divisor_s;
                    if !(-32768..=32767).contains(&q) {
                        // Overflow: V set, C clear; destination remains unchanged.
                        state.sr |= 0x0002;
                        state.sr &= !0x0001;
                    } else {
                        let q16 = (q as i16 as u16) as u32;
                        let r16 = (r as i16 as u16) as u32;
                        state.d[dn] = (r16 << 16) | q16;
                        state.sr &= !0x000F;
                        if (q16 & 0x8000) != 0 {
                            state.sr |= 0x0008;
                        }
                        if (q16 & 0xFFFF) == 0 {
                            state.sr |= 0x0004;
                        }
                    }
                }
                let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
                state.pc = pc.saturating_add(2 + ea_words * 2);
                continue;
            }
        }

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
                continue;
            }
        }

        // ADD/SUB register/memory classes (length-only handling).
        if (word & 0xF000) == 0xD000 || (word & 0xF000) == 0x9000 {
            let mode = (word >> 3) & 0x0007;
            let reg = word & 0x0007;
            let ea_words = ea_ext_words(mode, reg).unwrap_or(0);
            let opmode = (word >> 6) & 0x0007;
            let dst = ((word >> 9) & 0x0007) as usize;
            // ADDA/SUBA minimal support for startup glue.
            if opmode == 0x3 || opmode == 0x7 {
                let src_long = opmode == 0x7;
                let src = match (mode, reg) {
                    // Dn / An sources.
                    (0, r) => {
                        let v = state.d[r as usize];
                        if src_long { Some(v) } else { Some((v as u16 as i16) as i32 as u32) }
                    }
                    (1, r) => {
                        let v = state.a[r as usize];
                        if src_long { Some(v) } else { Some((v as u16 as i16) as i32 as u32) }
                    }
                    // Immediate source (#imm), used by SUBA.L in startup glue.
                    (7, 4) => {
                        if src_long {
                            memory.read_u32_be(pc.saturating_add(2))
                        } else {
                            memory
                                .read_u16_be(pc.saturating_add(2))
                                .map(|v| (v as i16 as i32) as u32)
                        }
                    }
                    _ => None,
                };
                if let Some(src_v) = src {
                    if (word & 0xF000) == 0xD000 {
                        state.a[dst] = state.a[dst].wrapping_add(src_v);
                    } else {
                        state.a[dst] = state.a[dst].wrapping_sub(src_v);
                    }
                }
                let ext_words = if mode == 7 && reg == 4 {
                    if src_long { 2 } else { 1 }
                } else {
                    ea_words
                };
                state.pc = pc.saturating_add(2 + ext_words * 2);
                continue;
            }
            // Minimal execution for ADD <Dn>,Dn (word/long destination in Dn).
            if (word & 0xF000) == 0xD000 && mode == 0 {
                let src = reg as usize;
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
            } else if (word & 0xF000) == 0xD000 && mode == 1 {
                // ADD <An>,Dn (needed by app loop setup, e.g. ADD.L A6,D3).
                let src_an = reg as usize;
                match opmode {
                    0x1 => {
                        // ADD.W An,Dn (word source from An low word).
                        let s = (state.a[src_an] & 0xFFFF) as u16;
                        let d = (state.d[dst] & 0xFFFF) as u16;
                        let r = d.wrapping_add(s);
                        state.d[dst] = (state.d[dst] & 0xFFFF_0000) | (r as u32);
                        set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
                    }
                    0x2 => {
                        // ADD.L An,Dn
                        let r = state.d[dst].wrapping_add(state.a[src_an]);
                        state.d[dst] = r;
                        set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
                    }
                    _ => {}
                }
            } else if (word & 0xF000) == 0xD000 && mode == 2 {
                // ADD Dn,(An) variants used by startup relocation glue.
                let an = reg as usize;
                let src = ((word >> 9) & 0x0007) as usize;
                let opmode = (word >> 6) & 0x0007;
                match opmode {
                    0x4 => {
                        // ADD.B Dn,(An)
                        if let Some(d) = memory.read_u8(state.a[an]) {
                            let s = (state.d[src] & 0xFF) as u8;
                            let r = d.wrapping_add(s);
                            memory.write_u8(state.a[an], r);
                            set_ccr_nz(state, (r & 0x80) != 0, r == 0);
                        }
                    }
                    0x5 => {
                        // ADD.W Dn,(An)
                        if let Some(d) = memory.read_u16_be(state.a[an]) {
                            let s = (state.d[src] & 0xFFFF) as u16;
                            let r = d.wrapping_add(s);
                            memory.write_u16_be(state.a[an], r);
                            set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
                        }
                    }
                    0x6 => {
                        // ADD.L Dn,(An)
                        if let Some(d) = memory.read_u32_be(state.a[an]) {
                            let r = d.wrapping_add(state.d[src]);
                            memory.write_u32_be(state.a[an], r);
                            set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
                        }
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
            // CMPA.W/L <ea>,An (minimal: sets CCR from An - src).
            if opmode == 0x3 || opmode == 0x7 {
                let src_long = opmode == 0x7;
                let src = match (mode, reg) {
                    (0, r) => {
                        let v = state.d[r];
                        if src_long { Some(v) } else { Some((v as u16 as i16) as i32 as u32) }
                    }
                    (1, r) => {
                        let v = state.a[r];
                        if src_long { Some(v) } else { Some((v as u16 as i16) as i32 as u32) }
                    }
                    _ => None,
                };
                if let Some(src_v) = src {
                    let dst_v = state.a[dn];
                    let res = dst_v.wrapping_sub(src_v);
                    set_ccr_sub(state, src_v, dst_v, res, 32);
                }
                let ext_words = ea_ext_words(mode as u16, reg as u16).unwrap_or(0);
                state.pc = pc.saturating_add(2 + ext_words * 2);
                continue;
            }
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

        // ADD <ea>,Dn subset (needed by Palm glue, e.g. `ADD.L A6,D3`).
        if (word & 0xF000) == 0xD000 {
            let opmode = (word >> 6) & 0x0007;
            let dn = ((word >> 9) & 0x0007) as usize;
            let mode = (word >> 3) & 0x0007;
            let reg = (word & 0x0007) as usize;
            let size_bytes = match opmode {
                0 => Some(1u32), // ADD.B <ea>,Dn
                1 => Some(2u32), // ADD.W <ea>,Dn
                2 => Some(4u32), // ADD.L <ea>,Dn
                _ => None,
            };
            if let Some(size) = size_bytes {
                let ea_words = ea_ext_words(mode, reg as u16).unwrap_or(0);
                let mut ext_pc = pc.saturating_add(2);
                let src = match mode {
                    0 => Some(state.d[reg]),
                    1 => Some(state.a[reg]),
                    2 => {
                        let addr = state.a[reg];
                        match size {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        }
                    }
                    3 => {
                        let addr = state.a[reg];
                        let v = match size {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        };
                        let inc = if size == 1 && reg == 7 { 2 } else { size };
                        state.a[reg] = state.a[reg].wrapping_add(inc);
                        v
                    }
                    4 => {
                        let dec = if size == 1 && reg == 7 { 2 } else { size };
                        state.a[reg] = state.a[reg].wrapping_sub(dec);
                        let addr = state.a[reg];
                        match size {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        }
                    }
                    5 => {
                        let disp = memory.read_u16_be(ext_pc).map(|v| v as i16 as i32);
                        ext_pc = ext_pc.saturating_add(2);
                        disp.and_then(|d| {
                            let addr = add_signed_u32(state.a[reg], d);
                            match size {
                                1 => memory.read_u8(addr).map(u32::from),
                                2 => memory.read_u16_be(addr).map(u32::from),
                                _ => memory.read_u32_be(addr),
                            }
                        })
                    }
                    7 if reg == 0 => {
                        let aw = memory.read_u16_be(ext_pc).map(|v| v as i16 as i32 as u32);
                        ext_pc = ext_pc.saturating_add(2);
                        aw.and_then(|addr| match size {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        })
                    }
                    7 if reg == 1 => {
                        let al = memory.read_u32_be(ext_pc);
                        ext_pc = ext_pc.saturating_add(4);
                        al.and_then(|addr| match size {
                            1 => memory.read_u8(addr).map(u32::from),
                            2 => memory.read_u16_be(addr).map(u32::from),
                            _ => memory.read_u32_be(addr),
                        })
                    }
                    _ => None,
                };
                if let Some(s) = src {
                    match size {
                        1 => {
                            let d = (state.d[dn] & 0xFF) as u8;
                            let r = d.wrapping_add((s & 0xFF) as u8);
                            state.d[dn] = (state.d[dn] & 0xFFFF_FF00) | (r as u32);
                            set_ccr_nz(state, (r & 0x80) != 0, r == 0);
                        }
                        2 => {
                            let d = (state.d[dn] & 0xFFFF) as u16;
                            let r = d.wrapping_add((s & 0xFFFF) as u16);
                            state.d[dn] = (state.d[dn] & 0xFFFF_0000) | (r as u32);
                            set_ccr_nz(state, (r & 0x8000) != 0, r == 0);
                        }
                        _ => {
                            let d = state.d[dn];
                            let r = d.wrapping_add(s);
                            state.d[dn] = r;
                            set_ccr_nz(state, (r & 0x8000_0000) != 0, r == 0);
                        }
                    }
                }
                state.pc = pc.saturating_add(2 + ea_words * 2);
                continue;
            }
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
                    // For d16(PC), 68k uses the extension-word address as PC base.
                    (Some(add_signed_u32(pc.saturating_add(2), (disp16 as i16) as i32)), 4u32)
                } // (d16,PC)
                (7, 3) => {
                    let Some(ext) = memory.read_u16_be(pc + 2) else {
                        trace.stop = Some(StopReason::OutOfBounds { pc });
                        return trace;
                    };
                    // For d8(PC,Xn), 68k uses the extension-word address as PC base.
                    (Some(indexed_addr(pc.saturating_add(2), ext, state)), 4u32)
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
