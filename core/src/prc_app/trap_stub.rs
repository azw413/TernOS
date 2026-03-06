extern crate alloc;

use alloc::{vec, vec::Vec};

use crate::prc_app::{
    cpu::{core::CpuState68k, memory::MemoryMap},
    runtime::{FeatureEntry, MemBlock, PrcRuntimeContext, RuntimeEvent, EVT_FRM_LOAD, EVT_FRM_OPEN, EVT_NIL},
};

pub fn is_prc_runtime_trap_handled(trap_word: u16) -> bool {
    if (0xA000..=0xAFFF).contains(&trap_word) {
        return true;
    }
    matches!(
        trap_word,
        0xA08F | // sysTrapSysAppStartup
        0xA090 | // sysTrapSysAppExit
        0xA0C2 | // sysTrapSysRandom (stub for probing)
        0xA0A9 | // sysTrapSysHandleEvent (stub for probing)
        0xA0BA | // sysTrapSysLibFind (stub for probing)
        0xA2AC | // sysTrapSysLibLoad (stub for probing)
        0xA0C5 | // sysTrapStrCopy (stub for probing)
        0xA0C7 | // sysTrapStrLen (stub for probing)
        0xA0F7 | // sysTrapTimGetTicks (stub for probing)
        0xA11D | // sysTrapEvtGetEvent (stub for probing)
        0xA060 | // sysTrapDmGet1Resource (stub for probing)
        0xA01E | // sysTrapMemHandleNew (stub for probing)
        0xA021 | // sysTrapMemHandleLock (stub for probing)
        0xA022 | // sysTrapMemHandleUnlock (stub for probing)
        0xA02B | // sysTrapMemHandleFree (stub for probing)
        0xA027 | // sysTrapMemSet (stub for probing)
        0xA013 | // memTrap (treated as MemPtrNew-like during probing)
        0xA05F | // sysTrapDmGetResource (stub for probing)
        0xA226 | // sysTrapWinDrawBitmap (stub for probing)
        0xA234 | // sysTrapSndPlaySystemSound (no-op for now)
        0xA27B | // sysTrapFtrGet (minimal feature store)
        0xA27C | // sysTrapFtrSet (minimal feature store)
        0xA2E9 | // sysTrapSysTicksPerSecond (stub for probing)
        0xA061 | // sysTrapDmReleaseResource (stub for probing)
        0xA16F | // sysTrapFrmInitForm (stub for probing)
        0xA171 | // sysTrapFrmDrawForm (stub for probing)
        0xA173 | // sysTrapFrmGetActiveForm (stub for probing)
        0xA174 | // sysTrapFrmSetActiveForm (stub for probing)
        0xA180 | // sysTrapFrmGetObjectIndex (stub for probing)
        0xA183 | // sysTrapFrmGetObjectPtr (stub for probing)
        0xA195 | // sysTrapFrmHelp (stub for probing)
        0xA19F | // sysTrapFrmSetEventHandler (stub for probing)
        0xA1BF | // sysTrapMenuHandleEvent (stub for probing)
        0xA200 | // sysTrapWinGetDisplayWindow (stub for probing)
        0xA456 | // sysTrapWinGetBounds (stub for probing)
        0xA163 | // sysTrapFntGetFont (stub for probing)
        0xA164 | // sysTrapFntSetFont (stub for probing)
        0xA167 | // sysTrapFntCharHeight (stub for probing)
        0xA16B | // sysTrapFntCharsWidth (stub for probing)
        0xA16D | // sysTrapFntCharsInWidth (stub for probing)
        0xA153 | // sysTrapFldGetTextHandle (stub for probing)
        0xA158 | // sysTrapFldSetTextHandle (stub for probing)
        0xA135 | // sysTrapFldDrawField (stub for probing)
        0xA19B | // sysTrapFrmGotoForm (no-op for probing flow)
        0xA1A1 | // sysTrapFrmCloseAllForms (stub for probing)
        0xA1A0 | // sysTrapFrmDispatchEvent (stub for probing)
        0xA9F0   // libTrapDispatch (safe stub for probing)
    )
}

