extern crate alloc;

use alloc::{format, string::String};

use crate::palm::{
    cpu::{core::CpuState68k, memory::MemoryMap},
    runtime::{PrcRuntimeContext, RuntimeButtonLabel},
};

pub struct CategoryApi;

impl CategoryApi {
    pub fn handle_trap(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        trap_word: u16,
    ) -> bool {
        match trap_word {
            0xA104 => {
                Self::category_get_name(cpu, runtime, memory);
                true
            }
            0xA108 => {
                Self::category_set_trigger_label(cpu, runtime, memory);
                true
            }
            _ => false,
        }
    }

    fn read_c_string(memory: &MemoryMap, ptr: u32) -> String {
        if ptr == 0 || !memory.contains_addr(ptr) {
            return String::new();
        }
        let mut out = alloc::vec::Vec::new();
        let mut cur = ptr;
        while let Some(b) = memory.read_u8(cur) {
            if b == 0 {
                break;
            }
            out.push(b);
            cur = cur.saturating_add(1);
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn write_c_string(memory: &mut MemoryMap, ptr: u32, text: &str) {
        if ptr == 0 || !memory.contains_addr(ptr) {
            return;
        }
        let bytes = text.as_bytes();
        for (idx, b) in bytes.iter().take(15).enumerate() {
            let _ = memory.write_u8(ptr.saturating_add(idx as u32), *b);
        }
        let _ = memory.write_u8(ptr.saturating_add(bytes.len().min(15) as u32), 0);
    }

    fn default_category_name(index: u16) -> String {
        match index {
            0 => "Unfiled".into(),
            1 => "Business".into(),
            2 => "Personal".into(),
            3 => "Ideas".into(),
            _ => format!("Category {}", index),
        }
    }

    fn category_get_name(
        cpu: &mut CpuState68k,
        _runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0);
        let name_ptr = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let label = Self::default_category_name(index);
        Self::write_c_string(memory, name_ptr, &label);
    }

    fn category_set_trigger_label(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let ctl_ptr = memory.read_u32_be(sp).unwrap_or(cpu.a[0]);
        let name_ptr = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
        let label = Self::read_c_string(memory, name_ptr);
        if ctl_ptr == 0 || label.is_empty() {
            return;
        }
        let Some(obj) = runtime.form_objects.iter().find(|obj| obj.ptr == ctl_ptr) else {
            return;
        };
        if let Some(existing) = runtime
            .button_labels
            .iter_mut()
            .find(|entry| entry.form_id == obj.form_id && entry.object_id == obj.object_id)
        {
            existing.text = label;
            return;
        }
        runtime.button_labels.push(RuntimeButtonLabel {
            form_id: obj.form_id,
            object_id: obj.object_id,
            text: label,
        });
    }
}
