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
const PRC_EXEC_TRAP_STUB_LIMIT: usize = 65_536;
const PRC_IDLE_LOOP_POLLS: u32 = 4;

#[derive(Clone, Debug, Default)]
pub struct RuntimeUiSnapshot {
    pub form_id: Option<u16>,
    pub bitmap_draws: Vec<RuntimeBitmapDraw>,
    pub field_draws: Vec<RuntimeFieldDraw>,
}

#[derive(Clone, Debug)]
pub struct RuntimeBitmapDraw {
    pub resource_id: u16,
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeFieldDraw {
    pub form_id: u16,
    pub field_id: u16,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeRunState {
    Running,
    BlockedOnEvent { timeout_ticks: u32 },
    Stopped(core::StopReason),
}

#[derive(Clone, Debug)]
pub struct RuntimeRunOutput {
    pub snapshot: RuntimeUiSnapshot,
    pub state: RuntimeRunState,
    pub steps: usize,
}

pub struct PrcRuntimeSession {
    cpu: core::CpuState68k,
    memory: memory::MemoryMap,
    runtime: runtime::PrcRuntimeContext,
    stopped: bool,
}

impl PrcRuntimeSession {
    pub fn from_source<S: AppSource>(
        source: &mut S,
        path: &[String],
        entry: &ImageEntry,
        info: &PrcInfo,
        tick_seed: u32,
    ) -> Result<Self, ImageError> {
        const PRC_CODE_BASE: u32 = 0x0000_1000;
        let code_id = info
            .code_scan
            .iter()
            .find(|scan| scan.resource_id == 1)
            .map(|scan| scan.resource_id)
            .or_else(|| info.code_scan.first().map(|scan| scan.resource_id))
            .ok_or(ImageError::Unsupported)?;
        let code = source.load_prc_code_resource(path, entry, code_id)?;
        let code0 = source.load_prc_code_resource(path, entry, 0).ok();
        if code.len() < 2 {
            return Err(ImageError::Unsupported);
        }
        let prc_raw = source.load_prc_bytes(path, entry).ok();
        let prc_resources = prc_raw
            .as_deref()
            .map(bootstrap::parse_prc_resource_blobs)
            .unwrap_or_default();
        let system_resources = source.load_prc_system_resources();
        let system_fonts = source.load_prc_system_fonts();

        let mut cpu = core::CpuState68k::default();
        let mut runtime_ctx = runtime::PrcRuntimeContext::default();
        runtime_ctx.launch_cmd = 0;
        runtime_ctx.launch_flags = 0x008C;
        runtime_ctx.cmd_pbp = 0;
        runtime_ctx.ticks = tick_seed;
        runtime_ctx.trace_traps = true;
        runtime_ctx.trace_trap_budget = 100_000;
        runtime_ctx.block_on_evt_get_event = true;
        runtime_ctx.resources = prc_resources;
        runtime_ctx.resources.extend(system_resources);
        runtime_ctx.fonts = crate::prc_app::font::load_nfnt_fonts(&runtime_ctx.resources);
        for font in system_fonts {
            if let Some(existing) = runtime_ctx
                .fonts
                .iter_mut()
                .find(|f| f.font_id == font.font_id)
            {
                *existing = font;
            } else {
                runtime_ctx.fonts.push(font);
            }
        }
        runtime_ctx.prc_image = prc_raw.unwrap_or_default();
        if !runtime_ctx.prc_image.is_empty() {
            let parsed_forms = crate::prc_app::form_preview::parse_form_previews(&runtime_ctx.prc_image);
            let mut next_ptr = 0x3002_0000u32;
            for form in &parsed_forms {
                for (idx, obj) in form.objects.iter().enumerate() {
                    let (object_id, kind) = match obj {
                        crate::prc_app::form_preview::FormPreviewObject::Field { id, .. } => {
                            (*id, runtime::RuntimeFormObjectKind::Field)
                        }
                        crate::prc_app::form_preview::FormPreviewObject::Button { id, .. } => {
                            (*id, runtime::RuntimeFormObjectKind::Other)
                        }
                        _ => (0, runtime::RuntimeFormObjectKind::Other),
                    };
                    runtime_ctx.form_objects.push(runtime::RuntimeFormObject {
                        form_id: form.form_id,
                        object_index: idx as u16,
                        object_id,
                        kind,
                        ptr: next_ptr,
                        text_handle: 0,
                    });
                    next_ptr = next_ptr.saturating_add(0x20);
                }
            }
        }
        let mut memory = memory::MemoryMap::with_data(PRC_CODE_BASE, code.clone());
        let code_handle = runtime_ctx.next_handle;
        runtime_ctx.next_handle = runtime_ctx.next_handle.saturating_add(1);
        runtime_ctx.mem_blocks.push(runtime::MemBlock {
            handle: code_handle,
            ptr: PRC_CODE_BASE,
            size: code.len() as u32,
            locked: true,
            data: code,
            resource_kind: Some(u32::from_be_bytes(*b"code")),
            resource_id: Some(code_id),
                });
        // SysAppInfo.codeH is consumed by startup glue as an opaque pointer-like
        // value; use the mapped code base address to match Palm/Pumpkin layout.
        runtime_ctx.code_handle = PRC_CODE_BASE;
        // Build A5 world from code#0/data#0 layout when available.
        // Per Palm/Pumpkin contract:
        // code0[0..4] = aboveA5 size, code0[4..8] = belowA5(data) size.
        if let Some(code0_bytes) = code0.as_deref() {
            if code0_bytes.len() >= 8 {
                let above_size = u32::from_be_bytes([
                    code0_bytes[0],
                    code0_bytes[1],
                    code0_bytes[2],
                    code0_bytes[3],
                ]);
                let data_size = u32::from_be_bytes([
                    code0_bytes[4],
                    code0_bytes[5],
                    code0_bytes[6],
                    code0_bytes[7],
                ]);
                let total = above_size.saturating_add(data_size);
                if (16..=1_048_576).contains(&total) {
                    let handle = runtime_ctx.next_handle;
                    runtime_ctx.next_handle = runtime_ctx.next_handle.saturating_add(1);
                    let ptr = runtime_ctx.next_ptr;
                    runtime_ctx.next_ptr = runtime_ctx
                        .next_ptr
                        .saturating_add(total.max(16).saturating_add(16));
                    let data = vec![0u8; total as usize];
                    memory.upsert_overlay(ptr, data.clone());
                    runtime_ctx.mem_blocks.push(runtime::MemBlock {
                        handle,
                        ptr,
                        size: total,
                        locked: true,
                        data,
                        resource_kind: None,
                        resource_id: None,
                    });
                    if let Some(data0_blob) = runtime_ctx
                        .resources
                        .iter()
                        .find(|r| r.kind == u32::from_be_bytes(*b"data") && r.id == 0)
                        .cloned()
                    {
                        let _ = bootstrap::decode_data0_globals_into_memory(
                            code0_bytes,
                            &data0_blob.data,
                            &mut memory,
                            ptr,
                            PRC_CODE_BASE,
                        );
                    }
                    runtime_ctx.globals_ptr = ptr;
                    runtime_ctx.prev_globals_ptr = 0;
                    // A5 points to end of data segment (below-A5 globals).
                    cpu.a[5] = ptr.saturating_add(data_size);
                }
            }
        }

        // Fallback for PRCs with no usable code#0 metadata.
        if cpu.a[5] == 0 {
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
                runtime_ctx.globals_ptr = ptr;
                runtime_ctx.prev_globals_ptr = 0;
                cpu.a[5] = ptr.saturating_add(size);
            }
        }

        let stack_base = 0x00FC_0000u32;
        let mut stack = Vec::new();
        let mut stack_len = 0usize;
        for candidate in [64 * 1024usize, 32 * 1024, 16 * 1024, 8 * 1024, 4 * 1024, 2 * 1024] {
            if stack.try_reserve_exact(candidate).is_ok() {
                stack_len = candidate;
                break;
            }
        }
        if stack_len == 0 {
            stack_len = 256;
            let _ = stack.try_reserve_exact(stack_len);
        }
        stack.resize(stack_len.min(stack.capacity()), 0);
        let sp = stack_base + stack.len() as u32 - 16;
        memory.upsert_overlay(stack_base, stack);
        // Seed a synthetic caller return so top-level RTS cleanly stops the session.
        let _ = memory.write_u32_be(sp, u32::MAX);
        cpu.a[7] = sp;
        // Match Pumpkin's 68k launch contract: start execution at the beginning
        // of code #1. code #0 is used for globals/relocation metadata, not entry PC.
        cpu.pc = PRC_CODE_BASE;

        Ok(Self {
            cpu,
            memory,
            runtime: runtime_ctx,
            stopped: false,
        })
    }

    pub fn resume(&mut self) -> RuntimeRunOutput {
        if self.stopped {
            return RuntimeRunOutput {
                snapshot: self.snapshot(),
                state: RuntimeRunState::Stopped(core::StopReason::EntryReturn { pc: self.cpu.pc }),
                steps: 0,
            };
        }
        self.runtime.terminate_requested = false;
        self.runtime.blocked_on_evt_get_event = false;
        self.runtime.blocked_evt_timeout_ticks = 0;

        let mut total_steps = 0usize;
        loop {
            let trace = core::run_with_config(
                &mut self.cpu,
                &mut self.memory,
                core::ExecConfig {
                    step_limit: PRC_EXEC_STEP_LIMIT,
                    max_events: 128,
                    trap15_action: core::Trap15Action::Continue,
                    stop_on_atrap: true,
                    stop_on_unknown: true,
                },
            );
            total_steps = total_steps.saturating_add(trace.steps);
            let Some(stop) = trace.stop else {
                break;
            };
            match stop {
                core::StopReason::ATrap { trap_word, pc }
                    if trap_stub::is_prc_runtime_trap_handled(trap_word)
                        && total_steps < PRC_EXEC_STUB_STEP_BUDGET =>
                {
                    trap_stub::apply_prc_runtime_trap_stub(
                        &mut self.cpu,
                        &mut self.runtime,
                        &mut self.memory,
                        trap_word,
                        pc,
                    );
                    if self.runtime.terminate_requested {
                        self.runtime.terminate_requested = false;
                        if self.runtime.blocked_on_evt_get_event {
                            return RuntimeRunOutput {
                                snapshot: self.snapshot(),
                                state: RuntimeRunState::BlockedOnEvent {
                                    timeout_ticks: self.runtime.blocked_evt_timeout_ticks.max(1),
                                },
                                steps: total_steps,
                            };
                        }
                    }
                    continue;
                }
                other => {
                    self.stopped = true;
                    return RuntimeRunOutput {
                        snapshot: self.snapshot(),
                        state: RuntimeRunState::Stopped(other),
                        steps: total_steps,
                    };
                }
            }
        }

        RuntimeRunOutput {
            snapshot: self.snapshot(),
            state: RuntimeRunState::Running,
            steps: total_steps,
        }
    }

    pub fn queue_nil_event(&mut self) {
        self.runtime.event_queue.push(runtime::RuntimeEvent {
            e_type: runtime::EVT_NIL,
            data_u16: 0,
        });
    }

    pub fn queue_control_select(&mut self, control_id: u16) {
        let evt = runtime::RuntimeEvent {
            e_type: runtime::EVT_CTL_SELECT,
            data_u16: control_id,
        };
        self.runtime.event_queue.insert(0, evt);
        self.runtime.pending_dispatch_event = Some(evt);
        log::info!(
            "PRC runtime input queued ctlSelect control_id={} qlen={}",
            control_id,
            self.runtime.event_queue.len()
        );
    }

    pub fn inject_control_select_now(&mut self, control_id: u16) {
        let event_p = self.runtime.evt_event_p;
        if event_p != 0 && self.memory.contains_addr(event_p) {
            let _ = self.memory.write_u16_be(event_p, runtime::EVT_CTL_SELECT);
            let _ = self.memory.write_u16_be(event_p.saturating_add(2), 0);
            let _ = self.memory.write_u16_be(event_p.saturating_add(4), 0);
            let _ = self.memory.write_u16_be(event_p.saturating_add(6), 0);
            let _ = self.memory.write_u16_be(event_p.saturating_add(8), control_id);
            let _ = self
                .memory
                .write_u32_be(event_p.saturating_add(10), 0x3001_0000u32);
            let _ = self.memory.write_u8(event_p.saturating_add(14), 1);
            let _ = self.memory.write_u8(event_p.saturating_add(15), 0);
            let _ = self.memory.write_u16_be(event_p.saturating_add(16), 0);
            log::info!(
                "PRC runtime input injected ctlSelect eventP=0x{:08X} control_id={}",
                event_p,
                control_id
            );
        } else {
            log::info!(
                "PRC runtime input inject skipped (no event buffer) control_id={}",
                control_id
            );
        }
        self.queue_control_select(control_id);
    }

    fn snapshot(&self) -> RuntimeUiSnapshot {
        RuntimeUiSnapshot {
            form_id: self.runtime.drawn_form_id.or(self.runtime.active_form_id),
            bitmap_draws: self
                .runtime
                .drawn_bitmaps
                .iter()
                .map(|d| RuntimeBitmapDraw {
                    resource_id: d.resource_id,
                    x: d.x,
                    y: d.y,
                })
                .collect(),
            field_draws: self
                .runtime
                .field_draws
                .iter()
                .map(|f| RuntimeFieldDraw {
                    form_id: f.form_id,
                    field_id: f.field_id,
                    text: f.text.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Clone)]
struct TrapStat {
    trap_word: u16,
    count: usize,
    first_pc: u32,
    handled: bool,
}

pub fn log_prc_runtime_first_trap<S: AppSource>(
    source: &mut S,
    path: &[String],
    entry: &ImageEntry,
    info: &PrcInfo,
    verbose_logs: bool,
) -> RuntimeUiSnapshot {
    log_prc_runtime_first_trap_with_seed(source, path, entry, info, verbose_logs, 0)
}

pub fn log_prc_runtime_first_trap_with_seed<S: AppSource>(
    source: &mut S,
    path: &[String],
    entry: &ImageEntry,
    info: &PrcInfo,
    verbose_logs: bool,
    tick_seed: u32,
) -> RuntimeUiSnapshot {
    let code_id = info
        .code_scan
        .iter()
        .find(|scan| scan.resource_id == 1)
        .map(|scan| scan.resource_id)
        .or_else(|| info.code_scan.first().map(|scan| scan.resource_id));
    let Some(code_id) = code_id else {
        log::info!("PRC exec_trace skipped: no code resources");
        return RuntimeUiSnapshot::default();
    };

    let code = match source
        .load_prc_code_resource(path, entry, code_id)
    {
        Ok(code) => code,
        Err(ImageError::Unsupported) => {
            log::info!("PRC exec_trace skipped: source does not provide code bytes");
            return RuntimeUiSnapshot::default();
        }
        Err(err) => {
            log::info!("PRC exec_trace failed: {:?}", err);
            return RuntimeUiSnapshot::default();
        }
    };
    if code.len() < 2 {
        log::info!("PRC exec_trace skipped: code#{} too small ({} bytes)", code_id, code.len());
            return RuntimeUiSnapshot::default();
    }
    let prc_raw = source.load_prc_bytes(path, entry).ok();
    let prc_resources = prc_raw
        .as_deref()
        .map(bootstrap::parse_prc_resource_blobs)
        .unwrap_or_default();
    let system_resources = source.load_prc_system_resources();
    let system_fonts = source.load_prc_system_fonts();

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
        active_form_id: Option<u16>,
        drawn_form_id: Option<u16>,
        drawn_bitmaps: Vec<RuntimeBitmapDraw>,
    }

    let primary_entry = 0;
    let mut candidates: Vec<(u32, bool)> = Vec::new();
    let mut push_candidate = |pc: u32, is_primary: bool| {
        if (pc as usize) < code.len() && !candidates.iter().any(|(p, _)| *p == pc) {
            candidates.push((pc, is_primary));
        }
    };
    push_candidate(primary_entry, true);

    let run_candidate = |entry_pc: u32,
                         is_primary: bool,
                         launch_cmd: u16,
                         launch_flags: u16,
                         cmd_pbp: u32,
                         code: &[u8],
                         trace_traps: bool,
                         collect_debug: bool|
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
        runtime_ctx.ticks = tick_seed;
        runtime_ctx.trace_traps = trace_traps;
        runtime_ctx.cmd_pbp = cmd_pbp;
        runtime_ctx.resources = prc_resources.clone();
        runtime_ctx.resources.extend(system_resources.clone());
        runtime_ctx.fonts = crate::prc_app::font::load_nfnt_fonts(&runtime_ctx.resources);
        for font in system_fonts.clone() {
            if let Some(existing) = runtime_ctx
                .fonts
                .iter_mut()
                .find(|f| f.font_id == font.font_id)
            {
                *existing = font;
            } else {
                runtime_ctx.fonts.push(font);
            }
        }
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
        // Seed a synthetic stack frame so stack-based argument reads can work.
        // Keep this allocation adaptive for constrained targets (x4 firmware heap).
        let stack_base = 0x00FC_0000u32;
        let mut stack = Vec::new();
        let mut stack_len = 0usize;
        for candidate in [64 * 1024usize, 32 * 1024, 16 * 1024, 8 * 1024, 4 * 1024, 2 * 1024] {
            if stack.try_reserve_exact(candidate).is_ok() {
                stack_len = candidate;
                break;
            }
        }
        if stack_len == 0 {
            // Extreme low-memory fallback: keep enough bytes for launch args.
            stack_len = 256;
            let _ = stack.try_reserve_exact(stack_len);
        }
        stack.resize(stack_len.min(stack.capacity()), 0);
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
            if collect_debug {
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
            }

            for event in &trace.events {
                match event {
                    core::StepEvent::ATrap { trap_word, pc } => {
                        event_count = event_count.saturating_add(1);
                        trap_index = trap_index.saturating_add(1);
                        if collect_debug {
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
                        }
                        if *trap_word == 0xA08F {
                            has_startup_trap = true;
                        }
                        if matches!(*trap_word, 0xA11D | 0xA1A0 | 0xA1BF | 0xA173 | 0xA19F | 0xA470) {
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
                    // If we've clearly entered a stable GUI event loop, treat it as a successful boot.
                    if has_app_loop_trap
                        && runtime_ctx.evt_polls >= PRC_IDLE_LOOP_POLLS
                        && matches!(trap_word, 0xA11D | 0xA0A9 | 0xA1BF)
                    {
                        stop_reason = Some(core::StopReason::EntryReturn { pc });
                        break;
                    }
                    trap_stub::apply_prc_runtime_trap_stub(
                        &mut cpu,
                        &mut runtime_ctx,
                        &mut memory,
                        trap_word,
                        pc,
                    );
                    if runtime_ctx.terminate_requested {
                        stop_reason = Some(core::StopReason::ATrap { trap_word, pc });
                        break;
                    }
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
            active_form_id: runtime_ctx.active_form_id,
            drawn_form_id: runtime_ctx.drawn_form_id,
            drawn_bitmaps: runtime_ctx
                .drawn_bitmaps
                .iter()
                .map(|d| RuntimeBitmapDraw {
                    resource_id: d.resource_id,
                    x: d.x,
                    y: d.y,
                })
                .collect(),
        }
    };

    let mut runs: Vec<BootstrapRun> = candidates
        .iter()
        .copied()
        .map(|(pc, is_primary)| run_candidate(pc, is_primary, 0, 0x000C, 0, &code, false, verbose_logs))
        .collect();
    runs.sort_by(|a, b| {
        b.drawn_bitmaps.len()
            .cmp(&a.drawn_bitmaps.len())
            .then_with(|| b.drawn_form_id.is_some().cmp(&a.drawn_form_id.is_some()))
            .then_with(|| b.has_startup_trap.cmp(&a.has_startup_trap))
            .then_with(|| b.has_app_loop_trap.cmp(&a.has_app_loop_trap))
            .then_with(|| {
                let b_step = matches!(b.stop_reason, Some(core::StopReason::StepLimit { .. }));
                let a_step = matches!(a.stop_reason, Some(core::StopReason::StepLimit { .. }));
                a_step.cmp(&b_step)
            })
            .then_with(|| b.is_primary.cmp(&a.is_primary))
            .then_with(|| b.event_count.cmp(&a.event_count))
            .then_with(|| b.trap_index.cmp(&a.trap_index))
            .then_with(|| b.total_steps.cmp(&a.total_steps))
            .then_with(|| b.entry_pc.cmp(&a.entry_pc))
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
        .unwrap_or_else(|| run_candidate(primary_entry, true, 0, 0x000C, 0, &code, false, false));
    let mut best = best;
    let launch_scenarios = [
        ("normal", 0u16, 0x000C_u16, 0u32),
        ("goto_new_globals", 2u16, 0x000C_u16, 0u32),
        ("goto_subcall", 2u16, 0x0018_u16, 0u32),
        ("goto_subcall_new_globals", 2u16, 0x001C_u16, 0u32),
    ];
    if !best.has_app_loop_trap || best.drawn_form_id.is_none() || best.drawn_bitmaps.is_empty() {
        let mut scenario_runs: Vec<(&str, BootstrapRun)> = launch_scenarios
            .iter()
            .map(|(label, cmd, flags, pbp)| {
                (
                    *label,
                    run_candidate(
                        best.entry_pc,
                        best.is_primary,
                        *cmd,
                        *flags,
                        *pbp,
                        &code,
                        false,
                        false,
                    ),
                )
            })
            .collect();
        scenario_runs.sort_by(|a, b| {
            let ar = &a.1;
            let br = &b.1;
            br.drawn_bitmaps.len()
                .cmp(&ar.drawn_bitmaps.len())
                .then_with(|| br.drawn_form_id.is_some().cmp(&ar.drawn_form_id.is_some()))
                .then_with(|| br.has_startup_trap.cmp(&ar.has_startup_trap))
                .then_with(|| br.has_app_loop_trap.cmp(&ar.has_app_loop_trap))
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
            if (top.drawn_form_id.is_some() && best.drawn_form_id.is_none())
                || (top.drawn_bitmaps.len() > best.drawn_bitmaps.len())
                || (top.has_app_loop_trap && !best.has_app_loop_trap)
                || (!best.has_startup_trap && top.has_startup_trap)
                || (best.stop_reason
                    == Some(core::StopReason::EntryReturn { pc: 0 })
                    && top.stop_reason
                        != Some(core::StopReason::EntryReturn { pc: 0 }))
                || (top.event_count > best.event_count)
            {
                let _ = label;
                best = top.clone();
            }
        }
    }
    log::info!(
        "PRC exec start code_id={} code_size={} entry_pc=0x{:X} launch_cmd={} launch_flags=0x{:04X} cmd_pbp=0x{:X}",
        code_id,
        code.len(),
        best.entry_pc,
        best.launch_cmd,
        best.launch_flags,
        best.cmd_pbp
    );
    let best = run_candidate(
        best.entry_pc,
        best.is_primary,
        best.launch_cmd,
        best.launch_flags,
        best.cmd_pbp,
        &code,
        true,
        false,
    );
    let total_steps = best.total_steps;
    let stop_reason = best.stop_reason;
    let snapshot = RuntimeUiSnapshot {
        form_id: best.drawn_form_id.or(best.active_form_id),
        bitmap_draws: best.drawn_bitmaps.clone(),
        field_draws: Vec::new(),
    };
    let stop_text = match stop_reason {
        Some(core::StopReason::ATrap { trap_word, pc }) => {
            let meta = table::lookup(trap_word);
            format!("trap=0x{:04X} {} pc=0x{:X}", trap_word, meta.name, pc)
        }
        Some(core::StopReason::Trap15 { pc }) => format!("trap15 pc=0x{:X}", pc),
        Some(core::StopReason::Trap { vector, pc }) => format!("trap#{} pc=0x{:X}", vector, pc),
        Some(core::StopReason::OutOfBounds { pc }) => format!("out_of_bounds pc=0x{:X}", pc),
        Some(core::StopReason::UnknownOpcode { pc, word }) => {
            format!("unknown_opcode pc=0x{:X} word=0x{:04X}", pc, word)
        }
        Some(core::StopReason::ReturnUnderflow { pc }) => format!("return_underflow pc=0x{:X}", pc),
        Some(core::StopReason::EntryReturn { pc }) => format!("entry_return pc=0x{:X}", pc),
        Some(core::StopReason::StepLimit { pc }) => format!("step_limit pc=0x{:X}", pc),
        None => String::from("none"),
    };
    log::info!(
        "PRC exec end stop={} steps={}",
        stop_text,
        total_steps
    );
    snapshot
}
