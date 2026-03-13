extern crate alloc;

use alloc::{vec, vec::Vec};

use crate::palm::{
    cpu::{core::CpuState68k, memory::MemoryMap},
    runtime::{
        EVT_CTL_SELECT, EVT_FLD_CHANGED, EVT_FLD_ENTER, EVT_FRM_LOAD, EVT_FRM_OPEN, EVT_KEY_DOWN,
        EVT_NIL, EVT_PEN_DOWN, FeatureEntry, MemBlock, PrcRuntimeContext, RuntimeEvent,
    },
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
        0xA178 | // sysTrapFrmGetFocus (stub for probing)
        0xA179 | // sysTrapFrmSetFocus (stub for probing)
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
    let trap_meta = crate::palm::traps::table::lookup(trap_word);
    let sp = cpu.a[7];
    let s0 = memory.read_u16_be(sp).unwrap_or(0);
    let s1 = memory.read_u16_be(sp.saturating_add(2)).unwrap_or(0);
    let s2 = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0);
    let s3 = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0);
    if runtime.trace_traps && runtime.trace_trap_budget > 0 {
        log::info!(
            "PRC trap call pc=0x{:X} trap=0x{:04X} group={} name={} d0=0x{:08X} d1=0x{:08X} d3=0x{:08X} a0=0x{:08X} a1=0x{:08X} a6=0x{:08X} a7=0x{:08X} sp[0..8]={:04X} {:04X} {:04X} {:04X}",
            pc,
            trap_word,
            trap_meta.group.as_str(),
            trap_meta.name,
            cpu.d[0],
            cpu.d[1],
            cpu.d[3],
            cpu.a[0],
            cpu.a[1],
            cpu.a[6],
            cpu.a[7],
            s0,
            s1,
            s2,
            s3
        );
        runtime.trace_trap_budget = runtime.trace_trap_budget.saturating_sub(1);
    }

    if crate::palm::traps::dm::DmApi::handle_trap(cpu, runtime, memory, trap_word) {
        return;
    }
    if crate::palm::traps::tbl::TblApi::handle_trap(cpu, runtime, memory, trap_word) {
        return;
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
        // Palm ABI passes MemHandle as the first stack argument.
        // Some glue stubs mirror it in A0/D0, so allow those as fallback only.
        if let Some(stack_raw) = memory.read_u32_be(sp) {
            if let Some(handle) = resolve_handle(runtime, stack_raw) {
                return handle;
            }
            // If an explicit stack argument was present, trust it.
            // This avoids treating null handles as stale A0/D0 values.
            return 0;
        }
        for raw in [cpu.a[0], cpu.d[0]] {
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
        let size = data.len().clamp(16, 1_048_576) as u32;
        // Keep Palm heap-ish allocations inside a stable synthetic window.
        if runtime.next_ptr < 0x2000_0000 || runtime.next_ptr > 0x2FFF_0000 {
            let mut high = 0x2000_0000u32;
            for b in &runtime.mem_blocks {
                if (0x2000_0000..=0x2FFF_FFFF).contains(&b.ptr) {
                    high = high.max(b.ptr.saturating_add(b.size).saturating_add(16));
                }
            }
            runtime.next_ptr = high.max(0x2000_0000);
        }
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

    fn get_font(runtime: &PrcRuntimeContext, font_id: u16) -> Option<&crate::palm::runtime::PalmFont> {
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
                return f.widths.get(idx).unwrap_or(f.avg_width).max(1) as u32;
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
        let idx = runtime.mem_blocks.iter().position(|b| b.handle == handle)?;
        if runtime.mem_blocks[idx].ptr == u32::MAX || runtime.mem_blocks[idx].ptr < 0x1000 {
            let mut high = runtime.next_ptr.max(0x2000_0000);
            for (i, b) in runtime.mem_blocks.iter().enumerate() {
                if i != idx && (0x2000_0000..=0x2FFF_FFFF).contains(&b.ptr) {
                    high = high.max(b.ptr.saturating_add(b.size).saturating_add(16));
                }
            }
            runtime.mem_blocks[idx].ptr = high;
            runtime.next_ptr = high.saturating_add(runtime.mem_blocks[idx].size.saturating_add(16));
        }
        runtime.mem_blocks[idx].locked = true;
        let ptr = runtime.mem_blocks[idx].ptr;
        let data = runtime.mem_blocks[idx].data.clone();
        memory.upsert_overlay(ptr, data);
        Some(ptr)
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

    fn decode_field_ptr(runtime: &PrcRuntimeContext, cpu: &CpuState68k, memory: &MemoryMap) -> u32 {
        let sp = cpu.a[7];
        [
            memory.read_u32_be(sp).unwrap_or(0),
            memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
            cpu.a[0],
            cpu.d[0],
        ]
        .into_iter()
        .find(|p| {
            runtime.form_objects.iter().any(|o| {
                o.ptr == *p && o.kind == crate::palm::runtime::RuntimeFormObjectKind::Field
            })
        })
        .unwrap_or(0)
    }

    fn find_field_obj_index(runtime: &PrcRuntimeContext, fld_p: u32) -> Option<usize> {
        runtime.form_objects.iter().position(|o| {
            o.ptr == fld_p && o.kind == crate::palm::runtime::RuntimeFormObjectKind::Field
        })
    }

    fn read_field_text(runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap, obj_idx: usize) -> Vec<u8> {
        let handle = runtime
            .form_objects
            .get(obj_idx)
            .map(|o| o.text_handle)
            .unwrap_or(0);
        if handle == 0 {
            return Vec::new();
        }
        let Some(ptr) = lock_handle(runtime, memory, handle) else {
            return Vec::new();
        };
        read_c_string(memory, ptr)
    }

    fn ensure_field_text_capacity(
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        obj_idx: usize,
        needed_with_nul: usize,
    ) -> Option<u32> {
        let needed = needed_with_nul.max(1);
        let current_handle = runtime
            .form_objects
            .get(obj_idx)
            .map(|o| o.text_handle)
            .unwrap_or(0);
        if current_handle != 0 {
            if let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.handle == current_handle) {
                if block.data.len() < needed {
                    block.data.resize(needed.max(16), 0);
                    block.size = block.data.len() as u32;
                    memory.upsert_overlay(block.ptr, block.data.clone());
                }
                return Some(current_handle);
            }
        }
        let (new_h, _new_ptr) = alloc_mem(runtime, memory, vec![0u8; needed.max(16)], None, None);
        if let Some(obj) = runtime.form_objects.get_mut(obj_idx) {
            obj.text_handle = new_h;
        }
        Some(new_h)
    }

    fn sync_field_draw_from_obj(runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap, obj_idx: usize) {
        let Some(obj) = runtime.form_objects.get(obj_idx).cloned() else {
            return;
        };
        let text = if obj.text_handle == 0 {
            alloc::string::String::new()
        } else if let Some(ptr) = lock_handle(runtime, memory, obj.text_handle) {
            let bytes = read_c_string(memory, ptr);
            alloc::string::String::from_utf8_lossy(&bytes).into_owned()
        } else {
            alloc::string::String::new()
        };
        if let Some(existing) = runtime
            .field_draws
            .iter_mut()
            .find(|f| f.form_id == obj.form_id && f.field_id == obj.object_id)
        {
            existing.text = text;
        } else {
            runtime.field_draws.push(crate::palm::runtime::RuntimeFieldDraw {
                form_id: obj.form_id,
                field_id: obj.object_id,
                text,
            });
        }
    }

    fn set_field_text(
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        obj_idx: usize,
        text: &[u8],
    ) -> bool {
        let Some(handle) = ensure_field_text_capacity(runtime, memory, obj_idx, text.len().saturating_add(1)) else {
            return false;
        };
        let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.handle == handle) else {
            return false;
        };
        let needed = text.len().saturating_add(1);
        if block.data.len() < needed {
            block.data.resize(needed.max(16), 0);
            block.size = block.data.len() as u32;
        }
        let n = text.len();
        if n > 0 {
            block.data[..n].copy_from_slice(text);
        }
        block.data[n] = 0;
        if block.data.len() > n + 1 {
            block.data[n + 1..].fill(0);
        }
        memory.upsert_overlay(block.ptr, block.data.clone());
        true
    }

    fn field_apply_replace(
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        obj_idx: usize,
        mut start: usize,
        mut end: usize,
        insert: &[u8],
    ) -> bool {
        let old_text = read_field_text(runtime, memory, obj_idx);
        let old_len = old_text.len();
        start = start.min(old_len);
        end = end.min(old_len);
        if end < start {
            core::mem::swap(&mut start, &mut end);
        }
        let mut new_text = Vec::with_capacity(old_len.saturating_sub(end.saturating_sub(start)).saturating_add(insert.len()));
        new_text.extend_from_slice(&old_text[..start]);
        new_text.extend_from_slice(insert);
        new_text.extend_from_slice(&old_text[end..]);
        let ok = set_field_text(runtime, memory, obj_idx, &new_text);
        if ok {
            let new_pos = start.saturating_add(insert.len()) as u16;
            if let Some(obj) = runtime.form_objects.get_mut(obj_idx) {
                obj.sel_start = new_pos;
                obj.sel_end = new_pos;
                obj.ins_pt = new_pos;
                obj.dirty = true;
            }
            sync_field_draw_from_obj(runtime, memory, obj_idx);
        }
        ok
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

    fn decode_form_id_from_handle_or_active(runtime: &PrcRuntimeContext, form_h: u32) -> Option<u16> {
        if (form_h & 0xFFFF_0000) == 0x3000_0000 {
            Some((form_h & 0xFFFF) as u16)
        } else {
            runtime.active_form_id
        }
    }

    fn set_focused_field_by_id(
        runtime: &mut PrcRuntimeContext,
        form_id: u16,
        field_id: u16,
    ) -> bool {
        if let Some(obj) = runtime.form_objects.iter().find(|o| {
            o.form_id == form_id
                && o.object_id == field_id
                && o.kind == crate::palm::runtime::RuntimeFormObjectKind::Field
        }) {
            runtime.focused_field_index = Some(obj.object_index);
            return true;
        }
        false
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

    fn normalize_tstr_payload(data: &[u8]) -> Vec<u8> {
        // Palm string resources may be stored as C strings or length-prefixed.
        // Convert known length-prefixed encodings into a C-string payload so
        // StrLen/StrCopy callers behave consistently.
        if data.is_empty() {
            return vec![0];
        }
        if data.contains(&0) {
            return data.to_vec();
        }
        let len8 = data[0] as usize;
        if len8 > 0 && len8 + 1 <= data.len() {
            let mut out = data[1..1 + len8].to_vec();
            out.push(0);
            return out;
        }
        if data.len() >= 2 {
            let len16 = u16::from_be_bytes([data[0], data[1]]) as usize;
            if len16 > 0 && len16 + 2 <= data.len() {
                let mut out = data[2..2 + len16].to_vec();
                out.push(0);
                return out;
            }
        }
        let mut out = data.to_vec();
        out.push(0);
        out
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
        if e_type == EVT_CTL_SELECT {
            // ctlSelectEvent payload starts at +8 in EventType union:
            // controlID (u16), pControl (u32), on (u8), reserved (u8), value (u16).
            // Keep pControl synthetic but stable; many apps only read controlID/on.
            let _ = memory.write_u32_be(event_p.saturating_add(10), 0x3001_0000u32);
            let _ = memory.write_u8(event_p.saturating_add(14), 1);
            let _ = memory.write_u8(event_p.saturating_add(15), 0);
            let _ = memory.write_u16_be(event_p.saturating_add(16), 0);
        } else if e_type == EVT_KEY_DOWN {
            // keyDownEvent payload:
            // chr (WChar), keyCode (UInt16), modifiers (UInt16).
            let _ = memory.write_u16_be(event_p.saturating_add(8), data_u16);
            let _ = memory.write_u16_be(event_p.saturating_add(10), data_u16);
            let _ = memory.write_u16_be(event_p.saturating_add(12), 0);
        } else {
            // Keep a minimal generic payload consistent with eType to help glue code
            // that aliases the union through generic fields.
            let _ = memory.write_u16_be(event_p.saturating_add(10), e_type);
        }
    }

    fn decode_ptr_arg_from_stack(
        cpu: &CpuState68k,
        memory: &MemoryMap,
        arg_offset: u32,
    ) -> u32 {
        let sp = cpu.a[7];
        let raw_full = memory.read_u32_be(sp.saturating_add(arg_offset)).unwrap_or(0);
        if raw_full != 0 && memory.contains_addr(raw_full) {
            return raw_full;
        }
        let raw = raw_full & 0x00FF_FFFF;
        if raw != 0 && memory.contains_addr(raw) {
            return raw;
        }
        // Palm 68K glue frequently passes frame-relative locals as 16-bit offsets
        // (signed or unsigned representation depending on codegen).
        if raw_full <= 0xFFFF || (raw_full & 0xFFFF_0000) == 0xFFFF_0000 {
            let off_signed = (raw_full as u16) as i16 as i32;
            let signed_candidate = if off_signed >= 0 {
                cpu.a[6].wrapping_add(off_signed as u32)
            } else {
                cpu.a[6].wrapping_sub((-off_signed) as u32)
            };
            if signed_candidate != 0 && memory.contains_addr(signed_candidate) {
                return signed_candidate;
            }
            let off_unsigned = (raw_full as u16) as u32;
            let unsigned_candidate = cpu.a[6].wrapping_add(off_unsigned);
            if unsigned_candidate != 0 && memory.contains_addr(unsigned_candidate) {
                return unsigned_candidate;
            }
        }
        raw_full
    }

    fn type_to_name(db_type: u32) -> alloc::string::String {
        let b = db_type.to_be_bytes();
        if b.iter().all(|c| (0x20..=0x7E).contains(c)) {
            alloc::string::String::from_utf8_lossy(&b).into_owned()
        } else {
            alloc::format!("DB{:08X}", db_type)
        }
    }

    fn write_c_string(memory: &mut MemoryMap, dst: u32, s: &str) {
        if dst == 0 || !memory.contains_addr(dst) {
            return;
        }
        for (i, ch) in s.as_bytes().iter().enumerate() {
            let _ = memory.write_u8(dst.saturating_add(i as u32), *ch);
        }
        let _ = memory.write_u8(dst.saturating_add(s.len() as u32), 0);
    }

    fn trap_arg_u8(memory: &MemoryMap, sp: u32, idx: &mut u32) -> u8 {
        let v = memory.read_u8(sp.saturating_add(*idx)).unwrap_or(0);
        *idx = idx.saturating_add(2);
        v
    }

    fn trap_arg_u16(memory: &MemoryMap, sp: u32, idx: &mut u32) -> u16 {
        let v = memory.read_u16_be(sp.saturating_add(*idx)).unwrap_or(0);
        *idx = idx.saturating_add(2);
        v
    }

    fn trap_arg_u32(memory: &MemoryMap, sp: u32, idx: &mut u32) -> u32 {
        let v = memory.read_u32_be(sp.saturating_add(*idx)).unwrap_or(0);
        *idx = idx.saturating_add(4);
        v
    }

    fn resolve_out_ptr(cpu: &CpuState68k, memory: &MemoryMap, stack_ptr: u32) -> u32 {
        if stack_ptr != 0 && memory.contains_addr(stack_ptr) {
            return stack_ptr;
        }
        for p in [cpu.a[0], cpu.a[1], cpu.d[0], cpu.d[1]] {
            if p != 0 && memory.contains_addr(p) {
                return p;
            }
        }
        0
    }

    fn ensure_db_for_type_creator(
        runtime: &mut PrcRuntimeContext,
        db_type: u32,
        creator: u32,
    ) -> u32 {
        if let Some(existing) = runtime
            .databases
            .iter()
            .find(|db| db.db_type == db_type && db.creator == creator)
            .cloned()
        {
            return existing.local_id;
        }
        let local_id = runtime.next_local_id;
        runtime.next_local_id = runtime.next_local_id.saturating_add(1);
        runtime.databases.push(crate::palm::runtime::RuntimeDatabase {
            local_id,
            card_no: 0,
            name: type_to_name(db_type),
            creator,
            db_type,
            is_resource_db: false,
            version: 1,
            attributes: 0,
            mod_number: 0,
            app_info_id: 0,
            sort_info_id: 0,
            record_handles: alloc::vec::Vec::new(),
        });
        local_id
    }

    fn open_db_local_id(runtime: &mut PrcRuntimeContext, local_id: u32) -> u32 {
        if let Some(existing) = runtime
            .open_databases
            .iter()
            .find(|o| o.local_id == local_id)
            .cloned()
        {
            return existing.db_ref;
        }
        let db_ref = runtime.next_db_ref;
        runtime.next_db_ref = runtime.next_db_ref.saturating_add(1);
        runtime.open_databases.push(crate::palm::runtime::RuntimeOpenDatabase {
            db_ref,
            local_id,
            mode: 0,
        });
        db_ref
    }

    fn queue_form_transition(runtime: &mut PrcRuntimeContext, form_id: u16) {
        if form_id == 0 {
            return;
        }
        runtime.event_queue.push(RuntimeEvent {
            e_type: EVT_FRM_LOAD,
            data_u16: form_id,
        });
        runtime.event_queue.push(RuntimeEvent {
            e_type: EVT_FRM_OPEN,
            data_u16: form_id,
        });
        runtime.startup_open_dispatched = false;
    }

    match trap_word {
        0xA08F => {
            runtime.shutting_down = false;
            runtime.active_form_id = None;
            runtime.active_form_handle = 0x3000_0000;
            runtime.active_form_handler = 0;
            runtime.focused_field_index = None;
            runtime.form_return_stack.clear();
            runtime.event_queue.clear();
            runtime.pending_dispatch_event = None;
            runtime.startup_open_dispatched = false;
            runtime.evt_polls = 0;
            runtime.blink_next_tick = 175;
            runtime.blink_phase = 0;
            runtime.field_draws.clear();
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
            queue_form_transition(runtime, form_id);
            cpu.d[0] = 0;
        }
        0xA19C => {
            // FrmPopupForm(formID): modal-style transition.
            // Maintain a lightweight return stack so FrmReturnToForm(0) works.
            let sp = cpu.a[7];
            let form_id = memory.read_u16_be(sp).unwrap_or(0);
            if form_id != 0 {
                if let Some(current) = runtime.active_form_id {
                    if current != form_id {
                        runtime.form_return_stack.push(current);
                    }
                }
                queue_form_transition(runtime, form_id);
            }
            cpu.d[0] = 0;
        }
        0xA19E => {
            // FrmReturnToForm(formID): return to requested form or previous popup parent.
            let sp = cpu.a[7];
            let requested = memory.read_u16_be(sp).unwrap_or(0);
            let target = if requested != 0 {
                // Drop any stale nested entries at/above requested target.
                if let Some(pos) = runtime
                    .form_return_stack
                    .iter()
                    .rposition(|id| *id == requested)
                {
                    runtime.form_return_stack.truncate(pos);
                }
                Some(requested)
            } else {
                runtime.form_return_stack.pop()
            };
            if let Some(form_id) = target {
                queue_form_transition(runtime, form_id);
            }
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
                let form_id = (form_h & 0xFFFF) as u16;
                runtime.active_form_id = Some(form_id);
                runtime.focused_field_index = None;
                runtime.startup_open_dispatched = false;
                let has_form_transition = runtime
                    .event_queue
                    .iter()
                    .any(|e| e.e_type == EVT_FRM_LOAD || e.e_type == EVT_FRM_OPEN);
                if !has_form_transition && form_id != 0 {
                    runtime.event_queue.push(RuntimeEvent {
                        e_type: EVT_FRM_LOAD,
                        data_u16: form_id,
                    });
                    runtime.event_queue.push(RuntimeEvent {
                        e_type: EVT_FRM_OPEN,
                        data_u16: form_id,
                    });
                }
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
            runtime.focused_field_index = None;
            runtime.startup_open_dispatched = false;
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
            runtime.startup_open_dispatched = true;
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
        0xA17E => {
            // FormType* FrmGetFormPtr(UInt16 formID)
            let sp = cpu.a[7];
            let form_id = [
                memory.read_u16_be(sp).unwrap_or(0),
                (cpu.d[0] & 0xFFFF) as u16,
                (cpu.a[0] & 0xFFFF) as u16,
            ]
            .into_iter()
            .find(|v| *v != 0)
            .unwrap_or(0);
            let has_form_resource = runtime
                .resources
                .iter()
                .any(|res| res.kind == u32::from_be_bytes(*b"tFRM") && res.id == form_id);
            let form_h = if has_form_resource {
                0x3000_0000u32 | (form_id as u32)
            } else {
                0
            };
            cpu.a[0] = form_h;
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | (form_h & 0xFFFF);
        }
        0xA17F => {
            // UInt16 FrmGetNumberOfObjects(const FormType *formP)
            let sp = cpu.a[7];
            let form_h = [
                memory.read_u32_be(sp).unwrap_or(0),
                cpu.a[0],
                cpu.d[0],
                runtime.active_form_handle,
            ]
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
            .unwrap_or(runtime.active_form_handle);
            let count = decode_form_id_from_handle_or_active(runtime, form_h)
                .map(|fid| {
                    runtime
                        .form_objects
                        .iter()
                        .filter(|o| o.form_id == fid)
                        .count() as u16
                })
                .unwrap_or(0);
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | count as u32;
        }
        0xA178 => {
            // UInt16 FrmGetFocus(const FormType *formP)
            let sp = cpu.a[7];
            let form_h = [
                memory.read_u32_be(sp).unwrap_or(0),
                cpu.a[0],
                runtime.active_form_handle,
            ]
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
            .unwrap_or(runtime.active_form_handle);
            let ret = if decode_form_id_from_handle_or_active(runtime, form_h) == runtime.active_form_id
            {
                runtime.focused_field_index.unwrap_or(0xFFFF)
            } else {
                0xFFFF
            };
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | ret as u32;
        }
        0xA179 => {
            // void FrmSetFocus(FormType *formP, UInt16 fieldIndex)
            let sp = cpu.a[7];
            let form_h = [
                memory.read_u32_be(sp).unwrap_or(0),
                cpu.a[0],
                runtime.active_form_handle,
            ]
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
            .unwrap_or(runtime.active_form_handle);
            let field_index = memory
                .read_u16_be(sp.saturating_add(4))
                .unwrap_or((cpu.d[1] & 0xFFFF) as u16);
            if field_index == 0xFFFF {
                runtime.focused_field_index = None;
            } else if let Some(form_id) = decode_form_id_from_handle_or_active(runtime, form_h) {
                let is_field = runtime.form_objects.iter().any(|o| {
                    o.form_id == form_id
                        && o.object_index == field_index
                        && o.kind == crate::palm::runtime::RuntimeFormObjectKind::Field
                });
                runtime.focused_field_index = if is_field {
                    Some(field_index)
                } else {
                    None
                };
            }
            cpu.d[0] = 0;
        }
        0xA18B => {
            // void FrmSetControlGroupSelection(formP, groupNum, controlID)
            // For now we accept and no-op; this unblocks apps that set up
            // radio/option groups during form-open.
            cpu.d[0] = 0;
        }
        0xA18C => {
            // UInt16 FrmGetControlGroupSelection(formP, groupNum)
            // Unknown selection in current lightweight form model.
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | 0xFFFF;
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
            let mut evt_type = memory.read_u16_be(event_p).unwrap_or(0xFFFF);
            // If a control-select is pending but the current event buffer still
            // carries nil, upgrade it in-place before dispatch so form handlers
            // observe button activation deterministically.
            if evt_type == EVT_NIL {
                if let Some(evt) = runtime.pending_dispatch_event.take() {
                    write_event(memory, event_p, evt.e_type, evt.data_u16);
                    evt_type = evt.e_type;
                    log::info!(
                        "PRC trap detail FrmDispatchEvent promoted pending eType={} data=0x{:04X} eventP=0x{:08X}",
                        evt.e_type,
                        evt.data_u16,
                        event_p
                    );
                    if let Some(i) = runtime
                        .event_queue
                        .iter()
                        .position(|e| e.e_type == evt.e_type && e.data_u16 == evt.data_u16)
                    {
                        let _ = runtime.event_queue.remove(i);
                    }
                } else if let Some(i) = runtime
                    .event_queue
                    .iter()
                    .position(|e| e.e_type == EVT_CTL_SELECT)
                {
                    let evt = runtime.event_queue.remove(i);
                    write_event(memory, event_p, evt.e_type, evt.data_u16);
                    evt_type = evt.e_type;
                    log::info!(
                        "PRC trap detail FrmDispatchEvent promoted queued eType={} data=0x{:04X} eventP=0x{:08X}",
                        evt.e_type,
                        evt.data_u16,
                        event_p
                    );
                }
            }
            if evt_type == EVT_FRM_OPEN {
                // Even when an app installs a custom form handler, Palm's
                // default behavior is an opened/visible form. Mark it drawn so
                // our preview/UI layer can present the active form immediately.
                runtime.drawn_form_id = runtime.active_form_id;
            }
            if runtime.active_form_handler != 0 {
                let ret_pc = cpu.pc;
                cpu.a[7] = cpu.a[7].wrapping_sub(4);
                let _ = memory.write_u32_be(cpu.a[7], ret_pc);
                // Keep CPU return tracking in sync with the synthetic call we
                // just emitted on the emulated stack.
                cpu.call_stack.push(ret_pc);
                cpu.pc = runtime.active_form_handler;
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail FrmDispatchEvent call handler=0x{:08X} eventP=0x{:08X} eType={}",
                        runtime.active_form_handler,
                        event_p,
                        evt_type
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
                if runtime.pending_dispatch_event == Some(evt) {
                    runtime.pending_dispatch_event = None;
                }
                if evt.e_type == EVT_FRM_OPEN {
                    runtime.startup_open_dispatched = true;
                    runtime.focused_field_index = None;
                } else if evt.e_type == EVT_FRM_LOAD {
                    runtime.focused_field_index = None;
                } else if evt.e_type == EVT_FLD_ENTER {
                    if let Some(form_id) = runtime.active_form_id {
                        let _ = set_focused_field_by_id(runtime, form_id, evt.data_u16);
                    }
                }
                write_event(memory, event_p, evt.e_type, evt.data_u16);
                if evt.e_type != EVT_NIL {
                    log::info!(
                        "PRC trap detail EvtGetEvent queued eType={} data=0x{:04X} eventP=0x{:08X}",
                        evt.e_type,
                        evt.data_u16,
                        event_p
                    );
                }
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
            if !runtime.startup_open_dispatched {
                if let Some(form_id) = runtime.active_form_id {
                    let evt = RuntimeEvent {
                        e_type: EVT_FRM_OPEN,
                        data_u16: form_id,
                    };
                    runtime.startup_open_dispatched = true;
                    runtime.focused_field_index = None;
                    write_event(memory, event_p, evt.e_type, evt.data_u16);
                    log::info!(
                        "PRC trap detail EvtGetEvent synth eType={} data=0x{:04X} eventP=0x{:08X}",
                        evt.e_type,
                        evt.data_u16,
                        event_p
                    );
                    runtime.blocked_on_evt_get_event = false;
                    runtime.blocked_evt_timeout_ticks = 0;
                    cpu.d[0] = 0;
                    return;
                }
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
            let sp = cpu.a[7];
            let stack_size = memory.read_u32_be(sp).unwrap_or(0);
            let size = [stack_size, cpu.a[0], cpu.d[0], cpu.a[1], cpu.d[1]]
                .into_iter()
                .find(|v| *v > 0 && *v <= 1_048_576)
                .unwrap_or(16);
            let data = vec![0u8; size as usize];
            let (handle, _ptr) = alloc_mem(runtime, memory, data, None, None);
            cpu.a[0] = handle;
            cpu.d[0] = handle;
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
        0xA02D => {
            // UInt32 MemHandleSize(MemHandle h)
            let handle = decode_handle_arg(runtime, cpu, memory);
            let size = runtime
                .mem_blocks
                .iter()
                .find(|b| b.handle == handle)
                .map(|b| b.size)
                .unwrap_or(0);
            cpu.d[0] = size;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail MemHandleSize handle=0x{:08X} -> {}",
                    handle, size
                );
            }
        }
        0xA02B => {
            let handle = decode_handle_arg(runtime, cpu, memory);
            cpu.d[0] = if free_handle(runtime, memory, handle) { 0 } else { 1 };
        }
        0xA013 => {
            let sp = cpu.a[7];
            let stack_size = memory.read_u32_be(sp).unwrap_or(0);
            let size = [stack_size, cpu.a[0], cpu.d[0], cpu.a[1], cpu.d[1]]
                .into_iter()
                .find(|v| *v > 0 && *v <= 1_048_576)
                .unwrap_or(16);
            let data = vec![0u8; size as usize];
            let (_handle, ptr) = alloc_mem(runtime, memory, data, None, None);
            cpu.a[0] = ptr;
            cpu.d[0] = ptr;
        }
        0xA012 => {
            // MemChunkFree(ptr): best-effort free by pointer.
            let sp = cpu.a[7];
            let ptr = [memory.read_u32_be(sp).unwrap_or(0), cpu.a[0], cpu.d[0]]
                .into_iter()
                .find(|p| *p != 0)
                .unwrap_or(0);
            if let Some(pos) = runtime.mem_blocks.iter().position(|b| b.ptr == ptr) {
                let block = runtime.mem_blocks.swap_remove(pos);
                memory.remove_overlay(block.ptr);
                cpu.d[0] = 0;
            } else {
                // Tolerate unknown/free-twice patterns.
                cpu.d[0] = 0;
            }
        }
        0xA035 => {
            // MemPtrUnlock(ptr): no-op in this model.
            cpu.d[0] = 0;
        }
        0xA026 => {
            // MemMove(dstP, srcP, numBytes)
            let sp = cpu.a[7];
            let s0 = memory.read_u32_be(sp).unwrap_or(0);
            let s1 = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
            let s2 = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
            let looks_ptr = |p: u32, runtime: &PrcRuntimeContext| {
                runtime.mem_blocks.iter().any(|b| p >= b.ptr && p < b.ptr.saturating_add(b.size))
                    || (p & 0xF000_0000) == 0x2000_0000
            };
            let dst = if looks_ptr(s0, runtime) { s0 } else { cpu.a[0] };
            let src = if looks_ptr(s1, runtime) { s1 } else { cpu.a[1] };
            let count = [s2, cpu.d[0], cpu.d[1], cpu.d[2]]
                .into_iter()
                .find(|v| *v > 0 && *v < 0x0010_0000)
                .unwrap_or(0) as usize;

            if count == 0 {
                cpu.d[0] = 0;
                cpu.a[0] = dst;
            } else {
                // Be permissive: Palm code often calls MemMove on pointers that are
                // valid in the app's view but may partially miss our overlays.
                // Copy readable bytes and write where possible, defaulting missing
                // source bytes to zero. Report success to keep control flow moving.
                let mut tmp = alloc::vec::Vec::with_capacity(count);
                for i in 0..count {
                    let b = memory.read_u8(src.saturating_add(i as u32)).unwrap_or(0);
                    tmp.push(b);
                }
                for (i, b) in tmp.iter().enumerate() {
                    let _ = memory.write_u8(dst.saturating_add(i as u32), *b);
                }
                cpu.d[0] = 0;
                cpu.a[0] = dst;
            }
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
            if let Some((kind, id, mut data)) = select_resource_data(runtime, kind_hint, id_hint) {
                let tstr = u32::from_be_bytes(*b"tSTR");
                if kind == tstr {
                    data = normalize_tstr_payload(&data);
                }
                maybe_log_resource_pick(runtime, kind_hint, id_hint, kind, id, data.len());
                let handle = if let Some(existing) = runtime
                    .mem_blocks
                    .iter_mut()
                    .find(|b| b.resource_kind == Some(kind) && b.resource_id == Some(id))
                {
                    if existing.data != data {
                        existing.data = data.clone();
                        existing.size = existing.data.len().max(16) as u32;
                        memory.upsert_overlay(existing.ptr, existing.data.clone());
                    }
                    existing.handle
                } else {
                    let (h, _ptr) = alloc_mem(runtime, memory, data, Some(kind), Some(id));
                    h
                };
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
        0xA046 => {
            // DmDatabaseInfo(cardNo, localID, ...out pointers...)
            let sp = cpu.a[7];
            let local_id = [
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                cpu.d[0],
                cpu.a[0],
            ]
            .into_iter()
            .find(|v| *v >= 0x1000)
            .unwrap_or(0);
            let db = runtime
                .databases
                .iter()
                .find(|db| db.local_id == local_id)
                .cloned()
                .unwrap_or(crate::palm::runtime::RuntimeDatabase {
                    local_id,
                    card_no: 0,
                    name: alloc::string::String::new(),
                    creator: 0,
                    db_type: 0,
                    is_resource_db: false,
                    version: 0,
                    attributes: 0,
                    mod_number: 0,
                    app_info_id: 0,
                    sort_info_id: 0,
                    record_handles: alloc::vec::Vec::new(),
                });

            // Signature:
            // cardNo(0), localID(2), nameP(6), attributesP(10), versionP(14),
            // crDateP(18), modDateP(22), bckUpDateP(26), modNumP(30),
            // appInfoIDP(34), sortInfoIDP(38), typeP(42), creatorP(46)
            let name_p = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
            let attrs_p = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
            let vers_p = memory.read_u32_be(sp.saturating_add(14)).unwrap_or(0);
            let cr_p = memory.read_u32_be(sp.saturating_add(18)).unwrap_or(0);
            let mod_p = memory.read_u32_be(sp.saturating_add(22)).unwrap_or(0);
            let bkp_p = memory.read_u32_be(sp.saturating_add(26)).unwrap_or(0);
            let modn_p = memory.read_u32_be(sp.saturating_add(30)).unwrap_or(0);
            let appi_p = memory.read_u32_be(sp.saturating_add(34)).unwrap_or(0);
            let sorti_p = memory.read_u32_be(sp.saturating_add(38)).unwrap_or(0);
            let type_p = memory.read_u32_be(sp.saturating_add(42)).unwrap_or(0);
            let creator_p = memory.read_u32_be(sp.saturating_add(46)).unwrap_or(0);

            write_c_string(memory, name_p, &db.name);
            if attrs_p != 0 && memory.contains_addr(attrs_p) {
                let _ = memory.write_u16_be(attrs_p, if db.is_resource_db { 1 } else { 0 });
            }
            if vers_p != 0 && memory.contains_addr(vers_p) {
                let _ = memory.write_u16_be(vers_p, 1);
            }
            if cr_p != 0 && memory.contains_addr(cr_p) {
                let _ = memory.write_u32_be(cr_p, 0);
            }
            if mod_p != 0 && memory.contains_addr(mod_p) {
                let _ = memory.write_u32_be(mod_p, 0);
            }
            if bkp_p != 0 && memory.contains_addr(bkp_p) {
                let _ = memory.write_u32_be(bkp_p, 0);
            }
            if modn_p != 0 && memory.contains_addr(modn_p) {
                let _ = memory.write_u32_be(modn_p, 0);
            }
            if appi_p != 0 && memory.contains_addr(appi_p) {
                let _ = memory.write_u32_be(appi_p, 0);
            }
            if sorti_p != 0 && memory.contains_addr(sorti_p) {
                let _ = memory.write_u32_be(sorti_p, 0);
            }
            if type_p != 0 && memory.contains_addr(type_p) {
                let _ = memory.write_u32_be(type_p, db.db_type);
            }
            if creator_p != 0 && memory.contains_addr(creator_p) {
                let _ = memory.write_u32_be(creator_p, db.creator);
            }
            cpu.d[0] = 0; // errNone
        }
        0xA049 => {
            // DmOpenDatabase(cardNo, localID, mode) -> DmOpenRef
            let sp = cpu.a[7];
            let local_id = [
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                cpu.d[0],
                cpu.a[0],
            ]
            .into_iter()
            .find(|v| *v >= 0x1000)
            .unwrap_or(0);
            if local_id == 0 || runtime.databases.iter().all(|db| db.local_id != local_id) {
                cpu.a[0] = 0;
                cpu.d[0] = 0x8000;
            } else {
                let db_ref = open_db_local_id(runtime, local_id);
                cpu.a[0] = db_ref;
                cpu.d[0] = 0;
            }
        }
        0xA04A => {
            // DmCloseDatabase(dbRef)
            let sp = cpu.a[7];
            let db_ref = [
                memory.read_u32_be(sp).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                cpu.a[0],
                cpu.d[0],
            ]
            .into_iter()
            .find(|v| (*v & 0xF000_0000) == 0x5000_0000)
            .unwrap_or(0);
            if let Some(i) = runtime.open_databases.iter().position(|o| o.db_ref == db_ref) {
                runtime.open_databases.remove(i);
            }
            cpu.d[0] = 0;
        }
        0xA04C => {
            // DmOpenDatabaseInfo(dbRef, ...) -> errNone
            let sp = cpu.a[7];
            let db_ref = [
                memory.read_u32_be(sp).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                cpu.a[0],
                cpu.d[0],
            ]
            .into_iter()
            .find(|v| (*v & 0xF000_0000) == 0x5000_0000)
            .unwrap_or(0);
            let local_id = runtime
                .open_databases
                .iter()
                .find(|o| o.db_ref == db_ref)
                .map(|o| o.local_id)
                .unwrap_or(0);
            // Signature: (dbRef, localIDP, openCountP, modeP, cardNoP, resDBP)
            let local_id_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
            let open_count_p = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
            let mode_p = memory.read_u32_be(sp.saturating_add(12)).unwrap_or(0);
            let card_no_p = memory.read_u32_be(sp.saturating_add(16)).unwrap_or(0);
            let res_db_p = memory.read_u32_be(sp.saturating_add(20)).unwrap_or(0);
            if local_id_p != 0 && memory.contains_addr(local_id_p) {
                let _ = memory.write_u32_be(local_id_p, local_id);
            }
            if open_count_p != 0 && memory.contains_addr(open_count_p) {
                let _ = memory.write_u16_be(open_count_p, 1);
            }
            if mode_p != 0 && memory.contains_addr(mode_p) {
                let _ = memory.write_u16_be(mode_p, 0);
            }
            if card_no_p != 0 && memory.contains_addr(card_no_p) {
                let _ = memory.write_u16_be(card_no_p, 0);
            }
            if res_db_p != 0 && memory.contains_addr(res_db_p) {
                let is_res = runtime
                    .databases
                    .iter()
                    .find(|db| db.local_id == local_id)
                    .map(|db| db.is_resource_db)
                    .unwrap_or(false);
                let _ = memory.write_u16_be(res_db_p, if is_res { 1 } else { 0 });
            }
            cpu.d[0] = 0;
        }
        0xA075 => {
            // DmOpenDatabaseByTypeCreator(type, creator, mode) -> DmOpenRef
            let sp = cpu.a[7];
            let db_type = [
                memory.read_u32_be(sp).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                cpu.d[0],
            ]
            .into_iter()
            .find(|v| *v != 0)
            .unwrap_or(0);
            let creator = [
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0),
                cpu.d[1],
            ]
            .into_iter()
            .find(|v| *v != 0)
            .unwrap_or(0);
            let local_id = ensure_db_for_type_creator(runtime, db_type, creator);
            let db_ref = open_db_local_id(runtime, local_id);
            cpu.a[0] = db_ref;
            cpu.d[0] = 0;
        }
        0xA0F5 => {
            // UInt32 TimGetSeconds()
            cpu.d[0] = runtime.ticks / 100;
        }
        0xA0F7 => {
            cpu.d[0] = runtime.ticks;
        }
        0xA0FC => {
            // void TimSecondsToDateTime(UInt32 seconds, DateTimeType *dtP)
            let sp = cpu.a[7];
            let mut seconds = memory.read_u32_be(sp).unwrap_or(cpu.d[0]);
            if seconds == 0 {
                seconds = cpu.d[0];
            }
            let dt_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(cpu.a[0]);
            if dt_p != 0 && memory.contains_addr(dt_p) {
                let sec = (seconds % 60) as u16;
                let min = ((seconds / 60) % 60) as u16;
                let hour = ((seconds / 3600) % 24) as u16;
                let day = (((seconds / 86400) % 28) + 1) as u16;
                let month = (((seconds / (86400 * 30)) % 12) + 1) as u16;
                let year = (2000 + ((seconds / (86400 * 365)) % 50)) as u16;
                let weekday = ((seconds / 86400) % 7) as u16;
                let _ = memory.write_u16_be(dt_p, sec);
                let _ = memory.write_u16_be(dt_p.saturating_add(2), min);
                let _ = memory.write_u16_be(dt_p.saturating_add(4), hour);
                let _ = memory.write_u16_be(dt_p.saturating_add(6), day);
                let _ = memory.write_u16_be(dt_p.saturating_add(8), month);
                let _ = memory.write_u16_be(dt_p.saturating_add(10), year);
                let _ = memory.write_u16_be(dt_p.saturating_add(12), weekday);
            }
            cpu.d[0] = 0;
        }
        0xA25F => {
            // UInt16 DayOfWeek(month, day, year), return 0=Sunday..6=Saturday.
            let sp = cpu.a[7];
            let mut month = memory.read_u16_be(sp).unwrap_or((cpu.d[0] & 0xFFFF) as u16);
            let mut day = memory
                .read_u16_be(sp.saturating_add(2))
                .unwrap_or((cpu.d[1] & 0xFFFF) as u16);
            let mut year = memory
                .read_u16_be(sp.saturating_add(4))
                .unwrap_or((cpu.d[2] & 0xFFFF) as u16);
            if month == 0 || month > 12 {
                month = 1;
            }
            if day == 0 || day > 31 {
                day = 1;
            }
            if year < 100 {
                year = year.saturating_add(1900);
            }
            let (y, m) = if month < 3 {
                (year as i32 - 1, month as i32 + 12)
            } else {
                (year as i32, month as i32)
            };
            let d = day as i32;
            // Zeller's congruence: h=0 Saturday..6 Friday.
            let h = (d + ((13 * (m + 1)) / 5) + (y % 100) + ((y % 100) / 4) + ((y / 100) / 4)
                + (5 * (y / 100)))
                % 7;
            // Convert to Palm convention 0=Sunday..6=Saturday.
            let dow = ((h + 6) % 7) as u16;
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | dow as u32;
        }
        0xA266 => {
            // DateToAscii(month, day, year, format, pString)
            let sp = cpu.a[7];
            let mut idx = 0u32;
            let mut month = trap_arg_u8(memory, sp, &mut idx) as u16;
            let mut day = trap_arg_u8(memory, sp, &mut idx) as u16;
            let mut year = trap_arg_u16(memory, sp, &mut idx);
            let date_format = trap_arg_u8(memory, sp, &mut idx);
            let dst = resolve_out_ptr(cpu, memory, trap_arg_u32(memory, sp, &mut idx));
            if month == 0 || month > 12 {
                month = 1;
            }
            if day == 0 || day > 31 {
                day = 1;
            }
            if year < 100 {
                year = year.saturating_add(1900);
            }
            if dst != 0 {
                let s = match date_format {
                    0 => alloc::format!("{:02}/{:02}/{:02}", month, day, year % 100), // MDY /
                    1 => alloc::format!("{:02}/{:02}/{:02}", day, month, year % 100), // DMY /
                    2 => alloc::format!("{:02}.{:02}.{:02}", day, month, year % 100), // DMY .
                    3 => alloc::format!("{:02}-{:02}-{:02}", day, month, year % 100), // DMY -
                    4 => alloc::format!("{:02}/{:02}/{:02}", year % 100, month, day), // YMD /
                    5 => alloc::format!("{:02}.{:02}.{:02}", year % 100, month, day), // YMD .
                    6 => alloc::format!("{:02}-{:02}-{:02}", year % 100, month, day), // YMD -
                    16 => alloc::format!("{:02}-{:02}-{:02}", month, day, year % 100), // MDY -
                    _ => alloc::format!("{:02}/{:02}/{:02}", month, day, year % 100),
                };
                write_c_string(memory, dst, &s);
            }
            cpu.d[0] = 0;
        }
        0xA267 => {
            // DateToDOWDMFormat(month, day, year, format, pString)
            let sp = cpu.a[7];
            let mut idx = 0u32;
            let mut month = trap_arg_u8(memory, sp, &mut idx) as u16;
            let mut day = trap_arg_u8(memory, sp, &mut idx) as u16;
            let mut year = trap_arg_u16(memory, sp, &mut idx);
            let date_format = trap_arg_u8(memory, sp, &mut idx);
            let dst = resolve_out_ptr(cpu, memory, trap_arg_u32(memory, sp, &mut idx));
            if month == 0 || month > 12 {
                month = 1;
            }
            if day == 0 || day > 31 {
                day = 1;
            }
            if year < 100 {
                year = year.saturating_add(1900);
            }
            let dow = {
                let (y, m) = if month < 3 {
                    (year as i32 - 1, month as i32 + 12)
                } else {
                    (year as i32, month as i32)
                };
                let d = day as i32;
                let h = (d
                    + ((13 * (m + 1)) / 5)
                    + (y % 100)
                    + ((y % 100) / 4)
                    + ((y / 100) / 4)
                    + (5 * (y / 100)))
                    % 7;
                ((h + 6) % 7) as usize
            };
            let dow_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            if dst != 0 {
                let mut date_buf = [0u8; 40];
                let date_s = match date_format {
                    0 => alloc::format!("{:02}/{:02}/{:02}", month, day, year % 100),
                    1 => alloc::format!("{:02}/{:02}/{:02}", day, month, year % 100),
                    2 => alloc::format!("{:02}.{:02}.{:02}", day, month, year % 100),
                    3 => alloc::format!("{:02}-{:02}-{:02}", day, month, year % 100),
                    4 => alloc::format!("{:02}/{:02}/{:02}", year % 100, month, day),
                    5 => alloc::format!("{:02}.{:02}.{:02}", year % 100, month, day),
                    6 => alloc::format!("{:02}-{:02}-{:02}", year % 100, month, day),
                    16 => alloc::format!("{:02}-{:02}-{:02}", month, day, year % 100),
                    _ => alloc::format!("{:02}/{:02}/{:02}", month, day, year % 100),
                };
                let bytes = date_s.as_bytes();
                let max = core::cmp::min(bytes.len(), date_buf.len().saturating_sub(1));
                date_buf[..max].copy_from_slice(&bytes[..max]);
                let suffix = core::str::from_utf8(&date_buf[..max]).unwrap_or("");
                write_c_string(memory, dst, &alloc::format!("{} {}", dow_names[dow], suffix));
            }
            cpu.d[0] = 0;
        }
        0xA268 => {
            // TimeToAscii(hours, minutes, format, pString)
            let sp = cpu.a[7];
            let mut idx = 0u32;
            let hour = trap_arg_u8(memory, sp, &mut idx).min(23) as u16;
            let minute = trap_arg_u8(memory, sp, &mut idx).min(59) as u16;
            let time_format = trap_arg_u8(memory, sp, &mut idx);
            let dst = resolve_out_ptr(cpu, memory, trap_arg_u32(memory, sp, &mut idx));
            if dst != 0 {
                let (h24, sep, ampm_mode) = match time_format {
                    1 => (false, Some(':'), true), // tfColonAMPM
                    2 => (true, Some(':'), false), // tfColon24h
                    3 => (true, Some('.'), false), // tfDot
                    4 => (false, Some('.'), true), // tfDotAMPM
                    5 => (true, Some('.'), false), // tfDot24h
                    6 => (false, None, true),      // tfHoursAMPM
                    7 => (true, None, false),      // tfHours24h
                    8 => (true, Some(','), false), // tfComma24h
                    _ => (true, Some(':'), false), // tfColon
                };
                let s = if h24 {
                    match sep {
                        Some(c) => alloc::format!("{}{}{:02}", hour, c, minute),
                        None => alloc::format!("{}", hour),
                    }
                } else {
                    let mut h12 = hour;
                    let suffix = if h12 < 12 { "am" } else { "pm" };
                    if h12 == 0 {
                        h12 = 12;
                    } else if h12 > 12 {
                        h12 -= 12;
                    }
                    match sep {
                        Some(c) if ampm_mode => alloc::format!("{}{}{:02} {}", h12, c, minute, suffix),
                        Some(c) => alloc::format!("{}{}{:02}", h12, c, minute),
                        None if ampm_mode => alloc::format!("{} {}", h12, suffix),
                        None => alloc::format!("{}", h12),
                    }
                };
                write_c_string(memory, dst, &s);
            }
            cpu.d[0] = 0;
        }
        0xA22C => {
            // PrefGetPreferences(prefsP)
            let sp = cpu.a[7];
            let prefs_p = [memory.read_u32_be(sp).unwrap_or(0), cpu.a[0], cpu.a[1]]
                .into_iter()
                .find(|p| *p != 0 && memory.contains_addr(*p))
                .unwrap_or(0);
            if prefs_p != 0 {
                // Minimal stable defaults expected by older apps.
                // Most fields are zeroed; set country to US (0), date/time format defaults.
                for i in 0..64u32 {
                    let _ = memory.write_u8(prefs_p.saturating_add(i), 0);
                }
            }
            cpu.d[0] = 0;
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
                // Palm compatibility defaults for common startup probes.
                let sys_creator = u32::from_be_bytes(*b"psys");
                let gdbs_creator = u32::from_be_bytes(*b"gdbS");
                if creator == sys_creator && num == 1 {
                    // sysFtrNumROMVersion: emulate a modern PalmOS ROM so apps
                    // don't abort on minimum-version checks.
                    memory.write_u32_be(value_p, 0x0500_0000);
                    cpu.d[0] = 0;
                } else if creator == gdbs_creator {
                    // Optional Graffiti database feature probes.
                    memory.write_u32_be(value_p, 0);
                    cpu.d[0] = 0;
                } else {
                    memory.write_u32_be(value_p, 0);
                    cpu.d[0] = 1;
                }
            }
        }
        0xA2E9 => {
            // UInt16 SysTicksPerSecond(): write low word and preserve upper word.
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | 100;
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
            // Int16 return in low word; preserve upper word for ABI fidelity.
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | (rnd15 & 0xFFFF);
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail SysRandom new_seed=0x{:08X} state=0x{:08X} out=0x{:04X}",
                    new_seed,
                    runtime.rand_state,
                    rnd15 & 0xFFFF
                );
            }
        }
        0xA0C7 => {
            let ptr = decode_ptr_arg_from_stack(cpu, memory, 0);
            let ptr = if ptr != 0 { ptr } else { cpu.a[0].max(cpu.d[0]) };
            cpu.d[0] = read_c_string(memory, ptr).len() as u32;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail StrLen ptr=0x{:08X} -> {}",
                    ptr,
                    cpu.d[0]
                );
            }
        }
        0xA0C5 => {
            let dst = decode_ptr_arg_from_stack(cpu, memory, 0);
            let src = decode_ptr_arg_from_stack(cpu, memory, 4);
            let dst = if dst != 0 { dst } else { cpu.a[0].max(cpu.d[0]) };
            let src = if src != 0 { src } else { cpu.a[1].max(cpu.d[1]) };
            let mut bytes = read_c_string(memory, src);
            bytes.push(0);
            write_bytes(runtime, memory, dst, &bytes);
            cpu.a[0] = dst;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail StrCopy dst=0x{:08X} src=0x{:08X} chars={}",
                    dst,
                    src,
                    bytes.len().saturating_sub(1)
                );
            }
        }
        0xA180 => {
            // UInt16 FrmGetObjectIndex(const FormType *formP, UInt16 objID)
            let sp = cpu.a[7];
            let form_h = [
                memory.read_u32_be(sp).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                cpu.a[0],
                cpu.d[0],
            ]
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
            .unwrap_or(runtime.active_form_handle);
            let obj_id = [
                memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0),
                memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0),
                (cpu.d[1] & 0xFFFF) as u16,
                (cpu.a[1] & 0xFFFF) as u16,
            ]
            .into_iter()
            .find(|v| *v != 0)
            .unwrap_or(0);
            let form_id = decode_form_id_from_handle_or_active(runtime, form_h);
            let mut idx = 0xFFFFu16;
            if let Some(fid) = form_id {
                if let Some(found) = runtime
                    .form_objects
                    .iter()
                    .find(|o| o.form_id == fid && o.object_id == obj_id)
                {
                    idx = found.object_index;
                }
            }
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | idx as u32;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail FrmGetObjectIndex form=0x{:08X} obj_id=0x{:04X} -> idx=0x{:04X}",
                    form_h,
                    obj_id,
                    idx
                );
            }
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
                        runtime.drawn_bitmaps.push(crate::palm::runtime::RuntimeBitmapDraw {
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
            // void *FrmGetObjectPtr(const FormType *formP, UInt16 objIndex)
            let sp = cpu.a[7];
            let form_h = [
                memory.read_u32_be(sp).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
                cpu.a[0],
                cpu.d[0],
            ]
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
            .unwrap_or(runtime.active_form_handle);
            let obj_index = [
                memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0xFFFF),
                memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0xFFFF),
                (cpu.d[1] & 0xFFFF) as u16,
                (cpu.a[1] & 0xFFFF) as u16,
            ]
            .into_iter()
            .find(|v| *v != 0xFFFF)
            .unwrap_or(0xFFFF);
            let form_id = decode_form_id_from_handle_or_active(runtime, form_h);
            let ptr = if let Some(fid) = form_id {
                runtime
                    .form_objects
                    .iter()
                    .find(|o| o.form_id == fid && o.object_index == obj_index)
                    .map(|o| o.ptr)
                    .unwrap_or(0)
            } else {
                0
            };
            cpu.a[0] = ptr;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail FrmGetObjectPtr form=0x{:08X} obj_idx=0x{:04X} -> ptr=0x{:08X}",
                    form_h,
                    obj_index,
                    ptr
                );
            }
        }
        0xA153 => {
            // MemHandle FldGetTextHandle(const FieldType *fldP)
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let handle = find_field_obj_index(runtime, fld_p)
                .and_then(|i| runtime.form_objects.get(i).map(|o| o.text_handle))
                .unwrap_or(0);
            cpu.a[0] = handle;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail FldGetTextHandle fld=0x{:08X} -> handle=0x{:08X}",
                    fld_p,
                    handle
                );
            }
        }
        0xA158 => {
            // void FldSetTextHandle(FieldType *fldP, MemHandle textHandle)
            let sp = cpu.a[7];
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let text_h = [
                memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
                memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0),
                cpu.a[1],
                cpu.d[1],
            ]
            .into_iter()
            .find(|h| *h == 0 || runtime.mem_blocks.iter().any(|b| b.handle == *h))
            .unwrap_or(0);
            if let Some(obj_idx) = find_field_obj_index(runtime, fld_p) {
                let len = if text_h == 0 {
                    0
                } else if let Some(ptr) = lock_handle(runtime, memory, text_h) {
                    read_c_string(memory, ptr).len().min(u16::MAX as usize) as u16
                } else {
                    0
                };
                if let Some(obj) = runtime.form_objects.get_mut(obj_idx) {
                    obj.text_handle = text_h;
                    obj.sel_start = len;
                    obj.sel_end = len;
                    obj.ins_pt = len;
                }
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    let field_id = runtime
                        .form_objects
                        .get(obj_idx)
                        .map(|o| o.object_id)
                        .unwrap_or(0);
                    log::info!(
                        "PRC trap detail FldSetTextHandle fld=0x{:08X} field_id=0x{:04X} handle=0x{:08X}",
                        fld_p,
                        field_id,
                        text_h
                    );
                }
            }
            cpu.d[0] = 0;
        }
        0xA139 => {
            // Char* FldGetTextPtr(const FieldType *fldP)
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let ptr = find_field_obj_index(runtime, fld_p)
                .and_then(|idx| runtime.form_objects.get(idx).map(|o| o.text_handle))
                .and_then(|h| if h != 0 { lock_handle(runtime, memory, h) } else { None })
                .unwrap_or(0);
            cpu.a[0] = ptr;
            cpu.d[0] = ptr;
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail FldGetTextPtr fld=0x{:08X} -> ptr=0x{:08X}",
                    fld_p,
                    ptr
                );
            }
        }
        0xA14B => {
            // UInt16 FldGetTextLength(const FieldType *fldP)
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let len = find_field_obj_index(runtime, fld_p)
                .map(|idx| read_field_text(runtime, memory, idx).len().min(u16::MAX as usize) as u16)
                .unwrap_or(0);
            cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | (len as u32);
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail FldGetTextLength fld=0x{:08X} -> {}",
                    fld_p,
                    len
                );
            }
        }
        0xA13A => {
            // void FldGetSelection(const FieldType *fldP, UInt16 *startP, UInt16 *endP)
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let sp = cpu.a[7];
            let start_p = [
                decode_ptr_arg_from_stack(cpu, memory, 4),
                decode_ptr_arg_from_stack(cpu, memory, 6),
                cpu.a[1],
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            let end_p = [
                decode_ptr_arg_from_stack(cpu, memory, 8),
                decode_ptr_arg_from_stack(cpu, memory, 10),
                decode_ptr_arg_from_stack(cpu, memory, 12),
                memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0),
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            let (sel_start, sel_end) = find_field_obj_index(runtime, fld_p)
                .and_then(|idx| runtime.form_objects.get(idx).map(|o| (o.sel_start, o.sel_end)))
                .unwrap_or((0, 0));
            if start_p != 0 {
                let _ = memory.write_u16_be(start_p, sel_start);
            }
            if end_p != 0 {
                let _ = memory.write_u16_be(end_p, sel_end);
            }
            cpu.d[0] = 0;
        }
        0xA142 => {
            // void FldSetSelection(FieldType *fldP, UInt16 start, UInt16 end)
            let sp = cpu.a[7];
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            if let Some(idx) = find_field_obj_index(runtime, fld_p) {
                let text_len = read_field_text(runtime, memory, idx).len().min(u16::MAX as usize) as u16;
                let start = [
                    memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0),
                    memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0),
                    (cpu.d[1] & 0xFFFF) as u16,
                ]
                .into_iter()
                .find(|v| *v != 0)
                .unwrap_or(0)
                .min(text_len);
                let end = [
                    memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0),
                    memory.read_u16_be(sp.saturating_add(8)).unwrap_or(0),
                    (cpu.d[2] & 0xFFFF) as u16,
                ]
                .into_iter()
                .find(|v| *v != 0 || start == 0)
                .unwrap_or(start)
                .min(text_len);
                if let Some(obj) = runtime.form_objects.get_mut(idx) {
                    obj.sel_start = start.min(end);
                    obj.sel_end = start.max(end);
                    obj.ins_pt = obj.sel_end;
                }
            }
            cpu.d[0] = 0;
        }
        0xA15D => {
            // Boolean FldInsert(FieldType *fldP, const Char *insertChars, UInt16 insertLen)
            let sp = cpu.a[7];
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let Some(obj_idx) = find_field_obj_index(runtime, fld_p) else {
                cpu.d[0] = 0;
                return;
            };
            let insert_p = [
                decode_ptr_arg_from_stack(cpu, memory, 4),
                decode_ptr_arg_from_stack(cpu, memory, 6),
                decode_ptr_arg_from_stack(cpu, memory, 8),
                cpu.a[1],
                cpu.d[1],
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            let mut insert_len = [
                memory.read_u16_be(sp.saturating_add(8)).unwrap_or(0),
                memory.read_u16_be(sp.saturating_add(10)).unwrap_or(0),
                memory.read_u16_be(sp.saturating_add(12)).unwrap_or(0),
                (cpu.d[2] & 0xFFFF) as u16,
            ]
            .into_iter()
            .find(|v| *v != 0)
            .unwrap_or(0) as usize;
            if insert_len == 0 && insert_p != 0 {
                insert_len = read_c_string(memory, insert_p).len();
            }
            let mut insert_bytes = Vec::with_capacity(insert_len);
            for i in 0..insert_len {
                let b = memory.read_u8(insert_p.saturating_add(i as u32)).unwrap_or(0);
                insert_bytes.push(b);
            }
            let (sel_start, sel_end, object_id) = runtime
                .form_objects
                .get(obj_idx)
                .map(|o| (o.sel_start as usize, o.sel_end as usize, o.object_id))
                .unwrap_or((0, 0, 0));
            let ok = field_apply_replace(runtime, memory, obj_idx, sel_start, sel_end, &insert_bytes);
            if ok {
                runtime.event_queue.insert(
                    0,
                    RuntimeEvent {
                        e_type: EVT_FLD_CHANGED,
                        data_u16: object_id,
                    },
                );
            }
            cpu.d[0] = if ok { 1 } else { 0 };
        }
        0xA15E => {
            // void FldDelete(FieldType *fldP, UInt16 start, UInt16 end)
            let sp = cpu.a[7];
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            if let Some(obj_idx) = find_field_obj_index(runtime, fld_p) {
                let (def_start, def_end, object_id) = runtime
                    .form_objects
                    .get(obj_idx)
                    .map(|o| (o.sel_start as usize, o.sel_end as usize, o.object_id))
                    .unwrap_or((0, 0, 0));
                let start = [
                    memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0),
                    memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0),
                    (cpu.d[1] & 0xFFFF) as u16,
                ]
                .into_iter()
                .find(|v| *v != 0 || def_start == 0)
                .unwrap_or(def_start as u16) as usize;
                let end = [
                    memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0),
                    memory.read_u16_be(sp.saturating_add(8)).unwrap_or(0),
                    (cpu.d[2] & 0xFFFF) as u16,
                ]
                .into_iter()
                .find(|v| *v != 0 || def_end == 0)
                .unwrap_or(def_end as u16) as usize;
                if field_apply_replace(runtime, memory, obj_idx, start, end, &[]) {
                    runtime.event_queue.insert(
                        0,
                        RuntimeEvent {
                            e_type: EVT_FLD_CHANGED,
                            data_u16: object_id,
                        },
                    );
                }
            }
            cpu.d[0] = 0;
        }
        0xA13B => {
            // Boolean FldHandleEvent(FieldType *fldP, EventType *eventP)
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            let Some(obj_idx) = find_field_obj_index(runtime, fld_p) else {
                cpu.d[0] = 0;
                return;
            };
            let event_p = [
                decode_ptr_arg_from_stack(cpu, memory, 4),
                decode_ptr_arg_from_stack(cpu, memory, 6),
                runtime.evt_event_p,
            ]
            .into_iter()
            .find(|p| *p != 0 && memory.contains_addr(*p))
            .unwrap_or(0);
            let evt_type = memory.read_u16_be(event_p).unwrap_or(0xFFFF);
            let mut handled = false;
            match evt_type {
                EVT_PEN_DOWN => {
                    let object_id = runtime
                        .form_objects
                        .get(obj_idx)
                        .map(|o| o.object_id)
                        .unwrap_or(0);
                    runtime.event_queue.insert(
                        0,
                        RuntimeEvent {
                            e_type: EVT_FLD_ENTER,
                            data_u16: object_id,
                        },
                    );
                    handled = true;
                }
                EVT_FLD_ENTER => {
                    let text_len = read_field_text(runtime, memory, obj_idx).len().min(u16::MAX as usize) as u16;
                    if let Some(obj) = runtime.form_objects.get_mut(obj_idx) {
                        let pos = obj.ins_pt.min(text_len);
                        obj.sel_start = pos;
                        obj.sel_end = pos;
                        obj.ins_pt = pos;
                        runtime.focused_field_index = Some(obj.object_index);
                    }
                    handled = true;
                }
                EVT_KEY_DOWN => {
                    let chr = memory.read_u16_be(event_p.saturating_add(8)).unwrap_or(0);
                    let key_code = memory.read_u16_be(event_p.saturating_add(10)).unwrap_or(chr);
                    let (sel_start, sel_end, ins_pt, object_id) = runtime
                        .form_objects
                        .get(obj_idx)
                        .map(|o| (o.sel_start as usize, o.sel_end as usize, o.ins_pt as usize, o.object_id))
                        .unwrap_or((0, 0, 0, 0));
                    let text_len = read_field_text(runtime, memory, obj_idx).len();
                    let mut changed = false;
                    if chr == 0x08 || chr == 0x7F {
                        let (start, end) = if sel_start != sel_end {
                            (sel_start.min(sel_end), sel_start.max(sel_end))
                        } else if ins_pt > 0 {
                            (ins_pt - 1, ins_pt)
                        } else {
                            (0, 0)
                        };
                        if end > start {
                            changed = field_apply_replace(runtime, memory, obj_idx, start, end, &[]);
                        }
                        handled = true;
                    } else if (0x20..0x7F).contains(&chr) {
                        let ch = [chr as u8];
                        changed = field_apply_replace(runtime, memory, obj_idx, sel_start, sel_end, &ch);
                        handled = true;
                    } else if chr == 0x1C || key_code == 0x1C {
                        let new_pos = if sel_start != sel_end {
                            sel_start.min(sel_end)
                        } else {
                            ins_pt.saturating_sub(1)
                        };
                        if let Some(obj) = runtime.form_objects.get_mut(obj_idx) {
                            let pos = new_pos.min(text_len).min(u16::MAX as usize) as u16;
                            obj.sel_start = pos;
                            obj.sel_end = pos;
                            obj.ins_pt = pos;
                        }
                        handled = true;
                    } else if chr == 0x1D || key_code == 0x1D {
                        let new_pos = if sel_start != sel_end {
                            sel_start.max(sel_end)
                        } else {
                            (ins_pt + 1).min(text_len)
                        };
                        if let Some(obj) = runtime.form_objects.get_mut(obj_idx) {
                            let pos = new_pos.min(text_len).min(u16::MAX as usize) as u16;
                            obj.sel_start = pos;
                            obj.sel_end = pos;
                            obj.ins_pt = pos;
                        }
                        handled = true;
                    }
                    if changed {
                        runtime.event_queue.insert(
                            0,
                            RuntimeEvent {
                                e_type: EVT_FLD_CHANGED,
                                data_u16: object_id,
                            },
                        );
                    }
                }
                _ => {}
            }
            cpu.d[0] = if handled { 1 } else { 0 };
        }
        0xA135 => {
            // void FldDrawField(FieldType *fldP)
            let fld_p = decode_field_ptr(runtime, cpu, memory);
            if let Some(idx) = find_field_obj_index(runtime, fld_p) {
                let field_id = runtime
                    .form_objects
                    .get(idx)
                    .map(|o| o.object_id)
                    .unwrap_or(0);
                let chars = read_field_text(runtime, memory, idx).len();
                sync_field_draw_from_obj(runtime, memory, idx);
                if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                    log::info!(
                        "PRC trap detail FldDrawField fld=0x{:08X} field_id=0x{:04X} chars={}",
                        fld_p,
                        field_id,
                        chars
                    );
                }
            }
            cpu.d[0] = 0;
        }
        0xA195 => {
            // UInt16 FrmHelp(UInt16 helpMsgId)
            let sp = cpu.a[7];
            let help_id = memory
                .read_u16_be(sp)
                .unwrap_or((cpu.d[0] & 0xFFFF) as u16);
            let tstr = u32::from_be_bytes(*b"tSTR");
            let text = runtime
                .resources
                .iter()
                .find(|res| res.kind == tstr && res.id == help_id)
                .map(|res| normalize_tstr_payload(&res.data))
                .map(|bytes| {
                    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                    alloc::string::String::from_utf8_lossy(&bytes[..end]).into_owned()
                })
                .unwrap_or_else(|| alloc::format!("Help {}", help_id));
            runtime.help_dialog = Some(crate::palm::runtime::RuntimeHelpDialog {
                help_id,
                text: text.clone(),
                scroll_line: 0,
            });
            if runtime.trace_traps && runtime.trace_trap_budget > 0 {
                log::info!(
                    "PRC trap detail FrmHelp help_id=0x{:04X} chars={}",
                    help_id,
                    text.len()
                );
            }
            cpu.d[0] = 0;
        }
        0xA194 => {
            // UInt16 FrmCustomAlert(alertId, s1, s2, s3)
            // Keep flow moving and return default button index.
            cpu.d[0] = 0;
        }
        0xA1D3 | 0xA1EA => {
            // TblSetRowUsable / TblSetColumnUsable.
            // Table UI is not modeled yet; acknowledge and continue.
            cpu.d[0] = 0;
        }
        0xA1CA | 0xA1D5 | 0xA1F5 => {
            // TblDrawTable / TblSetCustomDrawProcedure / TblSetColumnSpacing.
            // Keep table setup paths alive while full table emulation lands.
            cpu.d[0] = 0;
        }
        0xA10F => {
            // CtlHideControl
            cpu.d[0] = 0;
        }
        0xA1A1 | 0xA234 | 0xA9F0 => {
            cpu.d[0] = 0;
        }
        _ => {
            // Default exploratory stub: keep execution moving and record what needs real impls.
            cpu.d[0] = 0;
        }
    }
}
