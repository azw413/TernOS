extern crate alloc;

use alloc::{string::String, vec, vec::Vec};

use crate::prc_app::{
    cpu::{core::CpuState68k, memory::MemoryMap},
    runtime::{MemBlock, PrcRuntimeContext, RuntimeDatabase, RuntimeOpenDatabase},
};

pub struct DmApi;

impl DmApi {
    const DM_ERR_NONE: u16 = 0;
    const DM_ERR_INVALID_PARAM: u16 = 0x8000;
    const DM_ERR_ALREADY_EXISTS: u16 = 0x8001;
    const DM_ERR_CANT_FIND: u16 = 0x8002;

    pub fn handle_trap(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        trap_word: u16,
    ) -> bool {
        match trap_word {
            0xA041 => {
                Self::dm_create_database(cpu, runtime, memory);
                true
            }
            0xA045 => {
                Self::dm_find_database(cpu, runtime, memory);
                true
            }
            0xA046 => {
                Self::dm_database_info(cpu, runtime, memory);
                true
            }
            0xA047 => {
                Self::dm_set_database_info(cpu, runtime, memory);
                true
            }
            0xA049 => {
                Self::dm_open_database(cpu, runtime, memory);
                true
            }
            0xA04A => {
                Self::dm_close_database(cpu, runtime, memory);
                true
            }
            0xA04C => {
                Self::dm_open_database_info(cpu, runtime, memory);
                true
            }
            0xA04E => {
                cpu.d[0] = runtime.dm_last_err as u32;
                true
            }
            0xA04F => {
                Self::dm_num_records(cpu, runtime, memory);
                true
            }
            0xA050 => {
                Self::dm_record_info(cpu, runtime, memory);
                true
            }
            0xA055 => {
                Self::dm_new_record(cpu, runtime, memory);
                true
            }
            0xA05B => {
                Self::dm_query_record(cpu, runtime, memory, false);
                true
            }
            0xA05C => {
                Self::dm_query_record(cpu, runtime, memory, true);
                true
            }
            0xA05D => {
                Self::dm_resize_record(cpu, runtime, memory);
                true
            }
            0xA05E => {
                Self::dm_release_record(cpu, runtime, memory);
                true
            }
            0xA05F => {
                Self::dm_get_resource(cpu, runtime, memory, false);
                true
            }
            0xA060 => {
                Self::dm_get_resource(cpu, runtime, memory, true);
                true
            }
            0xA061 => {
                cpu.d[0] = 0;
                true
            }
            0xA075 => {
                Self::dm_open_database_by_type_creator(cpu, runtime, memory);
                true
            }
            0xA076 => {
                Self::dm_write(cpu, runtime, memory);
                true
            }
            0xA07E => {
                Self::dm_set(cpu, runtime, memory);
                true
            }
            _ => false,
        }
    }

