extern crate alloc;

use crate::prc_app::cpu::decode::{DecodedOp, decode_word};
use crate::prc_app::cpu::dispatch::{self, OpcodeFamily};
use crate::prc_app::cpu::memory::MemoryMap;
use crate::prc_app::cpu::ops::alu;
use crate::prc_app::cpu::ops::arith;
use crate::prc_app::cpu::ops::branch;
use crate::prc_app::cpu::ops::extra;
use crate::prc_app::cpu::ops::flow;
use crate::prc_app::cpu::ops::logic;
use crate::prc_app::cpu::ops::misc;
use crate::prc_app::cpu::ops::movem;
use crate::prc_app::cpu::ops::move_ops;
use crate::prc_app::cpu::ops::muldiv;
use crate::prc_app::cpu::ops::quick;
use crate::prc_app::cpu::ops::shiftmem;
use crate::prc_app::cpu::ops::system;
use crate::prc_app::cpu::ops::xops;

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
        let opcode_family = dispatch::classify(word);
        trace.steps = trace.steps.saturating_add(1);

        // BRA/BSR/Bcc
        if matches!(opcode_family, OpcodeFamily::Branch) {
            match branch::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Quick) {
            match quick::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::MulDiv) {
            match muldiv::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }

        if matches!(opcode_family, OpcodeFamily::Other | OpcodeFamily::Trap) {
            match flow::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Other) {
            match system::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Other) {
            match extra::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Other) {
            match misc::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }

        if matches!(opcode_family, OpcodeFamily::Other) {
            match movem::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }

        if matches!(opcode_family, OpcodeFamily::Other) {
            match alu::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Other) {
            match logic::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Other) {
            match xops::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }
        if matches!(opcode_family, OpcodeFamily::Other) {
            match shiftmem::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }

        if matches!(opcode_family, OpcodeFamily::Other) {
            match move_ops::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
        }

        if matches!(opcode_family, OpcodeFamily::Other) {
            match arith::execute(word, pc, state, memory) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(stop) => {
                    trace.stop = Some(stop);
                    return trace;
                }
            }
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
