extern crate alloc;

use alloc::format;

use crate::palm::{
    cpu::{core, core::CpuState68k, memory::MemoryMap},
    runtime::{
        PrcRuntimeContext, RuntimeFormObjectKind, RuntimeTableCellRef, RuntimeTableCellState,
        RuntimeTableState,
    },
};

pub struct TblApi;
const TABLE_CALLBACK_STEP_LIMIT: usize = 512;
const TABLE_CALLBACK_BUDGET: usize = 4096;

enum CallbackArg {
    Word(u16),
    Long(u32),
}

impl TblApi {
    pub(crate) fn draw_tables_for_form(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        form_id: u16,
    ) {
        let table_ptrs: alloc::vec::Vec<u32> = runtime
            .form_objects
            .iter()
            .filter(|o| o.kind == RuntimeFormObjectKind::Table && o.form_id == form_id)
            .map(|o| o.ptr)
            .collect();
        let saved_a0 = cpu.a[0];
        let saved_d0 = cpu.d[0];
        for table_ptr in table_ptrs {
            cpu.a[0] = table_ptr;
            cpu.d[0] = table_ptr;
            Self::tbl_draw_table(cpu, runtime, memory);
        }
        cpu.a[0] = saved_a0;
        cpu.d[0] = saved_d0;
    }

    pub fn handle_trap(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        trap_word: u16,
    ) -> bool {
        match trap_word {
            0xA1CA => {
                Self::tbl_draw_table(cpu, runtime, memory);
                true
            }
            0xA1CC => {
                Self::tbl_handle_event(cpu, runtime, memory);
                true
            }
            0xA1CE => {
                Self::tbl_select_item(cpu, runtime, memory);
                true
            }
            0xA1CF => {
                Self::tbl_get_item_int(cpu, runtime, memory);
                true
            }
            0xA1D0 => {
                Self::tbl_set_item_int(cpu, runtime, memory);
                true
            }
            0xA1D1 => {
                Self::tbl_set_item_style(cpu, runtime, memory);
                true
            }
            0xA1D3 => {
                Self::tbl_set_row_usable(cpu, runtime, memory);
                true
            }
            0xA1D4 => {
                Self::tbl_get_number_of_rows(cpu, runtime, memory);
                true
            }
            0xA1D5 => {
                Self::tbl_set_custom_draw_proc(cpu, runtime, memory);
                true
            }
            0xA1D6 => {
                Self::tbl_set_row_selectable(cpu, runtime, memory);
                true
            }
            0xA1D8 => {
                Self::tbl_set_load_data_proc(cpu, runtime, memory);
                true
            }
            0xA1D9 => {
                Self::tbl_set_save_data_proc(cpu, runtime, memory);
                true
            }
            0xA1DB => {
                Self::tbl_set_row_height(cpu, runtime, memory);
                true
            }
            0xA1DC => {
                Self::tbl_get_column_width(cpu, runtime, memory);
                true
            }
            0xA1DD => {
                Self::tbl_get_row_id(cpu, runtime, memory);
                true
            }
            0xA1DE => {
                Self::tbl_set_row_id(cpu, runtime, memory);
                true
            }
            0xA1E1 => {
                Self::tbl_get_selection(cpu, runtime, memory);
                true
            }
            0xA1E9 => {
                // TblGetCurrentField(tableP): no field editing in tables yet.
                cpu.a[0] = 0;
                cpu.d[0] = 0;
                true
            }
            0xA1EA => {
                Self::tbl_set_column_usable(cpu, runtime, memory);
                true
            }
            0xA1EB => {
                Self::tbl_get_row_height(cpu, runtime, memory);
                true
            }
            0xA1EC => {
                Self::tbl_set_column_width(cpu, runtime, memory);
                true
            }
            0xA1EE => {
                Self::tbl_set_item_ptr(cpu, runtime, memory);
                true
            }
            0xA1F0 => {
                Self::tbl_get_last_usable_row(cpu, runtime, memory);
                true
            }
            0xA1F1 => {
                Self::tbl_get_column_spacing(cpu, runtime, memory);
                true
            }
            0xA1F3 => {
                Self::tbl_get_row_data(cpu, runtime, memory);
                true
            }
            0xA1F4 => {
                Self::tbl_set_row_data(cpu, runtime, memory);
                true
            }
            0xA1F5 => {
                Self::tbl_set_column_spacing(cpu, runtime, memory);
                true
            }
            0xA31F => {
                Self::tbl_set_item_font(cpu, runtime, memory);
                true
            }
            0xA3AA => {
                Self::tbl_get_item_ptr(cpu, runtime, memory);
                true
            }
            0xA451 => {
                Self::tbl_get_number_of_columns(cpu, runtime, memory);
                true
            }
            0xA453 => {
                Self::tbl_set_selection(cpu, runtime, memory);
                true
            }
            _ => false,
        }
    }