    fn read_c_string(memory: &MemoryMap, ptr: u32) -> String {
        if ptr == 0 || !memory.contains_addr(ptr) {
            return String::new();
        }
        let mut out = Vec::new();
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

    fn write_c_string(memory: &mut MemoryMap, ptr: u32, s: &str) {
        if ptr == 0 || !memory.contains_addr(ptr) {
            return;
        }
        for (i, b) in s.as_bytes().iter().enumerate() {
            let _ = memory.write_u8(ptr.saturating_add(i as u32), *b);
        }
        let _ = memory.write_u8(ptr.saturating_add(s.len() as u32), 0);
    }

    fn alloc_mem(
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        data: Vec<u8>,
        resource_kind: Option<u32>,
        resource_id: Option<u16>,
    ) -> u32 {
        let size = data.len().clamp(16, 1_048_576) as u32;
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
        handle
    }

    fn set_last_err(runtime: &mut PrcRuntimeContext, err: u16) {
        runtime.dm_last_err = err;
    }

    fn current_app_resource_db(runtime: &PrcRuntimeContext) -> Option<&RuntimeDatabase> {
        runtime
            .databases
            .iter()
            .find(|db| db.is_resource_db && db.creator != 0 && db.db_type != 0)
    }

    fn db_by_local_id(runtime: &PrcRuntimeContext, local_id: u32) -> Option<&RuntimeDatabase> {
        runtime.databases.iter().find(|db| db.local_id == local_id)
    }

    fn db_by_name<'a>(runtime: &'a PrcRuntimeContext, name: &str) -> Option<&'a RuntimeDatabase> {
        runtime.databases.iter().find(|db| db.name == name)
    }

    fn db_by_ref(runtime: &PrcRuntimeContext, db_ref: u32) -> Option<&RuntimeDatabase> {
        let local_id = runtime
            .open_databases
            .iter()
            .find(|o| o.db_ref == db_ref)
            .map(|o| o.local_id)?;
        Self::db_by_local_id(runtime, local_id)
    }

    fn open_ref_for_local_id(runtime: &mut PrcRuntimeContext, local_id: u32, mode: u16) -> u32 {
        if let Some(existing) = runtime
            .open_databases
            .iter_mut()
            .find(|o| o.local_id == local_id)
        {
            existing.mode = mode;
            return existing.db_ref;
        }
        let db_ref = runtime.next_db_ref;
        runtime.next_db_ref = runtime.next_db_ref.saturating_add(1);
        runtime.open_databases.push(RuntimeOpenDatabase {
            db_ref,
            local_id,
            mode,
        });
        db_ref
    }

    fn create_database(
        runtime: &mut PrcRuntimeContext,
        name: String,
        db_type: u32,
        creator: u32,
        is_resource_db: bool,
    ) -> u32 {
        let local_id = runtime.next_local_id;
        runtime.next_local_id = runtime.next_local_id.saturating_add(1);
        runtime.databases.push(RuntimeDatabase {
            local_id,
            card_no: 0,
            name,
            creator,
            db_type,
            is_resource_db,
            version: 1,
            attributes: 0,
            mod_number: 0,
            app_info_id: 0,
            sort_info_id: 0,
            record_handles: Vec::new(),
        });
        local_id
    }

    fn db_by_local_id_mut(
        runtime: &mut PrcRuntimeContext,
        local_id: u32,
    ) -> Option<&mut RuntimeDatabase> {
        runtime.databases.iter_mut().find(|db| db.local_id == local_id)
    }

    fn db_by_ref_mut(runtime: &mut PrcRuntimeContext, db_ref: u32) -> Option<&mut RuntimeDatabase> {
        let local_id = runtime
            .open_databases
            .iter()
            .find(|o| o.db_ref == db_ref)
            .map(|o| o.local_id)?;
        Self::db_by_local_id_mut(runtime, local_id)
    }

    fn handle_from_any(runtime: &PrcRuntimeContext, raw: u32) -> Option<u32> {
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

    fn fourcc_name(db_type: u32) -> String {
        let bytes = db_type.to_be_bytes();
        if bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            alloc::format!("DB{:08X}", db_type)
        }
    }

    fn resource_entries_for_db<'a>(
        runtime: &'a PrcRuntimeContext,
        _db: &RuntimeDatabase,
    ) -> Vec<&'a crate::prc_app::runtime::ResourceBlob> {
        // Runtime currently keeps app + system resources in a shared table.
        // Until per-DB resource partitioning lands, always resolve from that table.
        runtime.resources.iter().collect()
    }

    fn find_resource_handle(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        db: Option<&RuntimeDatabase>,
        kind: u32,
        id: u16,
    ) {
        let mut found = false;
        if let Some(db) = db {
            for res in Self::resource_entries_for_db(runtime, db) {
                if res.kind == kind && res.id == id {
                    let handle = if let Some(existing) = runtime
                        .mem_blocks
                        .iter()
                        .find(|b| b.resource_kind == Some(res.kind) && b.resource_id == Some(res.id))
                    {
                        existing.handle
                    } else {
                        Self::alloc_mem(runtime, memory, res.data.clone(), Some(res.kind), Some(res.id))
                    };
                    cpu.a[0] = handle;
                    cpu.d[0] = handle;
                    found = true;
                    break;
                }
            }
        } else {
            for res in &runtime.resources {
                if res.kind == kind && res.id == id {
                    let handle = if let Some(existing) = runtime
                        .mem_blocks
                        .iter()
                        .find(|b| b.resource_kind == Some(res.kind) && b.resource_id == Some(res.id))
                    {
                        existing.handle
                    } else {
                        Self::alloc_mem(runtime, memory, res.data.clone(), Some(res.kind), Some(res.id))
                    };
                    cpu.a[0] = handle;
                    cpu.d[0] = handle;
                    found = true;
                    break;
                }
            }
        }
        if found {
            return;
        }

        // Defensive fallback: if DB resolution selected a record DB path by mistake,
        // still allow resource lookup from the global PRC resource table.
        for res in &runtime.resources {
            if res.kind == kind && res.id == id {
                let handle = if let Some(existing) = runtime
                    .mem_blocks
                    .iter()
                    .find(|b| b.resource_kind == Some(res.kind) && b.resource_id == Some(res.id))
                {
                    existing.handle
                } else {
                    Self::alloc_mem(runtime, memory, res.data.clone(), Some(res.kind), Some(res.id))
                };
                cpu.a[0] = handle;
                cpu.d[0] = handle;
                return;
            }
        }
        cpu.a[0] = 0;
    }

    fn dm_get_resource(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        only_top: bool,
    ) {
        runtime.dm_get_resource_probe_count = runtime.dm_get_resource_probe_count.saturating_add(1);
        let sp = cpu.a[7];
        let kind = memory.read_u32_be(sp).unwrap_or(0);
        let id = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0);

        if only_top {
            let db = runtime
                .open_databases
                .last()
                .and_then(|open| Self::db_by_local_id(runtime, open.local_id))
                .filter(|db| db.is_resource_db)
                .cloned()
                .or_else(|| {
                    runtime
                        .open_databases
                        .iter()
                        .rev()
                        .filter_map(|open| Self::db_by_local_id(runtime, open.local_id))
                        .find(|db| db.is_resource_db)
                        .cloned()
                })
                .or_else(|| Self::current_app_resource_db(runtime).cloned());
            Self::find_resource_handle(cpu, runtime, memory, db.as_ref(), kind, id);
        } else {
            let candidates: Vec<RuntimeDatabase> = runtime
                .open_databases
                .iter()
                .rev()
                .filter_map(|open| Self::db_by_local_id(runtime, open.local_id).cloned())
                .collect();
            for db in &candidates {
                Self::find_resource_handle(cpu, runtime, memory, Some(db), kind, id);
                if cpu.a[0] != 0 {
                    break;
                }
            }
            if cpu.a[0] == 0 {
                let db = Self::current_app_resource_db(runtime).cloned();
                Self::find_resource_handle(cpu, runtime, memory, db.as_ref(), kind, id);
            }
        }
        if cpu.a[0] == 0 {
            Self::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
        } else {
            Self::set_last_err(runtime, Self::DM_ERR_NONE);
        }
        if runtime.trace_traps && runtime.trace_trap_budget > 0 {
            let k = kind.to_be_bytes();
            if cpu.a[0] == 0 {
                let tuple = (kind, id, 0, 0);
                if runtime.dm_get_resource_last_log != Some(tuple) {
                    runtime.dm_get_resource_last_log = Some(tuple);
                    let tstr = u32::from_be_bytes(*b"tSTR");
                    if kind == tstr {
                        let mut ids: Vec<u16> = runtime
                            .resources
                            .iter()
                            .filter(|res| res.kind == tstr)
                            .map(|res| res.id)
                            .collect();
                        ids.sort_unstable();
                        ids.dedup();
                        let sample_count = ids.len().min(12);
                        let mut sample = String::new();
                        for (idx, rid) in ids.iter().take(sample_count).enumerate() {
                            if idx > 0 {
                                sample.push_str(",");
                            }
                            let _ = core::fmt::Write::write_fmt(&mut sample, format_args!("{}", rid));
                        }
                        log::info!(
                            "PRC trap detail DmGetResource tSTR ids available={} sample=[{}]",
                            ids.len(),
                            sample
                        );
                    }
                }
                log::info!(
                    "PRC trap detail DmGetResource req='{}{}{}{}'/{} -> null",
                    k[0] as char,
                    k[1] as char,
                    k[2] as char,
                    k[3] as char,
                    id
                );
            } else {
                runtime.dm_get_resource_last_log = Some((kind, id, kind, id));
                log::info!(
                    "PRC trap detail DmGetResource req='{}{}{}{}'/{} -> got='{}{}{}{}'/{} handle=0x{:08X}",
                    k[0] as char,
                    k[1] as char,
                    k[2] as char,
                    k[3] as char,
                    id,
                    k[0] as char,
                    k[1] as char,
                    k[2] as char,
                    k[3] as char,
                    id,
                    cpu.a[0]
                );
            }
        }
    }

    fn dm_create_database(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        let name_p = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let creator = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let db_type = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
        let res_db = memory.read_u16_be(sp.saturating_add(14)).unwrap_or(0) != 0;
        let name = Self::read_c_string(memory, name_p);

        if name.is_empty() {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        if Self::db_by_name(runtime, &name).is_some() {
            cpu.d[0] = Self::DM_ERR_ALREADY_EXISTS as u32;
            Self::set_last_err(runtime, Self::DM_ERR_ALREADY_EXISTS);
            return;
        }

        Self::create_database(runtime, name, db_type, creator, res_db);
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_find_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        let name_p = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let name = Self::read_c_string(memory, name_p);
        let local_id = Self::db_by_name(runtime, &name).map(|db| db.local_id).unwrap_or(0);
        cpu.d[0] = local_id;
        cpu.a[0] = local_id;
        if local_id == 0 {
            Self::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
        } else {
            Self::set_last_err(runtime, Self::DM_ERR_NONE);
        }
    }

    fn dm_database_info(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let local_id = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let Some(db) = Self::db_by_local_id(runtime, local_id).cloned() else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
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

        Self::write_c_string(memory, name_p, &db.name);
        if attrs_p != 0 && memory.contains_addr(attrs_p) {
            let _ = memory.write_u16_be(attrs_p, db.attributes);
        }
        if vers_p != 0 && memory.contains_addr(vers_p) {
            let _ = memory.write_u16_be(vers_p, db.version);
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
            let _ = memory.write_u32_be(modn_p, db.mod_number);
        }
        if appi_p != 0 && memory.contains_addr(appi_p) {
            let _ = memory.write_u32_be(appi_p, db.app_info_id);
        }
        if sorti_p != 0 && memory.contains_addr(sorti_p) {
            let _ = memory.write_u32_be(sorti_p, db.sort_info_id);
        }
        if type_p != 0 && memory.contains_addr(type_p) {
            let _ = memory.write_u32_be(type_p, db.db_type);
        }
        if creator_p != 0 && memory.contains_addr(creator_p) {
            let _ = memory.write_u32_be(creator_p, db.creator);
        }
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_open_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let local_id = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let mode = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0);
        if Self::db_by_local_id(runtime, local_id).is_none() {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        }
        let db_ref = Self::open_ref_for_local_id(runtime, local_id, mode);
        cpu.a[0] = db_ref;
        cpu.d[0] = db_ref;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_close_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(cpu.a[0]);
        if let Some(pos) = runtime.open_databases.iter().position(|o| o.db_ref == db_ref) {
            runtime.open_databases.remove(pos);
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_NONE);
        } else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
        }
    }

    fn dm_open_database_info(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(cpu.a[0]);
        let Some(open) = runtime.open_databases.iter().find(|o| o.db_ref == db_ref).cloned() else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Some(db) = Self::db_by_local_id(runtime, open.local_id).cloned() else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let local_id_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
        let open_count_p = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
        let mode_p = memory.read_u32_be(sp.saturating_add(12)).unwrap_or(0);
        let card_no_p = memory.read_u32_be(sp.saturating_add(16)).unwrap_or(0);
        let res_db_p = memory.read_u32_be(sp.saturating_add(20)).unwrap_or(0);

        if local_id_p != 0 && memory.contains_addr(local_id_p) {
            let _ = memory.write_u32_be(local_id_p, db.local_id);
        }
        if open_count_p != 0 && memory.contains_addr(open_count_p) {
            let count = runtime
                .open_databases
                .iter()
                .filter(|o| o.local_id == db.local_id)
                .count() as u16;
            let _ = memory.write_u16_be(open_count_p, count.max(1));
        }
        if mode_p != 0 && memory.contains_addr(mode_p) {
            let _ = memory.write_u16_be(mode_p, open.mode);
        }
        if card_no_p != 0 && memory.contains_addr(card_no_p) {
            let _ = memory.write_u16_be(card_no_p, db.card_no);
        }
        if res_db_p != 0 && memory.contains_addr(res_db_p) {
            let _ = memory.write_u8(res_db_p, if db.is_resource_db { 1 } else { 0 });
        }
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_open_database_by_type_creator(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let db_type = memory.read_u32_be(sp).unwrap_or(cpu.d[0]);
        let creator = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(cpu.d[1]);
        let mode = memory.read_u16_be(sp.saturating_add(8)).unwrap_or(0);
        let Some(local_id) = runtime
            .databases
            .iter()
            .find(|db| db.db_type == db_type && db.creator == creator)
            .map(|db| db.local_id)
        else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        };
        let db_ref = Self::open_ref_for_local_id(runtime, local_id, mode);
        cpu.a[0] = db_ref;
        cpu.d[0] = db_ref;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_set_database_info(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let local_id = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let Some(db) = Self::db_by_local_id_mut(runtime, local_id) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let name_p = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let attrs_p = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
        let vers_p = memory.read_u32_be(sp.saturating_add(14)).unwrap_or(0);
        let modn_p = memory.read_u32_be(sp.saturating_add(30)).unwrap_or(0);
        let appi_p = memory.read_u32_be(sp.saturating_add(34)).unwrap_or(0);
        let sorti_p = memory.read_u32_be(sp.saturating_add(38)).unwrap_or(0);
        let type_p = memory.read_u32_be(sp.saturating_add(42)).unwrap_or(0);
        let creator_p = memory.read_u32_be(sp.saturating_add(46)).unwrap_or(0);

        if name_p != 0 {
            let name = Self::read_c_string(memory, name_p);
            if !name.is_empty() {
                db.name = name;
            }
        }
        if attrs_p != 0 && memory.contains_addr(attrs_p) {
            if let Some(v) = memory.read_u16_be(attrs_p) {
                db.attributes = v;
            }
        }
        if vers_p != 0 && memory.contains_addr(vers_p) {
            if let Some(v) = memory.read_u16_be(vers_p) {
                db.version = v;
            }
        }
        if modn_p != 0 && memory.contains_addr(modn_p) {
            if let Some(v) = memory.read_u32_be(modn_p) {
                db.mod_number = v;
            }
        }
        if appi_p != 0 && memory.contains_addr(appi_p) {
            if let Some(v) = memory.read_u32_be(appi_p) {
                db.app_info_id = v;
            }
        }
        if sorti_p != 0 && memory.contains_addr(sorti_p) {
            if let Some(v) = memory.read_u32_be(sorti_p) {
                db.sort_info_id = v;
            }
        }
        if type_p != 0 && memory.contains_addr(type_p) {
            if let Some(v) = memory.read_u32_be(type_p) {
                db.db_type = v;
            }
        }
        if creator_p != 0 && memory.contains_addr(creator_p) {
            if let Some(v) = memory.read_u32_be(creator_p) {
                db.creator = v;
            }
        }
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_new_record(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let at_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
        let size = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0) as usize;
        let insert_at = memory
            .read_u16_be(at_p)
            .map(|v| v as usize)
            .unwrap_or(usize::MAX);

        let handle = Self::alloc_mem(runtime, memory, vec![0u8; size], None, None);
        let Some(db) = Self::db_by_ref_mut(runtime, db_ref) else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let index = insert_at.min(db.record_handles.len());
        db.record_handles.insert(index, handle);
        if at_p != 0 && memory.contains_addr(at_p) {
            let _ = memory.write_u16_be(at_p, index as u16);
        }
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_num_records(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let db = Self::db_by_ref(runtime, stack_db_ref).or_else(|| {
            runtime
                .open_databases
                .last()
                .and_then(|open| Self::db_by_local_id(runtime, open.local_id))
        });
        let Some(db) = db else {
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        cpu.d[0] = db.record_handles.len() as u32;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_record_info(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let attr_p = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let unique_id_p = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
        let chunk_id_p = memory.read_u32_be(sp.saturating_add(14)).unwrap_or(0);
        let db = Self::db_by_ref(runtime, stack_db_ref).or_else(|| {
            runtime
                .open_databases
                .last()
                .and_then(|open| Self::db_by_local_id(runtime, open.local_id))
        });
        let Some(db) = db else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if index >= db.record_handles.len() {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        let handle = db.record_handles[index];
        if attr_p != 0 && memory.contains_addr(attr_p) {
            let _ = memory.write_u8(attr_p, 0);
        }
        if unique_id_p != 0 && memory.contains_addr(unique_id_p) {
            let _ = memory.write_u32_be(unique_id_p, (index as u32).saturating_add(1));
        }
        if chunk_id_p != 0 && memory.contains_addr(chunk_id_p) {
            let _ = memory.write_u32_be(chunk_id_p, handle);
        }
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_query_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        lock_record: bool,
    ) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let db = Self::db_by_ref(runtime, stack_db_ref).or_else(|| {
            runtime
                .open_databases
                .last()
                .and_then(|open| Self::db_by_local_id(runtime, open.local_id))
        });
        let Some(db) = db else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Some(handle) = db.record_handles.get(index).copied() else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if lock_record
            && let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.handle == handle)
        {
            block.locked = true;
            memory.upsert_overlay(block.ptr, block.data.clone());
        }
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_resize_record(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let mut new_size = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0) as usize;

        let mut handle = None;
        if let Some(db) = Self::db_by_ref(runtime, stack_db_ref) {
            handle = db.record_handles.get(index).copied();
        }
        if handle.is_none() {
            // Some glue paths pass stale/zero stack args; fall back to register candidates.
            for raw in [cpu.a[0], cpu.d[3], cpu.d[0], cpu.a[1], cpu.d[1]] {
                if let Some(h) = Self::handle_from_any(runtime, raw) {
                    handle = Some(h);
                    break;
                }
            }
        }
        if handle.is_none() {
            // Last resort: use latest open DB + requested index.
            if let Some(open) = runtime.open_databases.last()
                && let Some(db) = Self::db_by_local_id(runtime, open.local_id)
            {
                handle = db.record_handles.get(index).copied();
            }
        }
        let Some(handle) = handle else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.handle == handle) else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if new_size == 0 {
            // Be permissive for older glue paths that accidentally pass 0.
            new_size = block.data.len();
        }
        block.data.resize(new_size, 0);
        block.size = block.data.len() as u32;
        memory.upsert_overlay(block.ptr, block.data.clone());
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_release_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let dirty = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0) != 0;
        let db_ref = if Self::db_by_ref(runtime, stack_db_ref).is_some() {
            stack_db_ref
        } else {
            runtime.open_databases.last().map(|o| o.db_ref).unwrap_or(0)
        };
        let Some(db) = Self::db_by_ref_mut(runtime, db_ref) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if index >= db.record_handles.len() {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        if dirty {
            db.mod_number = db.mod_number.saturating_add(1);
        }
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_set(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let record_p = memory.read_u32_be(sp).unwrap_or(0);
        let offset = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let bytes = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0) as usize;
        let value = memory.read_u16_be(sp.saturating_add(12)).unwrap_or(0) as u8;
        let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.ptr == record_p) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if offset.saturating_add(bytes) > block.data.len() {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        for i in offset..offset + bytes {
            block.data[i] = value;
        }
        memory.upsert_overlay(block.ptr, block.data.clone());
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_write(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let record_p = memory.read_u32_be(sp).unwrap_or(0);
        let offset = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let src_p = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
        let bytes = memory.read_u32_be(sp.saturating_add(12)).unwrap_or(0) as usize;
        let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.ptr == record_p) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if offset.saturating_add(bytes) > block.data.len() {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        let mut ok = true;
        for i in 0..bytes {
            let Some(b) = memory.read_u8(src_p.saturating_add(i as u32)) else {
                ok = false;
                break;
            };
            block.data[offset + i] = b;
        }
        if !ok {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            Self::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        memory.upsert_overlay(block.ptr, block.data.clone());
        cpu.d[0] = 0;
        Self::set_last_err(runtime, Self::DM_ERR_NONE);
    }
}
