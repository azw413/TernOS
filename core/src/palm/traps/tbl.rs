extern crate alloc;

use crate::palm::{
    cpu::{core::CpuState68k, memory::MemoryMap},
    runtime::{PrcRuntimeContext, RuntimeFormObjectKind, RuntimeTableCellState, RuntimeTableState},
};

pub struct TblApi;

impl TblApi {
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
                // TblHandleEvent(tableP, eventP): not modeled yet.
                cpu.d[0] = 0;
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

    fn tbl_draw_table(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        Self::with_state_mut(cpu, runtime, memory, |state| {
            state.drawn = true;
            let rows = if state.rows > 0 { state.rows as usize } else { 6 };
            let cols = if state.cols > 0 { state.cols as usize } else { 3 };
            Self::ensure_row_count(state, rows);
            Self::ensure_col_count(state, cols);
        });
        cpu.d[0] = 0;
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
        if row >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let row_u = row as usize;
                Self::ensure_row_count(state, row_u + 1);
                state.row_data[row_u] = data;
            });
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
        let (row, col) = Self::decode_row_col(cpu, memory);
        let sp = cpu.a[7];
        let value = Self::stack_i16(memory, sp, 8, (cpu.d[3] & 0xFFFF) as u16 as i16);
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let cell = Self::ensure_cell_mut(state, row as u16, col as u16);
                cell.int_value = value;
            });
        }
        cpu.d[0] = 0;
    }

    fn tbl_set_item_ptr(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &MemoryMap) {
        let (row, col) = Self::decode_row_col(cpu, memory);
        let sp = cpu.a[7];
        let ptr = Self::stack_u32(memory, sp, 8, cpu.a[1].max(cpu.d[3]));
        if row >= 0 && col >= 0 {
            Self::with_state_mut(cpu, runtime, memory, |state| {
                let cell = Self::ensure_cell_mut(state, row as u16, col as u16);
                cell.ptr_value = ptr;
            });
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
        if row >= 0 {
            if let Some((form_id, table_id, _)) = Self::decode_table_object(runtime, cpu, memory) {
                if let Some(state) = Self::table_state_ref(runtime, form_id, table_id) {
                    if let Some(v) = state.row_data.get(row as usize) {
                        out = *v;
                    }
                }
            }
        }
        cpu.d[0] = out;
    }
}