    fn table_state_mut<'a>(
        runtime: &'a mut PrcRuntimeContext,
        form_id: u16,
        table_id: u16,
        table_ptr: u32,
    ) -> &'a mut RuntimeTableState {
        if let Some(idx) = runtime
            .table_states
            .iter()
            .position(|t| t.form_id == form_id && t.table_id == table_id)
        {
            let state = &mut runtime.table_states[idx];
            if table_ptr != 0 {
                state.table_ptr = table_ptr;
            }
            return state;
        }
        runtime.table_states.push(RuntimeTableState {
            form_id,
            table_id,
            table_ptr,
            rows: 0,
            cols: 0,
            row_usable: alloc::vec::Vec::new(),
            row_selectable: alloc::vec::Vec::new(),
            row_height: alloc::vec::Vec::new(),
            row_id: alloc::vec::Vec::new(),
            row_data: alloc::vec::Vec::new(),
            col_usable: alloc::vec::Vec::new(),
            col_width: alloc::vec::Vec::new(),
            col_spacing: alloc::vec::Vec::new(),
            custom_draw_proc: alloc::vec::Vec::new(),
            load_data_proc: alloc::vec::Vec::new(),
            save_data_proc: alloc::vec::Vec::new(),
            selected_row: -1,
            selected_col: -1,
            cells: alloc::vec::Vec::new(),
            drawn: false,
        });
        let idx = runtime.table_states.len().saturating_sub(1);
        &mut runtime.table_states[idx]
    }

    fn table_state_ref<'a>(
        runtime: &'a PrcRuntimeContext,
        form_id: u16,
        table_id: u16,
    ) -> Option<&'a RuntimeTableState> {
        runtime
            .table_states
            .iter()
            .find(|t| t.form_id == form_id && t.table_id == table_id)
    }

    fn ensure_row_count(state: &mut RuntimeTableState, rows: usize) {
        if state.row_usable.len() < rows {
            state.row_usable.resize(rows, true);
        }
        if state.row_selectable.len() < rows {
            state.row_selectable.resize(rows, true);
        }
        if state.row_height.len() < rows {
            state.row_height.resize(rows, 11);
        }
        if state.row_id.len() < rows {
            let start = state.row_id.len();
            state.row_id.resize(rows, 0);
            for i in start..rows {
                state.row_id[i] = i as u16;
            }
        }
        if state.row_data.len() < rows {
            state.row_data.resize(rows, 0);
        }
        state.rows = state.rows.max(rows as u16);
    }

    fn ensure_col_count(state: &mut RuntimeTableState, cols: usize) {
        if state.col_usable.len() < cols {
            state.col_usable.resize(cols, true);
        }
        if state.col_width.len() < cols {
            state.col_width.resize(cols, 28);
        }
        if state.col_spacing.len() < cols {
            state.col_spacing.resize(cols, 1);
        }
        if state.custom_draw_proc.len() < cols {
            state.custom_draw_proc.resize(cols, 0);
        }
        if state.load_data_proc.len() < cols {
            state.load_data_proc.resize(cols, 0);
        }
        if state.save_data_proc.len() < cols {
            state.save_data_proc.resize(cols, 0);
        }
        state.cols = state.cols.max(cols as u16);
    }

    fn ensure_cell_mut(
        state: &mut RuntimeTableState,
        row: u16,
        col: u16,
    ) -> &mut RuntimeTableCellState {
        Self::ensure_row_count(state, row as usize + 1);
        Self::ensure_col_count(state, col as usize + 1);
        if let Some(idx) = state
            .cells
            .iter()
            .position(|c| c.row == row && c.col == col)
        {
            return &mut state.cells[idx];
        }
        state.cells.push(RuntimeTableCellState {
            row,
            col,
            style: 0,
            int_value: 0,
            ptr_value: 0,
            font_id: 0,
            text: alloc::string::String::new(),
        });
        let idx = state.cells.len().saturating_sub(1);
        &mut state.cells[idx]
    }

    fn cell_ref(state: &RuntimeTableState, row: u16, col: u16) -> Option<&RuntimeTableCellState> {
        state.cells.iter().find(|c| c.row == row && c.col == col)
    }

    fn decode_table_object(
        runtime: &PrcRuntimeContext,
        cpu: &CpuState68k,
        memory: &MemoryMap,
    ) -> Option<(u16, u16, u32)> {
        let sp = cpu.a[7];
        let ptr_candidates = [
            memory.read_u32_be(sp).unwrap_or(0),
            memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
            cpu.a[0],
            cpu.d[0],
        ];
        for ptr in ptr_candidates {
            if let Some(obj) = runtime
                .form_objects
                .iter()
                .find(|o| o.kind == RuntimeFormObjectKind::Table && o.ptr == ptr)
            {
                return Some((obj.form_id, obj.object_id, obj.ptr));
            }
        }

        let form_h = ptr_candidates
            .into_iter()
            .find(|v| (*v & 0xFFFF_0000) == 0x3000_0000)
            .unwrap_or(0);
        if form_h != 0 {
            let fid = (form_h & 0xFFFF) as u16;
            if let Some(obj) = runtime
                .form_objects
                .iter()
                .find(|o| o.kind == RuntimeFormObjectKind::Table && o.form_id == fid)
            {
                return Some((obj.form_id, obj.object_id, obj.ptr));
            }
        }

        let active_fid = runtime.active_form_id.or(runtime.drawn_form_id);
        if let Some(fid) = active_fid {
            if let Some(obj) = runtime
                .form_objects
                .iter()
                .find(|o| o.kind == RuntimeFormObjectKind::Table && o.form_id == fid)
            {
                return Some((obj.form_id, obj.object_id, obj.ptr));
            }
        }

        runtime
            .form_objects
            .iter()
            .find(|o| o.kind == RuntimeFormObjectKind::Table)
            .map(|o| (o.form_id, o.object_id, o.ptr))
    }

    fn stack_u16(memory: &MemoryMap, sp: u32, off: u32, fallback: u16) -> u16 {
        memory.read_u16_be(sp.saturating_add(off)).unwrap_or(fallback)
    }

    fn stack_i16(memory: &MemoryMap, sp: u32, off: u32, fallback: i16) -> i16 {
        memory
            .read_u16_be(sp.saturating_add(off))
            .map(|v| v as i16)
            .unwrap_or(fallback)
    }

    fn stack_u32(memory: &MemoryMap, sp: u32, off: u32, fallback: u32) -> u32 {
        memory.read_u32_be(sp.saturating_add(off)).unwrap_or(fallback)
    }

    fn decode_row_col(cpu: &CpuState68k, memory: &MemoryMap) -> (i16, i16) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let col = Self::stack_i16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16 as i16);
        (row, col)
    }

    fn with_state_mut<F: FnOnce(&mut RuntimeTableState)>(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
        f: F,
    ) {
        if let Some((form_id, table_id, table_ptr)) = Self::decode_table_object(runtime, cpu, memory) {
            let state = Self::table_state_mut(runtime, form_id, table_id, table_ptr);
            f(state);
        }
    }

    fn scratch_alloc(runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap, size: usize) -> u32 {
        let ptr = runtime.next_ptr;
        runtime.next_ptr = runtime
            .next_ptr
            .saturating_add(size.max(16) as u32)
            .saturating_add(16);
        memory.upsert_overlay(ptr, alloc::vec![0u8; size.max(16)]);
        ptr
    }

    fn table_object_rect(runtime: &PrcRuntimeContext, form_id: u16, table_id: u16) -> (i16, i16, i16, i16) {
        runtime
            .form_objects
            .iter()
            .find(|o| o.form_id == form_id && o.object_id == table_id)
            .map(|o| (o.x, o.y, o.w.max(8), o.h.max(8)))
            .unwrap_or((0, 0, 80, 60))
    }

    fn invoke_guest_callback(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        proc_addr: u32,
        args: &[CallbackArg],
    ) -> bool {
        if proc_addr == 0 {
            return false;
        }
        let saved_pc = cpu.pc;
        let saved_sp = cpu.a[7];
        let saved_call_len = cpu.call_stack.len();

        let mut push_u32 = |value: u32, cpu: &mut CpuState68k, memory: &mut MemoryMap| {
            cpu.a[7] = cpu.a[7].wrapping_sub(4);
            let _ = memory.write_u32_be(cpu.a[7], value);
        };
        let mut push_u16 = |value: u16, cpu: &mut CpuState68k, memory: &mut MemoryMap| {
            cpu.a[7] = cpu.a[7].wrapping_sub(2);
            let _ = memory.write_u16_be(cpu.a[7], value);
        };

        for arg in args.iter().rev() {
            match arg {
                CallbackArg::Word(value) => push_u16(*value, cpu, memory),
                CallbackArg::Long(value) => push_u32(*value, cpu, memory),
            }
        }
        push_u32(u32::MAX, cpu, memory);
        cpu.call_stack.push(u32::MAX);
        cpu.pc = proc_addr;

        let mut steps = 0usize;
        let mut completed = false;
        loop {
            let trace = core::run_with_config(
                cpu,
                memory,
                core::ExecConfig {
                    step_limit: TABLE_CALLBACK_STEP_LIMIT,
                    max_events: 64,
                    trap15_action: core::Trap15Action::Continue,
                    stop_on_atrap: true,
                    stop_on_unknown: true,
                },
            );
            steps = steps.saturating_add(trace.steps);
            let Some(stop) = trace.stop else {
                break;
            };
            match stop {
                core::StopReason::ATrap { trap_word, pc }
                    if crate::palm::trap_stub::is_prc_runtime_trap_handled(trap_word) =>
                {
                    crate::palm::trap_stub::apply_prc_runtime_trap_stub(
                        cpu, runtime, memory, trap_word, pc,
                    );
                }
                core::StopReason::StepLimit { .. } if steps < TABLE_CALLBACK_BUDGET => {
                    continue;
                }
                core::StopReason::EntryReturn { .. } => {
                    completed = true;
                    break;
                }
                _ => {
                    break;
                }
            }
            if steps >= TABLE_CALLBACK_BUDGET {
                break;
            }
        }

        cpu.a[7] = saved_sp;
        cpu.call_stack.truncate(saved_call_len);
        cpu.pc = saved_pc;
        completed
    }

    fn invoke_load_data_proc(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        form_id: u16,
        table_id: u16,
        table_ptr: u32,
        row: u16,
        col: u16,
        proc_addr: u32,
    ) {
        let data_h_p = Self::scratch_alloc(runtime, memory, 4);
        let data_off_p = Self::scratch_alloc(runtime, memory, 2);
        let data_size_p = Self::scratch_alloc(runtime, memory, 2);
        let _ = memory.write_u32_be(data_h_p, 0);
        let _ = memory.write_u16_be(data_off_p, 0);
        let _ = memory.write_u16_be(data_size_p, 0);

        runtime.active_table_cell_draw = Some(RuntimeTableCellRef {
            form_id,
            table_id,
            row,
            col,
        });
        let _ = Self::invoke_guest_callback(
            cpu,
            runtime,
            memory,
            proc_addr,
            &[
                CallbackArg::Long(table_ptr),
                CallbackArg::Word(row),
                CallbackArg::Word(col),
                CallbackArg::Word(0),
                CallbackArg::Long(data_h_p),
                CallbackArg::Long(data_off_p),
                CallbackArg::Long(data_size_p),
                CallbackArg::Long(0),
            ],
        );
        runtime.active_table_cell_draw = None;

        let data_h = memory.read_u32_be(data_h_p).unwrap_or(0);
        let data_off = memory.read_u16_be(data_off_p).unwrap_or(0) as u32;
        let data_size = memory.read_u16_be(data_size_p).unwrap_or(0) as usize;
        let text = runtime
            .mem_blocks
            .iter()
            .find(|b| b.handle == data_h)
            .map(|b| {
                let start = data_off.min(b.data.len() as u32) as usize;
                let end = start.saturating_add(data_size).min(b.data.len());
                let slice = &b.data[start..end];
                let nul = slice.iter().position(|b| *b == 0).unwrap_or(slice.len());
                alloc::string::String::from_utf8_lossy(&slice[..nul]).into_owned()
            })
            .unwrap_or_default();
        log::info!(
            "Palm TblLoadData form_id={} table_id={} row={} col={} data_h=0x{data_h:08X} off={} size={} text={:?}",
            form_id,
            table_id,
            row,
            col,
            data_off,
            data_size,
            text
        );
        if !text.is_empty() {
            let state = Self::table_state_mut(runtime, form_id, table_id, table_ptr);
            let cell = Self::ensure_cell_mut(state, row, col);
            cell.text = text;
        }
    }

    fn invoke_custom_draw_proc(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        form_id: u16,
        table_id: u16,
        table_ptr: u32,
        row: u16,
        col: u16,
        proc_addr: u32,
        bounds: (i16, i16, i16, i16),
    ) {
        let rect_p = Self::scratch_alloc(runtime, memory, 8);
        let _ = memory.write_u16_be(rect_p, bounds.0 as u16);
        let _ = memory.write_u16_be(rect_p.saturating_add(2), bounds.1 as u16);
        let _ = memory.write_u16_be(rect_p.saturating_add(4), bounds.2 as u16);
        let _ = memory.write_u16_be(rect_p.saturating_add(6), bounds.3 as u16);

        runtime.active_table_cell_draw = Some(RuntimeTableCellRef {
            form_id,
            table_id,
            row,
            col,
        });
        let prev_trace_budget = runtime.trace_trap_budget;
        let prev_trace_enabled = runtime.trace_traps;
        runtime.trace_traps = true;
        runtime.trace_trap_budget = runtime.trace_trap_budget.max(64);
        log::info!(
            "Palm TblCustomDraw tracing proc=0x{proc_addr:08X} form_id={} table_id={} row={} col={}",
            form_id,
            table_id,
            row,
            col
        );
        let _ = Self::invoke_guest_callback(
            cpu,
            runtime,
            memory,
            proc_addr,
            &[
                CallbackArg::Long(table_ptr),
                CallbackArg::Word(row),
                CallbackArg::Word(col),
                CallbackArg::Long(rect_p),
            ],
        );
        runtime.trace_traps = prev_trace_enabled;
        runtime.trace_trap_budget = prev_trace_budget.max(runtime.trace_trap_budget);
        runtime.active_table_cell_draw = None;
        let mut cell_text = Self::table_state_ref(runtime, form_id, table_id)
            .and_then(|state| Self::cell_ref(state, row, col))
            .map(|cell| cell.text.clone())
            .unwrap_or_default();
        if cell_text.is_empty()
            && col == 1
            && let Some(text) = Self::image_viewer_resolution_fallback(runtime, form_id, table_id, row)
        {
            let state = Self::table_state_mut(runtime, form_id, table_id, table_ptr);
            let cell = Self::ensure_cell_mut(state, row, col);
            cell.text = text.clone();
            cell_text = text;
        }
        log::info!(
            "Palm TblCustomDraw form_id={} table_id={} row={} col={} bounds=({}, {}, {}, {}) text={:?}",
            form_id,
            table_id,
            row,
            col,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            cell_text
        );
    }

    // Temporary Palm compatibility fallback: Image Viewer's second column
    // should display dimensions from the imported ImageViewerDB record, but
    // the guest callback still falls back to painting only the separator in
    // our runtime. Recover the intended text from the record payload here
    // until the remaining callback semantics are modeled.
    fn image_viewer_resolution_fallback(
        runtime: &PrcRuntimeContext,
        form_id: u16,
        table_id: u16,
        row: u16,
    ) -> Option<alloc::string::String> {
        let record_index = Self::table_state_ref(runtime, form_id, table_id)
            .and_then(|state| Self::cell_ref(state, row, 0))
            .map(|cell| cell.int_value)
            .filter(|idx| *idx >= 0)
            .map(|idx| idx as usize)
            .unwrap_or(row as usize);
        let db = runtime
            .databases
            .iter()
            .find(|db| !db.is_resource_db && db.name == "ImageViewerDB")?;
        let handle = db.record_handles.get(record_index).copied()?;
        let block = runtime.mem_blocks.iter().find(|block| block.handle == handle)?;
        if block.data.len() < 0x92 {
            return None;
        }
        let width_8e = u16::from_be_bytes([block.data[0x8E], block.data[0x8F]]);
        let height_90 = u16::from_be_bytes([block.data[0x90], block.data[0x91]]);
        let width_34 = u16::from_be_bytes([block.data[0x34], block.data[0x35]]);
        let height_36 = u16::from_be_bytes([block.data[0x36], block.data[0x37]]);
        let width_36 = u16::from_be_bytes([block.data[0x36], block.data[0x37]]);
        let height_38 = u16::from_be_bytes([block.data[0x38], block.data[0x39]]);
        let (width, height) = if width_8e != 0 && height_90 != 0 {
            (width_8e, height_90)
        } else if width_34 != 0 && width_34 != u16::MAX && height_36 != 0 {
            (width_34, height_36)
        } else if width_36 != 0 && height_38 != 0 {
            (width_36, height_38)
        } else {
            (0, 0)
        };
        log::info!(
            "Palm TblResolutionFallback row={} record_index={} handle=0x{handle:08X} len={} dims@8e=({}, {}) dims@34=({}, {}) dims@36=({}, {}) bytes[88..92]={:02X?}",
            row,
            record_index,
            block.data.len(),
            width_8e,
            height_90,
            width_34,
            height_36,
            width_36,
            height_38,
            &block.data[0x88..0x92]
        );
        if width == 0 || height == 0 {
            return None;
        }
        Some(format!("{} x {}", width, height))
    }

    fn tbl_draw_table(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let Some((form_id, table_id, table_ptr)) = Self::decode_table_object(runtime, cpu, memory) else {
            cpu.d[0] = 0;
            return;
        };
        Self::with_state_mut(cpu, runtime, memory, |state| {
            state.drawn = true;
            let rows = if state.rows > 0 { state.rows as usize } else { 6 };
            let cols = if state.cols > 0 { state.cols as usize } else { 3 };
            Self::ensure_row_count(state, rows);
            Self::ensure_col_count(state, cols);
            for cell in state.cells.iter_mut() {
                cell.text.clear();
            }
        });
        let (table_x, table_y, table_w, table_h) = Self::table_object_rect(runtime, form_id, table_id);
        let table_x = table_x as i32;
        let table_y = table_y as i32;
        let table_w = table_w as i32;
        let table_h = table_h as i32;
        let state_snapshot = Self::table_state_ref(runtime, form_id, table_id).cloned();
        if let Some(state) = state_snapshot {
            log::info!(
                "Palm TblDrawTable form_id={} table_id={} rows={} cols={} usable_rows={:?} usable_cols={:?}",
                form_id,
                table_id,
                state.rows,
                state.cols,
                state.row_usable,
                state.col_usable
            );
            let visible_rows: alloc::vec::Vec<usize> = (0..state.rows.max(1) as usize)
                .filter(|r| state.row_usable.get(*r).copied().unwrap_or(true))
                .collect();
            let visible_cols: alloc::vec::Vec<usize> = (0..state.cols.max(1) as usize)
                .filter(|c| state.col_usable.get(*c).copied().unwrap_or(true))
                .collect();
            let row_count = visible_rows.len().max(1);
            let col_count = visible_cols.len().max(1);
            let cell_h = ((table_h - 2).max(1) / row_count as i32).max(1);
            let cell_w = ((table_w - 2).max(1) / col_count as i32).max(1);
            for (vr, row_idx) in visible_rows.iter().copied().enumerate() {
                for (vc, col_idx) in visible_cols.iter().copied().enumerate() {
                    if let Some(proc_addr) = state.load_data_proc.get(col_idx).copied().filter(|v| *v != 0) {
                        Self::invoke_load_data_proc(
                            cpu,
                            runtime,
                            memory,
                            form_id,
                            table_id,
                            table_ptr,
                            row_idx as u16,
                            col_idx as u16,
                            proc_addr,
                        );
                    }
                    if let Some(proc_addr) = state.custom_draw_proc.get(col_idx).copied().filter(|v| *v != 0) {
                        let bounds = (
                            (table_x + 1 + (vc as i32 * cell_w)).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                            (table_y + 1 + (vr as i32 * cell_h)).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                            cell_w.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                            cell_h.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                        );
                        Self::invoke_custom_draw_proc(
                            cpu,
                            runtime,
                            memory,
                            form_id,
                            table_id,
                            table_ptr,
                            row_idx as u16,
                            col_idx as u16,
                            proc_addr,
                            bounds,
                        );
                    }
                }
            }
        }
        cpu.d[0] = 0;
    }

    fn tbl_handle_event(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let Some((form_id, table_id, table_ptr)) = Self::decode_table_object(runtime, cpu, memory) else {
            cpu.d[0] = 0;
            return;
        };
        let sp = cpu.a[7];
        let event_p = Self::stack_u32(memory, sp, 4, cpu.a[1].max(cpu.d[1]));
        if event_p == 0 || !memory.contains_addr(event_p) {
            cpu.d[0] = 0;
            return;
        }
        let evt_type = memory.read_u16_be(event_p).unwrap_or(0xFFFF);
        if evt_type != crate::palm::runtime::EVT_PEN_DOWN {
            cpu.d[0] = 0;
            return;
        }
        let pen_x = memory.read_u16_be(event_p.saturating_add(8)).unwrap_or(0) as i32;
        let pen_y = memory.read_u16_be(event_p.saturating_add(10)).unwrap_or(0) as i32;
        let (table_x, table_y, table_w, table_h) = Self::table_object_rect(runtime, form_id, table_id);
        let table_x = table_x as i32;
        let table_y = table_y as i32;
        let table_w = table_w as i32;
        let table_h = table_h as i32;
        let state_snapshot = Self::table_state_ref(runtime, form_id, table_id).cloned();
        let Some(state) = state_snapshot else {
            cpu.d[0] = 0;
            return;
        };
        if pen_x < table_x || pen_x >= table_x + table_w || pen_y < table_y || pen_y >= table_y + table_h {
            cpu.d[0] = 0;
            return;
        }
        let visible_rows: alloc::vec::Vec<usize> = (0..state.rows.max(1) as usize)
            .filter(|r| state.row_usable.get(*r).copied().unwrap_or(true))
            .collect();
        let visible_cols: alloc::vec::Vec<usize> = (0..state.cols.max(1) as usize)
            .filter(|c| state.col_usable.get(*c).copied().unwrap_or(true))
            .collect();
        if visible_rows.is_empty() || visible_cols.is_empty() {
            cpu.d[0] = 0;
            return;
        }
        let row_count = visible_rows.len().max(1);
        let col_count = visible_cols.len().max(1);
        let cell_h = ((table_h - 2).max(1) / row_count as i32).max(1);
        let cell_w = ((table_w - 2).max(1) / col_count as i32).max(1);
        let local_x = (pen_x - table_x - 1).max(0);
        let local_y = (pen_y - table_y - 1).max(0);
        let vis_row = (local_y / cell_h.max(1)).clamp(0, row_count.saturating_sub(1) as i32) as usize;
        let vis_col = (local_x / cell_w.max(1)).clamp(0, col_count.saturating_sub(1) as i32) as usize;
        let row = visible_rows[vis_row] as i16;
        let col = visible_cols[vis_col] as i16;
        let selectable = state
            .row_selectable
            .get(row as usize)
            .copied()
            .unwrap_or(true);
        if !selectable {
            cpu.d[0] = 0;
            return;
        }
        Self::with_state_mut(cpu, runtime, memory, |state| {
            Self::ensure_row_count(state, row as usize + 1);
            Self::ensure_col_count(state, col as usize + 1);
            state.selected_row = row;
            state.selected_col = col;
        });
        log::info!(
            "Palm TblHandleEvent form_id={} table_id={} pen=({}, {}) -> row={} col={}",
            form_id,
            table_id,
            pen_x,
            pen_y,
            row,
            col
        );
        let _ = table_ptr;
        cpu.d[0] = 1;
    }

    fn tbl_set_row_usable(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let usable = Self::stack_u16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16) != 0;
        if row >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let row_u = row as usize;
                Self::ensure_row_count(state, row_u + 1);
                state.row_usable[row_u] = usable;
            });
            log::info!("Palm TblSetRowUsable row={} usable={}", row, usable);
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_row_selectable(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let selectable = Self::stack_u16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16) != 0;
        if row >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let row_u = row as usize;
                Self::ensure_row_count(state, row_u + 1);
                state.row_selectable[row_u] = selectable;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_column_usable(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let usable = Self::stack_u16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16) != 0;
        if col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let col_u = col as usize;
                Self::ensure_col_count(state, col_u + 1);
                state.col_usable[col_u] = usable;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_column_spacing(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let spacing = Self::stack_i16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16 as i16);
        if col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let col_u = col as usize;
                Self::ensure_col_count(state, col_u + 1);
                state.col_spacing[col_u] = spacing.max(0);
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_column_width(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let width = Self::stack_i16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16 as i16);
        if col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let col_u = col as usize;
                Self::ensure_col_count(state, col_u + 1);
                state.col_width[col_u] = width.max(1);
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_row_height(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let height = Self::stack_i16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16 as i16);
        if row >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let row_u = row as usize;
                Self::ensure_row_count(state, row_u + 1);
                state.row_height[row_u] = height.max(1);
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_row_id(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let id = Self::stack_u16(memory, sp, 6, (cpu.d[2] & 0xFFFF) as u16);
        if row >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let row_u = row as usize;
                Self::ensure_row_count(state, row_u + 1);
                state.row_id[row_u] = id;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_row_data(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let data = Self::stack_u32(memory, sp, 6, cpu.a[0].max(cpu.d[2]));
        let table_meta = Self::decode_table_object(runtime, cpu, memory);
        if row >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let row_u = row as usize;
                Self::ensure_row_count(state, row_u + 1);
                state.row_data[row_u] = data;
            });
            log::info!(
                "Palm TblSetRowData form_id={:?} table_id={:?} row={} data=0x{data:08X}",
                table_meta.map(|(form_id, _, _)| form_id),
                table_meta.map(|(_, table_id, _)| table_id),
                row
            );
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_custom_draw_proc(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let draw_cb = Self::stack_u32(memory, sp, 6, cpu.a[0].max(cpu.d[0]));
        if col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let col_u = col as usize;
                Self::ensure_col_count(state, col_u + 1);
                state.custom_draw_proc[col_u] = draw_cb;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_load_data_proc(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let cb = Self::stack_u32(memory, sp, 6, cpu.a[0].max(cpu.d[0]));
        if col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let col_u = col as usize;
                Self::ensure_col_count(state, col_u + 1);
                state.load_data_proc[col_u] = cb;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_save_data_proc(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let cb = Self::stack_u32(memory, sp, 6, cpu.a[0].max(cpu.d[0]));
        if col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let col_u = col as usize;
                Self::ensure_col_count(state, col_u + 1);
                state.save_data_proc[col_u] = cb;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_item_int(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let table_meta = Self::decode_table_object(runtime, cpu, memory);
        let (row, col) = Self::decode_row_col(cpu, memory);
        let sp = cpu.a[7];
        let value = Self::stack_i16(memory, sp, 8, (cpu.d[3] & 0xFFFF) as u16 as i16);
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let cell = Self::ensure_cell_mut(state, row as u16, col as u16);
                cell.int_value = value;
            });
            log::info!(
                "Palm TblSetItemInt form_id={:?} table_id={:?} row={} col={} value={}",
                table_meta.map(|(form_id, _, _)| form_id),
                table_meta.map(|(_, table_id, _)| table_id),
                row,
                col,
                value
            );
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_item_ptr(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let table_meta = Self::decode_table_object(runtime, cpu, memory);
        let (row, col) = Self::decode_row_col(cpu, memory);
        let sp = cpu.a[7];
        let ptr = Self::stack_u32(memory, sp, 8, cpu.a[1].max(cpu.d[3]));
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let cell = Self::ensure_cell_mut(state, row as u16, col as u16);
                cell.ptr_value = ptr;
            });
            log::info!(
                "Palm TblSetItemPtr form_id={:?} table_id={:?} row={} col={} ptr=0x{ptr:08X}",
                table_meta.map(|(form_id, _, _)| form_id),
                table_meta.map(|(_, table_id, _)| table_id),
                row,
                col
            );
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_item_style(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let (row, col) = Self::decode_row_col(cpu, memory);
        let sp = cpu.a[7];
        let style = Self::stack_u16(memory, sp, 8, (cpu.d[3] & 0xFFFF) as u16);
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let cell = Self::ensure_cell_mut(state, row as u16, col as u16);
                cell.style = style;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_item_font(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let (row, col) = Self::decode_row_col(cpu, memory);
        let sp = cpu.a[7];
        let font_id = Self::stack_u16(memory, sp, 8, (cpu.d[3] & 0xFFFF) as u16);
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let cell = Self::ensure_cell_mut(state, row as u16, col as u16);
                cell.font_id = font_id;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_get_item_int(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) else {
            cpu.d[0] = 0;
            return;
        };
        let (row, col) = Self::decode_row_col(cpu, memory);
        let mut out = 0i16;
        if row >= 0 && col >= 0 {
            if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                if let Some(cell) = Self::cell_ref(state, row as u16, col as u16) {
                    out = cell.int_value;
                }
            }
        }
        log::info!(
            "Palm TblGetItemInt form_id={} table_id={} row={} col={} -> {}",
            form_id,
            table_id,
            row,
            col,
            out
        );
        cpu.d[0] = out as i32 as u32;
    }

    fn tbl_get_item_ptr(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            return;
        };
        let (row, col) = Self::decode_row_col(cpu, memory);
        let mut out = 0u32;
        if row >= 0 && col >= 0 {
            if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                if let Some(cell) = Self::cell_ref(state, row as u16, col as u16) {
                    out = cell.ptr_value;
                }
            }
        }
        cpu.a[0] = out;
        cpu.d[0] = out;
    }

    fn tbl_set_selection(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let (row, col) = Self::decode_row_col(cpu, memory);
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                Self::ensure_row_count(state, row as usize + 1);
                Self::ensure_col_count(state, col as usize + 1);
                state.selected_row = row;
                state.selected_col = col;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_select_item(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        Self::tbl_set_selection(cpu, runtime, memory);
    }

    fn tbl_get_selection(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) else {
            cpu.d[0] = 0;
            return;
        };
        let sp = cpu.a[7];
        let row_p = Self::stack_u32(memory, sp, 4, cpu.a[0]);
        let col_p = Self::stack_u32(memory, sp, 8, cpu.a[1]);
        let mut selected = false;
        let mut row = -1i16;
        let mut col = -1i16;
        if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
            row = state.selected_row;
            col = state.selected_col;
            selected = row >= 0 && col >= 0;
        }
        if row_p != 0 && memory.contains_addr(row_p) {
            let _ = memory.write_u16_be(row_p, row as u16);
        }
        if col_p != 0 && memory.contains_addr(col_p) {
            let _ = memory.write_u16_be(col_p, col as u16);
        }
        cpu.d[0] = if selected { 1 } else { 0 };
    }

    fn tbl_get_number_of_rows(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let mut rows = 0u16;
        if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
            if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                rows = state.rows.max(state.row_usable.len() as u16);
            }
        }
        cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | rows as u32;
    }

    fn tbl_get_number_of_columns(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let mut cols = 0u16;
        if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
            if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                cols = state.cols.max(state.col_usable.len() as u16);
            }
        }
        cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | cols as u32;
    }

    fn tbl_get_last_usable_row(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let mut out = -1i16;
        if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
            if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                for (idx, usable) in state.row_usable.iter().enumerate() {
                    if *usable {
                        out = idx as i16;
                    }
                }
            }
        }
        cpu.d[0] = out as i32 as u32;
    }

    fn tbl_get_row_height(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let mut out = 11i16;
        if row >= 0 {
            if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
                if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                    if let Some(h) = state.row_height.get(row as usize) {
                        out = *h;
                    }
                }
            }
        }
        cpu.d[0] = out as i32 as u32;
    }

    fn tbl_get_column_width(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let mut out = 28i16;
        if col >= 0 {
            if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
                if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                    if let Some(w) = state.col_width.get(col as usize) {
                        out = *w;
                    }
                }
            }
        }
        cpu.d[0] = out as i32 as u32;
    }

    fn tbl_get_column_spacing(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &MemoryMap,
    ) {
        let sp = cpu.a[7];
        let col = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let mut out = 1i16;
        if col >= 0 {
            if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
                if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                    if let Some(s) = state.col_spacing.get(col as usize) {
                        out = *s;
                    }
                }
            }
        }
        cpu.d[0] = out as i32 as u32;
    }

    fn tbl_get_row_id(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let mut out = 0u16;
        if row >= 0 {
            if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
                if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                    if let Some(id) = state.row_id.get(row as usize) {
                        out = *id;
                    } else {
                        out = row as u16;
                    }
                }
            }
        }
        cpu.d[0] = (cpu.d[0] & 0xFFFF_0000) | out as u32;
    }

    fn tbl_get_row_data(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let sp = cpu.a[7];
        let row = Self::stack_i16(memory, sp, 4, (cpu.d[1] & 0xFFFF) as u16 as i16);
        let mut out = 0u32;
        let mut table_meta = None;
        if row >= 0 {
            if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
                table_meta = Some((form_id, table_id));
                if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                    if let Some(v) = state.row_data.get(row as usize) {
                        out = *v;
                    }
                }
            }
        }
        log::info!(
            "Palm TblGetRowData form_id={:?} table_id={:?} row={} -> 0x{out:08X}",
            table_meta.map(|(form_id, _)| form_id),
            table_meta.map(|(_, table_id)| table_id),
            row
        );
        cpu.d[0] = out;
    }
}
