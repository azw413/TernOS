extern crate alloc;

use alloc::{format, string::String, vec, vec::Vec};

use crate::{
    image_viewer::{AppSource, ImageEntry, ImageError},
    prc_app::{
        PrcInfo,
        cpu::{core, memory},
        runtime,
        traps::table,
    },
};
use crate::prc_app::{bootstrap, trap_stub};

const PRC_EXEC_STEP_LIMIT: usize = 262_144;
const PRC_EXEC_STUB_STEP_BUDGET: usize = 1_000_000;
const PRC_EXEC_TRAP_STUB_LIMIT: usize = 4_096;

#[derive(Clone)]
struct TrapStat {
    trap_word: u16,
    count: usize,
    first_pc: u32,
    handled: bool,
}

pub fn log_prc_runtime_first_trap<S: AppSource>(source: &mut S, path: &[String], entry: &ImageEntry, info: &PrcInfo, verbose_logs: bool) {
    let code_id = info
        .code_scan
        .iter()
        .find(|scan| scan.resource_id == 1)
        .map(|scan| scan.resource_id)
        .or_else(|| info.code_scan.first().map(|scan| scan.resource_id));
    let Some(code_id) = code_id else {
        log::info!("PRC exec_trace skipped: no code resources");
        return;
    };

    let code0 = source
        .load_prc_code_resource(path, entry, 0)
        .ok();

    let code = match source
        .load_prc_code_resource(path, entry, code_id)
    {
        Ok(code) => code,
        Err(ImageError::Unsupported) => {
            log::info!("PRC exec_trace skipped: source does not provide code bytes");
            return;
        }
        Err(err) => {
            log::info!("PRC exec_trace failed: {:?}", err);
            return;
        }
    };
    if code.len() < 2 {
        log::info!("PRC exec_trace skipped: code#{} too small ({} bytes)", code_id, code.len());
        return;
    }
    let prc_raw = source.load_prc_bytes(path, entry).ok();
    let prc_resources = prc_raw
        .as_deref()
        .map(bootstrap::parse_prc_resource_blobs)
        .unwrap_or_default();

    #[derive(Clone)]
    struct BootstrapRun {
        entry_pc: u32,
        is_primary: bool,
        total_steps: usize,
        trap_index: usize,
        event_count: usize,
        has_startup_trap: bool,
        has_app_loop_trap: bool,
        unknown_total: u32,
        unknown_samples: Vec<(u32, u16)>,
        pc_samples: Vec<(u32, u32)>,
        recent_pcs: Vec<u32>,
        stop_reason: Option<core::StopReason>,
        event_lines: Vec<String>,
        launch_cmd: u16,
        launch_flags: u16,
        cmd_pbp: u32,
        final_sr: u16,
        final_d: [u32; 8],
        final_a: [u32; 8],
        final_mem_at_a2: [u8; 8],
        final_frame_at_a6: Option<[u32; 4]>,
        a2_changes: Vec<(usize, u32, u32)>,
        trap_stats: Vec<TrapStat>,
        default_stubbed_traps: Vec<u16>,
    }

    let primary_entry = code0
        .as_deref()
        .and_then(|c0| bootstrap::derive_prc_entry_from_code0(c0, code.len() as u32))
        .unwrap_or_else(|| bootstrap::derive_prc_entry_in_code1(&code));
    let mut candidates: Vec<(u32, bool)> = Vec::new();
    let mut push_candidate = |pc: u32, is_primary: bool| {
        if (pc as usize) < code.len() && !candidates.iter().any(|(p, _)| *p == pc) {
            candidates.push((pc, is_primary));
        }
    };
    push_candidate(primary_entry, true);
    // Secondary launch stubs observed in the wild (still validated by trap/lifecycle signals).
    push_candidate(bootstrap::derive_prc_entry_in_code1(&code), false);
    push_candidate(0, false);
    push_candidate(4, false);
    let mut link_added = 0usize;
    let scan_limit = code.len().min(4096);
    let mut i = 0usize;
    while i + 1 < scan_limit && link_added < 16 {
        let w = u16::from_be_bytes([code[i], code[i + 1]]);
        if w == 0x4E56 {
            push_candidate(i as u32, false);
            link_added += 1;
        }
        i += 2;
    }

    let run_candidate = |entry_pc: u32,
                         is_primary: bool,
                         launch_cmd: u16,
                         launch_flags: u16,
                         cmd_pbp: u32,
                         code: &[u8]|
     -> BootstrapRun {
        let mut cpu = core::CpuState68k::default();
        cpu.pc = entry_pc;
        let mut runtime_ctx = runtime::PrcRuntimeContext::default();
        runtime_ctx.launch_cmd = launch_cmd;
        // Palm OS commonly marks globals as relocated when globals exist.
        runtime_ctx.launch_flags = if (launch_flags & 0x0004) != 0 {
            launch_flags | 0x0080
        } else {
            launch_flags
        };
        runtime_ctx.cmd_pbp = cmd_pbp;
        runtime_ctx.resources = prc_resources.clone();
        runtime_ctx.prc_image = prc_raw.clone().unwrap_or_default();
        let mut memory = memory::MemoryMap::with_data(0, code.to_vec());
        // Seed a minimal globals world from data#0 when available; many apps rely on A5 globals.
        if let Some(globals_res) = runtime_ctx
            .resources
            .iter()
            .find(|r| r.kind == u32::from_be_bytes(*b"data") && r.id == 0)
            .cloned()
        {
            let size = globals_res.data.len().max(256) as u32;
            let handle = runtime_ctx.next_handle;
            runtime_ctx.next_handle = runtime_ctx.next_handle.saturating_add(1);
            let ptr = runtime_ctx.next_ptr;
            runtime_ctx.next_ptr = runtime_ctx
                .next_ptr
                .saturating_add(size.max(16).saturating_add(16));
            memory.upsert_overlay(ptr, globals_res.data.clone());
            runtime_ctx.mem_blocks.push(runtime::MemBlock {
                handle,
                ptr,
                size,
                locked: true,
                data: globals_res.data,
                resource_kind: Some(u32::from_be_bytes(*b"data")),
                resource_id: Some(0),
            });
            cpu.a[5] = ptr;
        }
        // Seed a simple launch frame so stack-based argument reads can work.
        // Use a larger synthetic stack region to avoid false frame/return corruption
        // during deeper app startup paths.
        let stack_base = 0x00FC_0000u32;
        let mut stack = vec![0u8; 256 * 1024];
        let mut sp = stack_base + stack.len() as u32;
        let push_u32 = |stack: &mut [u8], sp: &mut u32, v: u32| {
            *sp = sp.saturating_sub(4);
            let off = (*sp).saturating_sub(stack_base) as usize;
            if off + 4 <= stack.len() {
                stack[off..off + 4].copy_from_slice(&v.to_be_bytes());
            }
        };
        let push_u16 = |stack: &mut [u8], sp: &mut u32, v: u16| {
            *sp = sp.saturating_sub(2);
            let off = (*sp).saturating_sub(stack_base) as usize;
            if off + 2 <= stack.len() {
                stack[off..off + 2].copy_from_slice(&v.to_be_bytes());
            }
        };
        // Palm-style PilotMain(cmd: UInt16, cmdPBP: void*, launchFlags: UInt16)
        // pushed right-to-left, plus fake return address.
        push_u16(&mut stack, &mut sp, runtime_ctx.launch_flags);
        push_u32(&mut stack, &mut sp, runtime_ctx.cmd_pbp);
        push_u16(&mut stack, &mut sp, runtime_ctx.launch_cmd);
        push_u32(&mut stack, &mut sp, u32::MAX);
        memory.upsert_overlay(stack_base, stack);
        cpu.a[7] = sp;
        cpu.call_stack.push(u32::MAX);
        bootstrap::seed_prc_launch_registers(&mut cpu, &runtime_ctx);
        let mut total_steps = 0usize;
        let mut trap_index = 0usize;
        let mut event_count = 0usize;
        let mut has_startup_trap = false;
        let mut has_app_loop_trap = false;
        let mut unknown_total = 0u32;
        let mut unknown_samples: Vec<(u32, u16)> = Vec::new();
        let mut pc_samples: Vec<(u32, u32)> = Vec::new();
        let mut recent_pcs: Vec<u32> = Vec::new();
        let mut a2_changes: Vec<(usize, u32, u32)> = Vec::new();
        let mut stop_reason: Option<core::StopReason> = None;
        let mut event_lines: Vec<String> = Vec::new();
        let mut trap_stats: Vec<TrapStat> = Vec::new();
        loop {
                let trace = core::run_with_config(
                    &mut cpu,
                    &mut memory,
                    core::ExecConfig {
                    step_limit: PRC_EXEC_STEP_LIMIT,
                    max_events: 128,
                    trap15_action: core::Trap15Action::Continue,
                    stop_on_atrap: true,
                    stop_on_unknown: false,
                },
            );
            total_steps = total_steps.saturating_add(trace.steps);
            unknown_total = unknown_total.saturating_add(trace.unknown_count);
            for sample in trace.unknown_samples {
                if unknown_samples.len() >= 16 {
                    break;
                }
                if !unknown_samples.iter().any(|(pc, w)| *pc == sample.0 && *w == sample.1) {
                    unknown_samples.push(sample);
                }
            }
            for (pc, cnt) in trace.pc_samples {
                if let Some((_, total)) = pc_samples.iter_mut().find(|(p, _)| *p == pc) {
                    *total = total.saturating_add(cnt);
                } else if pc_samples.len() < 64 {
                    pc_samples.push((pc, cnt));
                }
            }
            if !trace.recent_pcs.is_empty() {
                recent_pcs = trace.recent_pcs;
            }
            for (step, pc, a2) in trace.a2_changes {
                if a2_changes.len() >= 64 {
                    break;
                }
                if a2_changes
                    .last()
                    .map(|(s, p, v)| *s == step && *p == pc && *v == a2)
                    .unwrap_or(false)
                {
                    continue;
                }
                a2_changes.push((step, pc, a2));
            }

            for event in &trace.events {
                match event {
                    core::StepEvent::ATrap { trap_word, pc } => {
                        event_count = event_count.saturating_add(1);
                        trap_index = trap_index.saturating_add(1);
                        if let Some(stat) = trap_stats.iter_mut().find(|s| s.trap_word == *trap_word) {
                            stat.count = stat.count.saturating_add(1);
                        } else {
                            trap_stats.push(TrapStat {
                                trap_word: *trap_word,
                                count: 1,
                                first_pc: *pc,
                                handled: trap_stub::is_prc_runtime_trap_handled(*trap_word),
                            });
                        }
                        if *trap_word == 0xA08F {
                            has_startup_trap = true;
                        }
                        if matches!(*trap_word, 0xA11D | 0xA1A0 | 0xA1BF | 0xA173 | 0xA19F) {
                            has_app_loop_trap = true;
                        }
                        let meta = table::lookup(*trap_word);
                        let handled = trap_stub::is_prc_runtime_trap_handled(*trap_word);
                        if event_lines.len() < 48 {
                            event_lines.push(format!(
                                "PRC exec_trace event[{}] trap=0x{:04X} group={} name={} pc=0x{:X} handled={}",
                                trap_index,
                                trap_word,
                                meta.group.as_str(),
                                meta.name,
                                pc,
                                handled
                            ));
                        }
                    }
                    core::StepEvent::Trap15 { pc, selector } => {
                        event_count = event_count.saturating_add(1);
                        if event_lines.len() < 48 {
                            if let Some(sel) = selector {
                                event_lines.push(format!(
                                    "PRC exec_trace event trap15 pc=0x{:X} selector=0x{:04X}",
                                    pc, sel
                                ));
                            } else {
                                event_lines.push(format!(
                                    "PRC exec_trace event trap15 pc=0x{:X} selector=?",
                                    pc
                                ));
                            }
                        }
                    }
                    core::StepEvent::Trap { vector, pc } => {
                        event_count = event_count.saturating_add(1);
                        if event_lines.len() < 48 {
                            event_lines.push(format!("PRC exec_trace event trap#{} pc=0x{:X}", vector, pc));
                        }
                    }
                }
            }

            let Some(stop) = trace.stop else {
                break;
            };
            match stop {
                core::StopReason::ATrap { trap_word, pc }
                    if trap_stub::is_prc_runtime_trap_handled(trap_word)
                        && trap_index < PRC_EXEC_TRAP_STUB_LIMIT
                        && total_steps < PRC_EXEC_STUB_STEP_BUDGET =>
                {
                    trap_stub::apply_prc_runtime_trap_stub(
                        &mut cpu,
                        &mut runtime_ctx,
                        &mut memory,
                        trap_word,
                        pc,
                    );
                    continue;
                }
                other => {
                    stop_reason = Some(other);
                    break;
                }
            }
        }
        BootstrapRun {
            entry_pc,
            is_primary,
            total_steps,
            trap_index,
            event_count,
            has_startup_trap,
            has_app_loop_trap,
            unknown_total,
            unknown_samples,
            pc_samples,
            recent_pcs,
            stop_reason,
            event_lines,
            launch_cmd: runtime_ctx.launch_cmd,
            launch_flags: runtime_ctx.launch_flags,
            cmd_pbp: runtime_ctx.cmd_pbp,
            final_sr: cpu.sr,
            final_d: cpu.d,
            final_a: cpu.a,
            final_mem_at_a2: {
                let mut bytes = [0u8; 8];
                let base = cpu.a[2];
                let mut idx = 0usize;
                while idx < bytes.len() {
                    bytes[idx] = memory.read_u8(base.saturating_add(idx as u32)).unwrap_or(0);
                    idx += 1;
                }
                bytes
            },
            final_frame_at_a6: {
                let a6 = cpu.a[6];
                if a6 != 0 {
                    Some([
                        memory.read_u32_be(a6).unwrap_or(0),
                        memory.read_u32_be(a6.saturating_add(4)).unwrap_or(0),
                        memory.read_u32_be(a6.saturating_add(8)).unwrap_or(0),
                        memory.read_u32_be(a6.saturating_add(12)).unwrap_or(0),
                    ])
                } else {
                    None
                }
            },
            a2_changes,
            trap_stats,
            default_stubbed_traps: runtime_ctx.default_stubbed_traps.clone(),
        }
    };

    let mut runs: Vec<BootstrapRun> = candidates
        .iter()
        .copied()
        .map(|(pc, is_primary)| run_candidate(pc, is_primary, 0, 0x000C, 0, &code))
        .collect();
    runs.sort_by(|a, b| {
        b.has_app_loop_trap
            .cmp(&a.has_app_loop_trap)
            .then_with(|| b.has_startup_trap.cmp(&a.has_startup_trap))
            .then_with(|| {
                let b_step = matches!(b.stop_reason, Some(core::StopReason::StepLimit { .. }));
                let a_step = matches!(a.stop_reason, Some(core::StopReason::StepLimit { .. }));
                a_step.cmp(&b_step)
            })
            .then_with(|| b.is_primary.cmp(&a.is_primary))
            .then_with(|| b.event_count
            .cmp(&a.event_count)
            .then_with(|| b.trap_index.cmp(&a.trap_index))
            .then_with(|| b.total_steps.cmp(&a.total_steps))
            .then_with(|| b.entry_pc.cmp(&a.entry_pc)))
    });
    if verbose_logs {
        for run in &runs {
            let stop = match run.stop_reason {
                Some(core::StopReason::ATrap { trap_word, .. }) => {
                    let meta = table::lookup(trap_word);
                    format!("trap 0x{:04X} {}", trap_word, meta.name)
                }
                Some(core::StopReason::Trap15 { .. }) => "trap15".into(),
                Some(core::StopReason::Trap { vector, .. }) => {
                    format!("trap#{}", vector)
                }
                Some(core::StopReason::OutOfBounds { .. }) => "out_of_bounds".into(),
                Some(core::StopReason::UnknownOpcode { word, .. }) => {
                    format!("unknown 0x{:04X}", word)
                }
                Some(core::StopReason::ReturnUnderflow { .. }) => "return_underflow".into(),
                Some(core::StopReason::EntryReturn { .. }) => "entry_return".into(),
                Some(core::StopReason::StepLimit { .. }) => "step_limit".into(),
                None => "none".into(),
            };
            log::info!(
                "PRC bootstrap cand entry=0x{:X} primary={} events={} traps={} steps={} startup={} app_loop={} stop={}",
                run.entry_pc,
                run.is_primary,
                run.event_count,
                run.trap_index,
                run.total_steps,
                run.has_startup_trap,
                run.has_app_loop_trap,
                stop
            );
        }
    }
    let best = runs
        .first()
        .cloned()
        .unwrap_or_else(|| run_candidate(primary_entry, true, 0, 0x000C, 0, &code));
    let mut best = best;
    let launch_scenarios = [
        ("normal", 0u16, 0x000C_u16, 0u32),
        ("goto_new_globals", 2u16, 0x000C_u16, 0u32),
        ("goto_subcall", 2u16, 0x0018_u16, 0u32),
        ("goto_subcall_new_globals", 2u16, 0x001C_u16, 0u32),
    ];
    if !best.has_app_loop_trap {
        let mut scenario_runs: Vec<(&str, BootstrapRun)> = launch_scenarios
            .iter()
            .map(|(label, cmd, flags, pbp)| {
                (
                    *label,
                    run_candidate(best.entry_pc, best.is_primary, *cmd, *flags, *pbp, &code),
                )
            })
            .collect();
        scenario_runs.sort_by(|a, b| {
            let ar = &a.1;
            let br = &b.1;
            br.has_app_loop_trap
                .cmp(&ar.has_app_loop_trap)
                .then_with(|| br.has_startup_trap.cmp(&ar.has_startup_trap))
                .then_with(|| {
                    let b_step =
                        matches!(br.stop_reason, Some(core::StopReason::StepLimit { .. }));
                    let a_step =
                        matches!(ar.stop_reason, Some(core::StopReason::StepLimit { .. }));
                    a_step.cmp(&b_step)
                })
                .then_with(|| br.event_count.cmp(&ar.event_count))
                .then_with(|| br.trap_index.cmp(&ar.trap_index))
                .then_with(|| br.total_steps.cmp(&ar.total_steps))
        });
        if let Some((label, top)) = scenario_runs.first() {
            if top.has_app_loop_trap
                || (!best.has_startup_trap && top.has_startup_trap)
                || (best.stop_reason
                    == Some(core::StopReason::EntryReturn { pc: 0 })
                    && top.stop_reason
                        != Some(core::StopReason::EntryReturn { pc: 0 }))
                || (top.event_count > best.event_count)
            {
                log::info!(
                    "PRC launch_scenario selected='{}' cmd={} flags=0x{:04X}",
                    label,
                    top.launch_cmd,
                    top.launch_flags
                );
                best = top.clone();
            }
        }
    }
    log::info!(
        "PRC bootstrap candidates={} selected_entry=0x{:X} selected_events={} selected_traps={} selected_steps={} startup_trap={} app_loop_trap={}",
        candidates.len(),
        best.entry_pc,
        best.event_count,
        best.trap_index,
        best.total_steps,
        best.has_startup_trap,
        best.has_app_loop_trap
    );
    let entry_pc = best.entry_pc;
    let total_steps = best.total_steps;
    let _trap_index = best.trap_index;
    let unknown_total = best.unknown_total;
    let unknown_samples = best.unknown_samples;
    let mut pc_samples = best.pc_samples;
    let stop_reason = best.stop_reason;
    let recent_pcs = best.recent_pcs;
    let final_sr = best.final_sr;
    let final_d = best.final_d;
    let final_a = best.final_a;
    let final_mem_at_a2 = best.final_mem_at_a2;
    let final_frame_at_a6 = best.final_frame_at_a6;
    let a2_changes = best.a2_changes;
    let mut trap_stats = best.trap_stats;
    let default_stubbed_traps = best.default_stubbed_traps;
    for line in best.event_lines {
        log::info!("{}", line);
    }
    log::info!(
        "PRC exec_trace start code_id={} code_size={} entry_pc=0x{:X} launch_cmd={} launch_flags=0x{:04X} cmd_pbp=0x{:X} steps={}",
        code_id,
        code.len(),
        entry_pc,
        best.launch_cmd,
        best.launch_flags,
        best.cmd_pbp,
        total_steps
    );
    if unknown_total > 0 {
        log::info!("PRC exec_trace unknown_count={}", unknown_total);
        for (pc, word) in unknown_samples {
            log::info!("PRC exec_trace unknown pc=0x{:X} word=0x{:04X}", pc, word);
        }
    }
    if matches!(stop_reason, Some(core::StopReason::StepLimit { .. })) {
        pc_samples.sort_by(|a, b| b.1.cmp(&a.1));
        for (pc, cnt) in pc_samples.into_iter().take(8) {
            let word = if (pc as usize) + 1 < code.len() {
                u16::from_be_bytes([code[pc as usize], code[pc as usize + 1]])
            } else {
                0
            };
            log::info!(
                "PRC exec_trace hotspot pc=0x{:X} hits={} word=0x{:04X}",
                pc,
                cnt,
                word
            );
        }
    }
    match stop_reason {
        Some(core::StopReason::ATrap { trap_word, pc }) => {
            let meta = table::lookup(trap_word);
            log::info!(
                "PRC exec_trace stop trap=0x{:04X} group={} name={} pc=0x{:X}",
                trap_word,
                meta.group.as_str(),
                meta.name,
                pc
            );
        }
        Some(core::StopReason::Trap15 { pc }) => {
            log::info!("PRC exec_trace stop trap15 pc=0x{:X}", pc);
        }
        Some(core::StopReason::Trap { vector, pc }) => {
            log::info!("PRC exec_trace stop trap#{} pc=0x{:X}", vector, pc);
        }
        Some(core::StopReason::OutOfBounds { pc }) => {
            log::info!("PRC exec_trace stop out_of_bounds pc=0x{:X}", pc);
            if !recent_pcs.is_empty() {
                for p in recent_pcs {
                    let w = if (p as usize) + 1 < code.len() {
                        u16::from_be_bytes([code[p as usize], code[p as usize + 1]])
                    } else {
                        0
                    };
                    log::info!("PRC exec_trace recent_pc pc=0x{:X} word=0x{:04X}", p, w);
                }
            }
        }
        Some(core::StopReason::UnknownOpcode { pc, word }) => {
            log::info!(
                "PRC exec_trace stop unknown_opcode pc=0x{:X} word=0x{:04X}",
                pc,
                word
            );
        }
        Some(core::StopReason::ReturnUnderflow { pc }) => {
            log::info!("PRC exec_trace stop return_underflow pc=0x{:X}", pc);
        }
        Some(core::StopReason::EntryReturn { pc }) => {
            log::info!("PRC exec_trace stop entry_return pc=0x{:X}", pc);
        }
        Some(core::StopReason::StepLimit { pc }) => {
            log::info!("PRC exec_trace stop step_limit pc=0x{:X}", pc);
        }
        None => {
            log::info!("PRC exec_trace stop none");
        }
    }
    if matches!(stop_reason, Some(core::StopReason::StepLimit { .. })) {
        log::info!(
            "PRC exec_trace regs sr=0x{:04X} d0={:#X} d1={:#X} d2={:#X} d3={:#X} d4={:#X} a0={:#X} a1={:#X} a2={:#X} a3={:#X} a5={:#X} a6={:#X} a7={:#X}",
            final_sr,
            final_d[0],
            final_d[1],
            final_d[2],
            final_d[3],
            final_d[4],
            final_a[0],
            final_a[1],
            final_a[2],
            final_a[3],
            final_a[5],
            final_a[6],
            final_a[7]
        );
        log::info!(
            "PRC exec_trace mem a2 bytes={:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            final_mem_at_a2[0],
            final_mem_at_a2[1],
            final_mem_at_a2[2],
            final_mem_at_a2[3],
            final_mem_at_a2[4],
            final_mem_at_a2[5],
            final_mem_at_a2[6],
            final_mem_at_a2[7]
        );
        for (step, pc, a2) in a2_changes.into_iter().take(24) {
            let word = if (pc as usize) + 1 < code.len() {
                u16::from_be_bytes([code[pc as usize], code[pc as usize + 1]])
            } else {
                0
            };
            log::info!(
                "PRC exec_trace a2_change step={} pc=0x{:X} word=0x{:04X} a2=0x{:X}",
                step,
                pc,
                word,
                a2
            );
        }
        for pc in (0x280usize..=0x29Cusize).step_by(2) {
            if pc + 1 >= code.len() {
                break;
            }
            let word = u16::from_be_bytes([code[pc], code[pc + 1]]);
            log::info!("PRC exec_trace focus_code pc=0x{:X} word=0x{:04X}", pc, word);
        }
        if let Some(frame) = final_frame_at_a6 {
            log::info!(
                "PRC exec_trace frame a6=0x{:X} [0]=0x{:08X} [4]=0x{:08X} [8]=0x{:08X} [12]=0x{:08X}",
                final_a[6], frame[0], frame[1], frame[2], frame[3]
            );
        }
        let mut pc = 0x270usize;
        let end = 0x2B8usize.min(code.len().saturating_sub(1));
        while pc + 1 < end {
            let word = u16::from_be_bytes([code[pc], code[pc + 1]]);
            log::info!("PRC exec_trace loop_code pc=0x{:X} word=0x{:04X}", pc, word);
            pc += 2;
        }
    }
    let total_traps: usize = trap_stats.iter().map(|s| s.count).sum();
    trap_stats.sort_by(|a, b| b.count.cmp(&a.count));
    log::info!(
        "PRC trap_census unique={} total={} default_stubbed={}",
        trap_stats.len(),
        total_traps,
        default_stubbed_traps.len()
    );
    for stat in trap_stats.into_iter().take(24) {
        let meta = table::lookup(stat.trap_word);
        log::info!(
            "PRC trap_census trap=0x{:04X} group={} name={} count={} first_pc=0x{:X} handled={}",
            stat.trap_word,
            meta.group.as_str(),
            meta.name,
            stat.count,
            stat.first_pc,
            stat.handled
        );
    }
}