pub fn apply_prc_runtime_trap_stub(
    cpu: &mut CpuState68k,
    runtime: &mut PrcRuntimeContext,
    memory: &mut MemoryMap,
    trap_word: u16,
    pc: u32,
) {
    if !runtime.default_stubbed_traps.contains(&trap_word) {
        runtime.default_stubbed_traps.push(trap_word);
    }
    let trap_meta = crate::prc_app::traps::table::lookup(trap_word);
    let sp = cpu.a[7];
    let s0 = memory.read_u16_be(sp).unwrap_or(0);
    let s1 = memory.read_u16_be(sp.saturating_add(2)).unwrap_or(0);
    let s2 = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0);
    let s3 = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0);
    if runtime.trace_traps && runtime.trace_trap_budget > 0 {
        log::info!(
            "PRC trap call pc=0x{:X} trap=0x{:04X} group={} name={} d0=0x{:08X} d1=0x{:08X} a0=0x{:08X} a1=0x{:08X} a7=0x{:08X} sp[0..8]={:04X} {:04X} {:04X} {:04X}",
            pc,
            trap_word,
            trap_meta.group.as_str(),
            trap_meta.name,
            cpu.d[0],
            cpu.d[1],
            cpu.a[0],
            cpu.a[1],
            cpu.a[7],
            s0,
            s1,
            s2,
            s3
        );
        runtime.trace_trap_budget = runtime.trace_trap_budget.saturating_sub(1);
    }

    fn resolve_handle(runtime: &PrcRuntimeContext, raw: u32) -> Option<u32> {
        if raw == 0 {
            return None;
        }
        if runtime.mem_blocks.iter().any(|b| b.handle == raw) {
            return Some(raw);
        }
        runtime
            .mem_blocks
            .iter()
            .find(|b| b.ptr == raw)
            .map(|b| b.handle)
    }

    fn decode_handle_arg(
        runtime: &PrcRuntimeContext,
        cpu: &CpuState68k,
        memory: &MemoryMap,
    ) -> u32 {
        let sp = cpu.a[7];
        let candidates = [memory.read_u32_be(sp).unwrap_or(0)];
        for raw in candidates {
            if let Some(handle) = resolve_handle(runtime, raw) {
                return handle;
            }
        }
        0
    }

    fn alloc_mem(
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        data: Vec<u8>,
        resource_kind: Option<u32>,
        resource_id: Option<u16>,
    ) -> (u32, u32) {
        let size = data.len().max(16) as u32;
        let handle = runtime.next_handle;
        runtime.next_handle = runtime.next_handle.saturating_add(1);
        let ptr = runtime.next_ptr;
        runtime.next_ptr = runtime
            .next_ptr
            .saturating_add(size.max(16).saturating_add(16));
        let block = MemBlock {
            handle,
            ptr,
            size,
            locked: false,
            data,
            resource_kind,
            resource_id,
        };
        memory.upsert_overlay(block.ptr, block.data.clone());
        runtime.mem_blocks.push(block);
        (handle, ptr)
    }

    fn get_font(runtime: &PrcRuntimeContext, font_id: u16) -> Option<&crate::prc_app::runtime::PalmFont> {
        runtime.fonts.iter().find(|f| f.font_id == font_id)
    }

    fn palm_font_metrics(font_id: u16) -> (u32, u32) {
        // Fallback metrics when no NFNT resource is available.
        // std=0, bold=1, large=2, symbol=3, symbol11=4, symbol7=5, led=6, largeBold=7
        match font_id {
            2 | 7 => (8, 14),
            4 => (8, 11),
            5 => (5, 7),
            _ => (6, 10),
        }
    }

    fn current_font_metrics(runtime: &PrcRuntimeContext) -> (u32, u32) {
        if let Some(f) = get_font(runtime, runtime.current_font) {
            let avg = f.avg_width.max(1) as u32;
            let h = f.rect_height.max(1) as u32;
            return (avg, h);
        }
        palm_font_metrics(runtime.current_font)
    }

    fn current_char_width(runtime: &PrcRuntimeContext, ch: u8) -> u32 {
        if let Some(f) = get_font(runtime, runtime.current_font) {
            if ch >= f.first_char && ch <= f.last_char {
                let idx = (ch - f.first_char) as usize;
                return f.widths.get(idx).copied().unwrap_or(f.avg_width).max(1) as u32;
            }
            // Palm uses a missing symbol width; use avg/max as practical fallback.
            return f.avg_width.max(f.max_width).max(1) as u32;
        }
        let (w, _) = palm_font_metrics(runtime.current_font);
        w
    }

    fn chars_width(runtime: &PrcRuntimeContext, memory: &MemoryMap, ptr: u32, len: u32) -> u32 {
        let mut width = 0u32;
        let mut i = 0u32;
        while i < len {
            let ch = memory.read_u8(ptr.saturating_add(i)).unwrap_or(0);
            width = width.saturating_add(current_char_width(runtime, ch));
            i = i.saturating_add(1);
        }
        width
    }

    fn lock_handle(
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        handle: u32,
    ) -> Option<u32> {
        for block in &mut runtime.mem_blocks {
            if block.handle == handle {
                block.locked = true;
                memory.upsert_overlay(block.ptr, block.data.clone());
                return Some(block.ptr);
            }
        }
        None
    }

    fn free_handle(runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap, handle: u32) -> bool {
        if let Some(pos) = runtime.mem_blocks.iter().position(|b| b.handle == handle) {
            let block = runtime.mem_blocks.swap_remove(pos);
            memory.remove_overlay(block.ptr);
            true
        } else {
            false
        }
    }

    fn write_bytes(runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap, ptr: u32, bytes: &[u8]) {
        for block in &mut runtime.mem_blocks {
            let start = block.ptr;
            let end = block.ptr.saturating_add(block.size);
            if ptr < start || ptr >= end {
                continue;
            }
            let off = (ptr - start) as usize;
            let max = block.data.len().saturating_sub(off);
            if max == 0 {
                return;
            }
            let n = bytes.len().min(max);
            block.data[off..off + n].copy_from_slice(&bytes[..n]);
            memory.upsert_overlay(block.ptr, block.data.clone());
            return;
        }
    }

    fn fill_bytes(runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap, ptr: u32, len: u32, val: u8) {
        if len == 0 {
            return;
        }
        for block in &mut runtime.mem_blocks {
            let start = block.ptr;
            let end = block.ptr.saturating_add(block.size);
            if ptr < start || ptr >= end {
                continue;
            }
            let off = (ptr - start) as usize;
            let max = block.data.len().saturating_sub(off);
            if max == 0 {
                return;
            }
            let n = (len as usize).min(max);
            block.data[off..off + n].fill(val);
            memory.upsert_overlay(block.ptr, block.data.clone());
            return;
        }
    }

    fn read_c_string(memory: &MemoryMap, ptr: u32) -> Vec<u8> {
        if ptr == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for i in 0..512u32 {
            let addr = ptr.saturating_add(i);
            let Some(b) = memory.read_u8(addr) else {
                break;
            };
            if b == 0 {
                break;
            }
            out.push(b);
        }
        out
    }

    fn find_resource_for_ptr(runtime: &PrcRuntimeContext, ptr: u32) -> Option<(u32, u16)> {
        runtime
            .mem_blocks
            .iter()
            .find(|b| ptr >= b.ptr && ptr < b.ptr.saturating_add(b.size))
            .and_then(|b| match (b.resource_kind, b.resource_id) {
                (Some(k), Some(id)) => Some((k, id)),
                _ => None,
            })
    }

    fn decode_form_handle(cpu: &CpuState68k, memory: &MemoryMap) -> Option<u32> {
        let sp = cpu.a[7];
        let candidates = [
            cpu.a[0],
            cpu.d[0],
            memory.read_u32_be(sp).unwrap_or(0),
            memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
            memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
            memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
        ];
        candidates
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
    }

    fn select_resource_data(
        runtime: &PrcRuntimeContext,
        kind_hint: u32,
        id_hint: u16,
    ) -> Option<(u32, u16, Vec<u8>)> {
        runtime
            .resources
            .iter()
            .find(|res| res.kind == kind_hint && res.id == id_hint)
            .map(|res| (res.kind, res.id, res.data.clone()))
    }

    fn maybe_log_resource_pick(
        runtime: &mut PrcRuntimeContext,
        kind_hint: u32,
        id_hint: u16,
        picked_kind: u32,
        picked_id: u16,
        _size: usize,
    ) {
        let tuple = (kind_hint, id_hint, picked_kind, picked_id);
        if runtime.dm_get_resource_last_log == Some(tuple) {
            return;
        }
        runtime.dm_get_resource_last_log = Some(tuple);
    }

    fn decode_dm_resource_args(
        cpu: &CpuState68k,
        memory: &MemoryMap,
    ) -> (u32, u16) {
        let sp = cpu.a[7];
        // Palm ABI for DmGetResource/DmGet1Resource:
        // type (UInt32), id (UInt16).
        let kind = memory.read_u32_be(sp).unwrap_or(0);
        let id = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0);
        (kind, id)
    }

    fn write_event(memory: &mut MemoryMap, event_p: u32, e_type: u16, data_u16: u16) {
        if event_p == 0 || !memory.contains_addr(event_p) {
            return;
        }
        // EventType starts with eType (UInt16). Palm event union payload
        // used by frmLoad/frmOpen formID is at +8.
        let _ = memory.write_u16_be(event_p, e_type);
        let _ = memory.write_u16_be(event_p.saturating_add(2), 0);
        let _ = memory.write_u16_be(event_p.saturating_add(4), 0);
        let _ = memory.write_u16_be(event_p.saturating_add(6), 0);
        let _ = memory.write_u16_be(event_p.saturating_add(8), data_u16);
        // Keep a minimal generic payload consistent with eType to help glue code
        // that aliases the union through generic fields.
        let _ = memory.write_u16_be(event_p.saturating_add(10), e_type);
    }

    fn decode_ptr_arg_from_stack(
        cpu: &CpuState68k,
        memory: &MemoryMap,
        arg_offset: u32,
    ) -> u32 {
        let sp = cpu.a[7];
        let raw = memory.read_u32_be(sp.saturating_add(arg_offset)).unwrap_or(0);
        if raw != 0 && memory.contains_addr(raw) {
            return raw;
        }
        // Palm 68K glue frequently passes frame-relative locals as 16-bit offsets
        // (signed or unsigned representation depending on codegen).
        if raw <= 0xFFFF || (raw & 0xFFFF_0000) == 0xFFFF_0000 {
            let off_signed = (raw as u16) as i16 as i32;
            let signed_candidate = if off_signed >= 0 {
                cpu.a[6].wrapping_add(off_signed as u32)
            } else {
                cpu.a[6].wrapping_sub((-off_signed) as u32)
            };
            if signed_candidate != 0 && memory.contains_addr(signed_candidate) {
                return signed_candidate;
            }
            let off_unsigned = (raw as u16) as u32;
            let unsigned_candidate = cpu.a[6].wrapping_add(off_unsigned);
            if unsigned_candidate != 0 && memory.contains_addr(unsigned_candidate) {
                return unsigned_candidate;
            }
        }
        raw
    }

    match trap_word {
        0xA08F => {
            runtime.shutting_down = false;
            runtime.active_form_id = None;
            runtime.active_form_handle = 0x3000_0000;
            runtime.active_form_handler = 0;
            runtime.event_queue.clear();
            runtime.evt_polls = 0;
            runtime.blink_next_tick = 175;
            runtime.blink_phase = 0;
            if runtime.sys_app_info_ptr == 0 {
                let (_h, ptr) = alloc_mem(runtime, memory, vec![0u8; 128], None, None);
                runtime.sys_app_info_ptr = ptr;
            }
            // SysAppStartup(appInfoPP, prevGlobalsP, globalsPtrP)
            // (matching Pumpkin/Palm contract).
            let sp = cpu.a[7];
            let app_info_pp = memory.read_u32_be(sp).unwrap_or(0);
            let prev_globals_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
            let globals_ptr_p = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
            if runtime.sys_app_info_ptr != 0 {
                let _ = memory.write_u16_be(runtime.sys_app_info_ptr, runtime.launch_cmd);
                let _ = memory.write_u32_be(
                    runtime.sys_app_info_ptr.saturating_add(2),
                    runtime.cmd_pbp,
                );
                let _ = memory.write_u16_be(
                    runtime.sys_app_info_ptr.saturating_add(6),
                    runtime.launch_flags,
                );
                // codeH: current code#1 handle.
                let _ = memory.write_u32_be(
                    runtime.sys_app_info_ptr.saturating_add(12),
                    runtime.code_handle,
                );
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail SysAppStartup appInfo=0x{:08X} cmd=0x{:04X} cmdPBP=0x{:08X} flags=0x{:04X} codeH=0x{:08X}",
                        runtime.sys_app_info_ptr,
                        runtime.launch_cmd,
                        runtime.cmd_pbp,
                        runtime.launch_flags,
                        runtime.code_handle
                    );
                }
            }
            if app_info_pp != 0 && memory.contains_addr(app_info_pp) {
                let _ = memory.write_u32_be(app_info_pp, runtime.sys_app_info_ptr);
            }
            if prev_globals_p != 0 && memory.contains_addr(prev_globals_p) {
                let _ = memory.write_u32_be(prev_globals_p, runtime.prev_globals_ptr);
            }
            if globals_ptr_p != 0 && memory.contains_addr(globals_ptr_p) {
                let _ = memory.write_u32_be(globals_ptr_p, runtime.globals_ptr);
            }
            cpu.d[0] = 0;
        }
        0xA090 => {
            runtime.shutting_down = true;
            cpu.d[0] = 0;
        }
        0xA19B => {
            // FrmGotoForm(formID): queue a deterministic Palm-style transition.
            let sp = cpu.a[7];
            let form_id = memory.read_u16_be(sp).unwrap_or(0);
            runtime.event_queue.push(RuntimeEvent {
                e_type: EVT_FRM_LOAD,
                data_u16: form_id,
            });
            runtime.event_queue.push(RuntimeEvent {
                e_type: EVT_FRM_OPEN,
                data_u16: form_id,
            });
            cpu.d[0] = 0;
        }
        0xA173 => {
            cpu.a[0] = runtime.active_form_handle;
        }
        0xA174 => {
            // void FrmSetActiveForm(FormType *formP)
            let sp = cpu.a[7];
            let form_h = memory.read_u32_be(sp).unwrap_or(0);
            if form_h != 0 {
                runtime.active_form_handle = form_h;
                runtime.active_form_id = Some((form_h & 0xFFFF) as u16);
            }
        }
        0xA16F => {
            let sp = cpu.a[7];
            let mut form_id = memory.read_u16_be(sp).unwrap_or(0);
            let evt_type = memory.read_u16_be(runtime.evt_event_p).unwrap_or(0xFFFF);
            let evt_form = memory
                .read_u16_be(runtime.evt_event_p.saturating_add(8))
                .unwrap_or(0);
            if (evt_type == EVT_FRM_LOAD || evt_type == EVT_FRM_OPEN)
                && evt_form != 0
                && form_id != evt_form
            {
                form_id = evt_form;
            }
            let has_form_resource = runtime
                .resources
                .iter()
                .any(|res| res.kind == u32::from_be_bytes(*b"tFRM") && res.id == form_id);
            if !has_form_resource {
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    let ep = runtime.evt_event_p;
                    let e_type = memory.read_u16_be(ep).unwrap_or(0xFFFF);
                    let e_form = memory.read_u16_be(ep.saturating_add(8)).unwrap_or(0xFFFF);
                    log::info!(
                        "PRC trap detail FrmInitForm invalid form_id=0x{:04X} evt_p=0x{:08X} evt_type={} evt_form=0x{:04X}",
                        form_id,
                        ep,
                        e_type,
                        e_form
                    );
                }
                cpu.a[0] = 0;
                cpu.d[0] = 0;
                return;
            }
            let form_h = 0x3000_0000u32 | (form_id as u32);
            cpu.a[0] = form_h;
            runtime.active_form_id = Some(form_id);
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                let ep = runtime.evt_event_p;
                let e_type = memory.read_u16_be(ep).unwrap_or(0xFFFF);
                let e_form = memory.read_u16_be(ep.saturating_add(8)).unwrap_or(0xFFFF);
                log::info!(
                    "PRC trap detail FrmInitForm form_id=0x{:04X} evt_p=0x{:08X} evt_type={} evt_form=0x{:04X}",
                    form_id,
                    ep,
                    e_type,
                    e_form
                );
            }
        }
        0xA171 => {
            if runtime.drawn_form_id.is_none() {
                runtime.drawn_form_id = runtime.active_form_id;
            }
            cpu.d[0] = 0;
        }
        0xA19F => {
            // FrmSetEventHandler(formP, handlerP)
            let sp = cpu.a[7];
            runtime.active_form_handle = memory.read_u32_be(sp).unwrap_or(runtime.active_form_handle);
            runtime.active_form_handler = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
            cpu.d[0] = 0;
        }
        0xA17C => {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
        }
        0xA084 => {
            // ErrDisplayFileLineMsg: swallow diagnostics/assert displays in exploratory mode.
            cpu.d[0] = 0;
        }
        0xA1A0 => {
            // FrmDispatchEvent(eventP): call active form handler if set.
            // If no app handler exists, emulate Palm's default frmOpen handling
            // by drawing the active form once.
            let mut event_p = decode_ptr_arg_from_stack(cpu, memory, 0);
            if !memory.contains_addr(event_p) {
                event_p = runtime.evt_event_p;
            }
            let evt_type = memory.read_u16_be(event_p).unwrap_or(0xFFFF);
            if runtime.active_form_handler != 0 {
                let ret_pc = cpu.pc;
                cpu.a[7] = cpu.a[7].wrapping_sub(4);
                let _ = memory.write_u32_be(cpu.a[7], ret_pc);
                cpu.pc = runtime.active_form_handler;
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail FrmDispatchEvent call handler=0x{:08X} eventP=0x{:08X}",
                        runtime.active_form_handler,
                        event_p
                    );
                }
                return;
            }
            if evt_type == EVT_FRM_OPEN {
                if runtime.drawn_form_id.is_none() {
                    runtime.drawn_form_id = runtime.active_form_id;
                }
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail FrmDispatchEvent default-open eventP=0x{:08X} form={:?}",
                        event_p,
                        runtime.drawn_form_id
                    );
                }
                cpu.d[0] = 1;
            } else {
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail FrmDispatchEvent default-pass eventP=0x{:08X} eType={}",
                        event_p,
                        evt_type
                    );
                }
                cpu.d[0] = 0;
            }
        }
        0xA0A9 | 0xA1BF => {
            cpu.d[0] = 0;
        }
        0xA11D => {
            runtime.evt_polls = runtime.evt_polls.saturating_add(1);
            let sp = cpu.a[7];
            let event_p = decode_ptr_arg_from_stack(cpu, memory, 0);
            runtime.evt_event_p = event_p;
            // EvtGetEvent(eventP, timeout): advance simulated ticks by timeout.
            let timeout = [
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as u32,
                memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0) as u32,
                cpu.d[0],
                (cpu.d[0] & 0xFFFF) as u32,
            ]
            .into_iter()
            .find(|v| *v > 0 && *v < 10_000)
            .unwrap_or(1);
            if let Some(evt) = runtime.event_queue.first().copied() {
                let _ = runtime.event_queue.remove(0);
                write_event(memory, event_p, evt.e_type, evt.data_u16);
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    let rb_type = memory.read_u16_be(event_p).unwrap_or(0xFFFF);
                    let rb_form = memory
                        .read_u16_be(event_p.saturating_add(8))
                        .unwrap_or(0xFFFF);
                    log::info!(
                        "PRC trap detail EvtGetEvent eventP=0x{:08X} deliver eType={} data=0x{:04X} rb_type={} rb_form=0x{:04X}",
                        event_p,
                        evt.e_type,
                        evt.data_u16,
                        rb_type,
                        rb_form
                    );
                }
                runtime.blocked_on_evt_get_event = false;
                runtime.blocked_evt_timeout_ticks = 0;
                cpu.d[0] = 0;
                return;
            }
            runtime.ticks = runtime.ticks.saturating_add(timeout.max(1));
            write_event(memory, event_p, EVT_NIL, 0);
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                let rb_type = memory.read_u16_be(event_p).unwrap_or(0xFFFF);
                log::info!(
                    "PRC trap detail EvtGetEvent eventP=0x{:08X} deliver eType={} timeout={} rb_type={}",
                    event_p,
                    EVT_NIL,
                    timeout,
                    rb_type
                );
            }
            if runtime.block_on_evt_get_event && !runtime.shutting_down {
                runtime.blocked_on_evt_get_event = true;
                runtime.blocked_evt_timeout_ticks = timeout.max(1);
                runtime.terminate_requested = true;
            }
        }
        0xA01E => {
            let size = cpu.d[0].max(16);
            let data = vec![0u8; size as usize];
            let (handle, _ptr) = alloc_mem(runtime, memory, data, None, None);
            cpu.a[0] = handle;
        }
        0xA021 => {
            let handle = decode_handle_arg(runtime, cpu, memory);
            if let Some(ptr) = lock_handle(runtime, memory, handle) {
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail MemHandleLock handle=0x{:08X} -> ptr=0x{:08X}",
                        handle, ptr
                    );
                }
                cpu.a[0] = ptr;
                if cpu.a[2] < 0x0001_0000 {
                    cpu.a[2] = ptr;
                }
            } else {
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail MemHandleLock handle=0x{:08X} -> null",
                        handle
                    );
                }
                cpu.a[0] = 0;
            }
        }
        0xA022 => {
            let handle = decode_handle_arg(runtime, cpu, memory);
            for block in &mut runtime.mem_blocks {
                if block.handle == handle {
                    block.locked = false;
                    break;
                }
            }
            cpu.d[0] = 0;
        }
        0xA02B => {
            let handle = decode_handle_arg(runtime, cpu, memory);
            cpu.d[0] = if free_handle(runtime, memory, handle) { 0 } else { 1 };
        }
        0xA013 => {
            let size = cpu.d[0].max(16);
            let data = vec![0u8; size as usize];
            let (_handle, ptr) = alloc_mem(runtime, memory, data, None, None);
            cpu.a[0] = ptr;
            cpu.d[0] = ptr;
        }
        0xA027 => {
            // MemSet(dst, value, count) style decode from stack/registers.
            let sp = cpu.a[7];
            let s0 = memory.read_u32_be(sp).unwrap_or(0);
            let s1 = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
            let s2 = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
            let looks_ptr = |p: u32, runtime: &PrcRuntimeContext| {
                runtime.mem_blocks.iter().any(|b| p >= b.ptr && p < b.ptr.saturating_add(b.size))
                    || (p & 0xF000_0000) == 0x2000_0000
            };
            let dst = [s0, s1, s2, cpu.a[0], cpu.d[0], cpu.a[1], cpu.d[1]]
                .into_iter()
                .find(|p| looks_ptr(*p, runtime))
                .unwrap_or(cpu.a[0].max(cpu.d[0]));

            let (value, count) = if s1 <= 0xFF && s2 > 0xFF {
                (s1 as u8, s2)
            } else if s2 <= 0xFF && s1 > 0xFF {
                (s2 as u8, s1)
            } else if s0 <= 0xFF && s1 > 0xFF {
                (s0 as u8, s1)
            } else {
                let c = [s2, s1, s0, cpu.d[1], cpu.d[0]]
                    .into_iter()
                    .find(|v| *v > 0 && *v < 0x0100_0000)
                    .unwrap_or(0);
                (0, c)
            };

            fill_bytes(runtime, memory, dst, count, value);
            cpu.a[0] = dst;
            cpu.d[0] = dst;
        }
        0xA060 | 0xA05F => {
            runtime.dm_get_resource_probe_count = runtime.dm_get_resource_probe_count.saturating_add(1);
            let (kind_hint, id_hint) = decode_dm_resource_args(cpu, memory);
            if let Some((kind, id, data)) = select_resource_data(runtime, kind_hint, id_hint) {
                maybe_log_resource_pick(runtime, kind_hint, id_hint, kind, id, data.len());
                let handle = runtime
                    .mem_blocks
                    .iter()
                    .find(|b| b.resource_kind == Some(kind) && b.resource_id == Some(id))
                    .map(|b| b.handle)
                    .unwrap_or_else(|| {
                        let (h, _ptr) = alloc_mem(runtime, memory, data, Some(kind), Some(id));
                        h
                    });
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    let k = kind.to_be_bytes();
                    let kh = kind_hint.to_be_bytes();
                    log::info!(
                        "PRC trap detail DmGetResource req='{}{}{}{}'/{} -> got='{}{}{}{}'/{} handle=0x{:08X}",
                        kh[0] as char, kh[1] as char, kh[2] as char, kh[3] as char, id_hint,
                        k[0] as char, k[1] as char, k[2] as char, k[3] as char, id, handle
                    );
                }
                cpu.a[0] = handle;
            } else {
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    let kh = kind_hint.to_be_bytes();
                    log::info!(
                        "PRC trap detail DmGetResource req='{}{}{}{}'/{} -> null",
                        kh[0] as char, kh[1] as char, kh[2] as char, kh[3] as char, id_hint
                    );
                }
                cpu.a[0] = 0;
            }
        }
        0xA061 => {
            cpu.d[0] = 0;
        }
        0xA0F7 => {
            cpu.d[0] = runtime.ticks;
        }
        0xA0BA => {
            // Err SysLibFind(const Char* nameP, UInt16* refNumP)
            let sp = cpu.a[7];
            let refnum_p = [
                cpu.a[1],
                cpu.d[1],
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0),
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            if refnum_p != 0 {
                // Return a deterministic fake library refnum.
                memory.write_u16_be(refnum_p, 1);
            }
            cpu.d[0] = 0; // errNone
        }
        0xA2AC => {
            // Err SysLibLoad(UInt16 libType, UInt32 libCreator, UInt16* refNumP)
            let sp = cpu.a[7];
            let refnum_p = [
                cpu.a[0],
                cpu.a[1],
                memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0),
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            if refnum_p != 0 {
                memory.write_u16_be(refnum_p, 1);
            }
            cpu.d[0] = 0; // errNone
        }
        0xA470 => {
            // SysTrapPinsDispatch(selector-based dispatcher used by DIA/PIN APIs).
            // Prefer D2 (CALL_WITH_SELECTOR convention), then small stack words.
            let sp = cpu.a[7];
            let selector = {
                let d2 = (cpu.d[2] & 0xFFFF) as u16;
                if d2 <= 64 {
                    d2
                } else {
                    [
                        memory.read_u16_be(sp).unwrap_or(0),
                        memory.read_u16_be(sp.saturating_add(2)).unwrap_or(0),
                        memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0),
                        memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0),
                    ]
                    .into_iter()
                    .find(|v| *v <= 64)
                    .unwrap_or(0)
                }
            };
            match selector {
                0 => cpu.d[0] = 0, // PINSetInputAreaState -> errNone
                1 => cpu.d[0] = 2, // PINGetInputAreaState -> pinInputAreaHide
                2 => cpu.d[0] = 0, // PINSetInputTriggerState -> errNone
                3 => cpu.d[0] = 0, // PINGetInputTriggerState -> disabled
                13 => cpu.d[0] = 0, // WinSetConstraintsSize -> errNone
                14 => cpu.d[0] = 0, // FrmSetDIAPolicyAttr -> errNone
                15 => cpu.d[0] = 0, // FrmGetDIAPolicyAttr -> default policy
                16 | 17 => cpu.d[0] = 0, // StatHide/StatShow -> errNone
                18 => {
                    // StatGetAttribute(selector, UInt32* dataP)
                    let data_p = [
                        cpu.a[1],
                        cpu.d[1],
                        memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                        memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                        memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                    ]
                    .into_iter()
                    .find(|p| *p != 0 && memory.contains_addr(*p))
                    .unwrap_or(0);
                    if data_p != 0 {
                        memory.write_u32_be(data_p, 0);
                    }
                    cpu.d[0] = 0;
                }
                19 => cpu.d[0] = 0, // SysGetOrientation -> portrait
                20 => cpu.d[0] = 0, // SysSetOrientation -> errNone
                21 => cpu.d[0] = 0, // SysGetOrientationTriggerState -> disabled
                22 => cpu.d[0] = 0, // SysSetOrientationTriggerState -> errNone
                _ => cpu.d[0] = 0,
            }
        }
        0xA27C => {
            // Err FtrSet(UInt32 creator, UInt16 num, UInt32 value)
            let sp = cpu.a[7];
            let creator = memory.read_u32_be(sp).unwrap_or(cpu.d[0]);
            let num = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(cpu.d[1] as u16);
            let value = memory
                .read_u32_be(sp.saturating_add(6))
                .or_else(|| memory.read_u32_be(sp.saturating_add(8)))
                .unwrap_or(cpu.a[0].max(cpu.d[2]));
            if let Some(f) = runtime
                .features
                .iter_mut()
                .find(|f| f.creator == creator && f.num == num)
            {
                f.value = value;
            } else {
                runtime.features.push(FeatureEntry {
                    creator,
                    num,
                    value,
                });
            }
            cpu.d[0] = 0;
        }
        0xA27B => {
            // Err FtrGet(UInt32 creator, UInt16 num, UInt32* valueP)
            let sp = cpu.a[7];
            let creator = memory.read_u32_be(sp).unwrap_or(cpu.d[0]);
            let num = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(cpu.d[1] as u16);
            let value_p = memory
                .read_u32_be(sp.saturating_add(6))
                .or_else(|| memory.read_u32_be(sp.saturating_add(8)))
                .unwrap_or(cpu.a[0]);
            if let Some(f) = runtime
                .features
                .iter()
                .find(|f| f.creator == creator && f.num == num)
                .cloned()
            {
                memory.write_u32_be(value_p, f.value);
                cpu.d[0] = 0;
            } else {
                memory.write_u32_be(value_p, 0);
                cpu.d[0] = 1;
            }
        }
        0xA2E9 => {
            cpu.d[0] = 100;
        }
        0xA0C2 => {
            // Int16 SysRandom(Int32 newSeed)
            // Palm API returns a 15-bit value (0..sysRandomMax-1, sysRandomMax=0x7FFF).
            let sp = cpu.a[7];
            let new_seed = memory.read_u32_be(sp).unwrap_or(cpu.d[0]);
            if new_seed != 0 {
                runtime.rand_state = new_seed;
            }
            runtime.rand_state = runtime
                .rand_state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let rnd15 = (runtime.rand_state & 0x7FFF_FFFF) % 0x7FFF;
            cpu.d[0] = rnd15;
        }
        0xA0C7 => {
            let ptr = if cpu.a[0] != 0 { cpu.a[0] } else { cpu.d[0] };
            cpu.d[0] = read_c_string(memory, ptr).len() as u32;
        }
        0xA0C5 => {
            let dst = if cpu.a[0] != 0 { cpu.a[0] } else { cpu.d[0] };
            let src = if cpu.a[1] != 0 { cpu.a[1] } else { cpu.d[1] };
            let mut bytes = read_c_string(memory, src);
            bytes.push(0);
            write_bytes(runtime, memory, dst, &bytes);
            cpu.a[0] = dst;
        }
        0xA180 => {
            cpu.d[0] = 0;
        }
        0xA163 => {
            cpu.d[0] = runtime.current_font as u32;
        }
        0xA164 => {
            let old = runtime.current_font;
            let requested = (cpu.d[0] & 0xFFFF) as u16;
            // Accept requested font when installed; keep fallback-compatible behavior for core fonts.
            runtime.current_font = if get_font(runtime, requested).is_some() || requested <= 7 {
                requested
            } else {
                0
            };
            cpu.d[0] = old as u32;
        }
        0xA167 => {
            let (_, h) = current_font_metrics(runtime);
            cpu.d[0] = h;
        }
        0xA16B => {
            let sp = cpu.a[7];
            let chars_p = [cpu.a[0], cpu.d[0], memory.read_u32_be(sp).unwrap_or(0)]
                .into_iter()
                .find(|p| *p != 0 && memory.contains_addr(*p))
                .unwrap_or(0);
            let len = if cpu.d[1] != 0 {
                cpu.d[1]
            } else {
                memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as u32
            };
            if chars_p != 0 && len > 0 {
                cpu.d[0] = chars_width(runtime, memory, chars_p, len);
            } else {
                let (cw, _) = current_font_metrics(runtime);
                cpu.d[0] = len.saturating_mul(cw);
            }
        }
        0xA16D => {
            // FntCharsInWidth(charsP, stringWidthP, stringLengthP, fitWithinWidth): return chars fit.
            let (cw, _) = current_font_metrics(runtime);
            let sp = cpu.a[7];
            let chars_p = [cpu.a[0], cpu.d[0], memory.read_u32_be(sp).unwrap_or(0)]
                .into_iter()
                .find(|p| *p != 0 && memory.contains_addr(*p))
                .unwrap_or(0);
            let fit_width = [
                cpu.d[0],
                memory.read_u16_be(sp).unwrap_or(0) as u32,
                memory.read_u16_be(sp.saturating_add(2)).unwrap_or(0) as u32,
            ]
            .into_iter()
            .find(|v| *v > 0)
            .unwrap_or(0);
            let len_guess = [
                cpu.d[1],
                memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as u32,
            ]
            .into_iter()
            .find(|v| *v > 0)
            .unwrap_or(0);
            let mut fit_chars = (fit_width / cw.max(1)).min(len_guess.max(1));
            if chars_p != 0 && len_guess > 0 {
                let mut used_w = 0u32;
                let mut fit = 0u32;
                while fit < len_guess {
                    let ch = memory.read_u8(chars_p.saturating_add(fit)).unwrap_or(0);
                    let w = current_char_width(runtime, ch);
                    if used_w.saturating_add(w) > fit_width {
                        break;
                    }
                    used_w = used_w.saturating_add(w);
                    fit = fit.saturating_add(1);
                }
                fit_chars = fit;
            }
            cpu.d[0] = fit_chars;
        }
        0xA200 => {
            // Return a stable synthetic display window handle.
            cpu.a[0] = 0x4000_0000;
        }
        0xA456 => {
            // WinGetBounds(winH, RectangleType* rP)
            let sp = cpu.a[7];
            let rect_p = [
                cpu.a[1],
                cpu.d[1],
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0),
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            if rect_p != 0 {
                // RectangleType: topLeft(x,y), extent(x,y) in Int16.
                memory.write_u16_be(rect_p, 0);
                memory.write_u16_be(rect_p.saturating_add(2), 0);
                memory.write_u16_be(rect_p.saturating_add(4), 160);
                memory.write_u16_be(rect_p.saturating_add(6), 160);
            }
            cpu.d[0] = 0;
        }
        0xA226 => {
            // WinDrawBitmap(bitmapP, x, y): collect draw calls for preview.
            let sp = cpu.a[7];
            let bitmap_p = [
                memory.read_u32_be(sp).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                cpu.a[0],
                cpu.d[0],
            ]
            .into_iter()
            .find(|p| *p != 0)
            .unwrap_or(0);
            if let Some((kind, res_id)) = find_resource_for_ptr(runtime, bitmap_p) {
                let tbmp = u32::from_be_bytes(*b"Tbmp");
                let taib = u32::from_be_bytes(*b"tAIB");
                if kind == tbmp || kind == taib {
                    let x = memory
                        .read_u16_be(sp.saturating_add(4))
                        .map(|v| v as i16)
                        .unwrap_or((cpu.d[0] & 0xFFFF) as u16 as i16);
                    let y = memory
                        .read_u16_be(sp.saturating_add(6))
                        .map(|v| v as i16)
                        .unwrap_or((cpu.d[1] & 0xFFFF) as u16 as i16);
                    if runtime.drawn_bitmaps.len() < 64 {
                        runtime.drawn_bitmaps.push(crate::prc_app::runtime::RuntimeBitmapDraw {
                            resource_id: res_id,
                            x,
                            y,
                        });
                    }
                }
            }
            cpu.d[0] = 0;
        }
        0xA183 => {
            cpu.a[0] = 0x3001_0000;
        }
        0xA153 => {
            cpu.a[0] = 0x3002_0000;
        }
        0xA158 | 0xA135 | 0xA195 | 0xA1A1 | 0xA234 | 0xA9F0 => {
            cpu.d[0] = 0;
        }
        _ => {
            // Default exploratory stub: keep execution moving and record what needs real impls.
            cpu.d[0] = 0;
        }
    }
}
