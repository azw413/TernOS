extern crate alloc;

use alloc::{string::String, vec, vec::Vec};

use crate::palm::{
    cpu::{core::CpuState68k, memory::MemoryMap},
    runtime::{PrcRuntimeContext, RuntimeDatabase},
};
use crate::ternos::services::db::runtime as db_runtime;

pub struct DmApi;

impl DmApi {
    const DM_ERR_NONE: u16 = 0;
    const DM_ERR_INVALID_PARAM: u16 = 0x8000;
    const DM_ERR_ALREADY_EXISTS: u16 = 0x8001;
    const DM_ERR_CANT_FIND: u16 = 0x8002;
    const DM_ALL_CATEGORIES: u16 = 0x00FF;
    const DM_UNFILED_CATEGORY: u16 = 0x0000;

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
            0xA042 => {
                Self::dm_delete_database(cpu, runtime, memory);
                true
            }
            0xA044 => {
                Self::dm_get_database(cpu, runtime, memory);
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
            0xA048 => {
                Self::dm_num_databases(cpu, runtime, memory);
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
            0xA051 => {
                Self::dm_set_record_info(cpu, runtime, memory);
                true
            }
            0xA052 => {
                Self::dm_attach_record(cpu, runtime, memory);
                true
            }
            0xA053 => {
                Self::dm_detach_record(cpu, runtime, memory);
                true
            }
            0xA055 => {
                Self::dm_new_record(cpu, runtime, memory);
                true
            }
            0xA059 => {
                Self::dm_new_handle(cpu, runtime, memory);
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
            0xA070 => {
                Self::dm_query_next_in_category(cpu, runtime, memory);
                true
            }
            0xA071 => {
                Self::dm_num_records_in_category(cpu, runtime, memory);
                true
            }
            0xA072 => {
                Self::dm_position_in_category(cpu, runtime, memory);
                true
            }
            0xA073 => {
                Self::dm_seek_record_in_category(cpu, runtime, memory);
                true
            }
            0xA074 => {
                Self::dm_get_next_database_by_type_creator(cpu, runtime, memory);
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

    fn resolve_open_db_ref(
        cpu: &CpuState68k,
        runtime: &PrcRuntimeContext,
        memory: &MemoryMap,
    ) -> Option<u32> {
        let sp = cpu.a[7];
        for raw in [
            memory.read_u32_be(sp).unwrap_or(0),
            cpu.a[0],
            cpu.d[0],
            runtime.open_databases.last().map(|open| open.db_ref).unwrap_or(0),
        ] {
            if raw != 0 && runtime.open_databases.iter().any(|open| open.db_ref == raw) {
                return Some(raw);
            }
        }
        None
    }

    fn resolve_record_db_ref(
        cpu: &CpuState68k,
        runtime: &PrcRuntimeContext,
        memory: &MemoryMap,
    ) -> Option<u32> {
        let sp = cpu.a[7];
        for raw in [
            memory.read_u32_be(sp).unwrap_or(0),
            cpu.a[0],
            cpu.d[0],
            cpu.d[3],
            cpu.a[1],
            cpu.d[1],
        ] {
            if raw != 0 && runtime.open_databases.iter().any(|open| open.db_ref == raw) {
                return Some(raw);
            }
        }
        runtime.open_databases.last().map(|open| open.db_ref)
    }

    fn resolve_local_id_after_card_no(runtime: &PrcRuntimeContext, memory: &MemoryMap, sp: u32) -> u32 {
        for raw in [
            memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0),
            memory.read_u32_be(sp).unwrap_or(0),
            memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0),
        ] {
            if raw != 0 && db_runtime::db_by_local_id(runtime, raw).is_some() {
                return raw;
            }
        }
        memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0)
    }

    fn fourcc_name(db_type: u32) -> String {
        let bytes = db_type.to_be_bytes();
        if bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            alloc::format!("DB{:08X}", db_type)
        }
    }

    fn normalize_tstr_payload(data: &[u8]) -> Vec<u8> {
        // Palm string resources may be stored as C strings or length-prefixed.
        // Normalize them to C-string payloads so StrLen/StrCopy/Fld flows see
        // the same memory layout regardless of resource encoding.
        if data.is_empty() {
            return [0u8].to_vec();
        }
        if data.contains(&0) {
            return data.to_vec();
        }
        let len8 = data[0] as usize;
        if len8 > 0 && len8 < data.len() {
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
            let resources: Vec<(u32, u16, Vec<u8>)> = db_runtime::resource_entries_for_db(runtime, db)
                .into_iter()
                .map(|res| (res.kind, res.id, res.data.clone()))
                .collect();
            for (res_kind, res_id, res_data) in resources {
                if res_kind == kind && res_id == id {
                    let mut data = res_data;
                    if kind == u32::from_be_bytes(*b"tSTR") {
                        data = Self::normalize_tstr_payload(&data);
                    }
                    let handle = if let Some(existing) = runtime
                        .mem_blocks
                        .iter_mut()
                        .find(|b| b.resource_kind == Some(res_kind) && b.resource_id == Some(res_id))
                    {
                        if existing.data != data {
                            existing.data = data.clone();
                            existing.size = existing.data.len().max(16) as u32;
                            memory.upsert_overlay(existing.ptr, existing.data.clone());
                        }
                        existing.handle
                    } else {
                        db_runtime::alloc_mem(
                            runtime,
                            memory,
                            data,
                            Some(res_kind),
                            Some(res_id),
                        )
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
                    let mut data = res.data.clone();
                    if kind == u32::from_be_bytes(*b"tSTR") {
                        data = Self::normalize_tstr_payload(&data);
                    }
                    let handle = if let Some(existing) = runtime
                        .mem_blocks
                        .iter_mut()
                        .find(|b| b.resource_kind == Some(res.kind) && b.resource_id == Some(res.id))
                    {
                        if existing.data != data {
                            existing.data = data.clone();
                            existing.size = existing.data.len().max(16) as u32;
                            memory.upsert_overlay(existing.ptr, existing.data.clone());
                        }
                        existing.handle
                    } else {
                        db_runtime::alloc_mem(
                            runtime,
                            memory,
                            data,
                            Some(res.kind),
                            Some(res.id),
                        )
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
                let mut data = res.data.clone();
                if kind == u32::from_be_bytes(*b"tSTR") {
                    data = Self::normalize_tstr_payload(&data);
                }
                let handle = if let Some(existing) = runtime
                    .mem_blocks
                    .iter_mut()
                    .find(|b| b.resource_kind == Some(res.kind) && b.resource_id == Some(res.id))
                {
                    if existing.data != data {
                        existing.data = data.clone();
                        existing.size = existing.data.len().max(16) as u32;
                        memory.upsert_overlay(existing.ptr, existing.data.clone());
                    }
                    existing.handle
                } else {
                    db_runtime::alloc_mem(
                        runtime,
                        memory,
                        data,
                        Some(res.kind),
                        Some(res.id),
                    )
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
                .and_then(|open| db_runtime::db_by_local_id(runtime, open.local_id))
                .filter(|db| db.is_resource_db)
                .cloned()
                .or_else(|| {
                    runtime
                        .open_databases
                        .iter()
                        .rev()
                        .filter_map(|open| db_runtime::db_by_local_id(runtime, open.local_id))
                        .find(|db| db.is_resource_db)
                        .cloned()
                })
                .or_else(|| db_runtime::current_app_resource_db(runtime).cloned());
            Self::find_resource_handle(cpu, runtime, memory, db.as_ref(), kind, id);
        } else {
            let candidates: Vec<RuntimeDatabase> = runtime
                .open_databases
                .iter()
                .rev()
                .filter_map(|open| db_runtime::db_by_local_id(runtime, open.local_id).cloned())
                .collect();
            for db in &candidates {
                Self::find_resource_handle(cpu, runtime, memory, Some(db), kind, id);
                if cpu.a[0] != 0 {
                    break;
                }
            }
            if cpu.a[0] == 0 {
                let db = db_runtime::current_app_resource_db(runtime).cloned();
                Self::find_resource_handle(cpu, runtime, memory, db.as_ref(), kind, id);
            }
        }
        if cpu.a[0] == 0 {
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
        } else {
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
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
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        }
        if db_runtime::db_by_name(runtime, &name).is_some() {
            cpu.d[0] = Self::DM_ERR_ALREADY_EXISTS as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_ALREADY_EXISTS);
            return;
        }

        db_runtime::create_database(runtime, name, db_type, creator, res_db);
        log::info!(
            "Palm DmCreateDatabase name={:?} type={:08X} creator={:08X} res_db={}",
            Self::read_c_string(memory, name_p),
            db_type,
            creator,
            res_db
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_find_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        let name_p = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let name = Self::read_c_string(memory, name_p);
        let local_id = db_runtime::db_by_name(runtime, &name).map(|db| db.local_id).unwrap_or(0);
        log::info!(
            "Palm DmFindDatabase name={name:?} -> local_id=0x{local_id:08X}"
        );
        cpu.d[0] = local_id;
        cpu.a[0] = local_id;
        if local_id == 0 {
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
        } else {
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
        }
    }

    fn dm_delete_database(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        let local_id = Self::resolve_local_id_after_card_no(runtime, memory, sp);
        let target_name = db_runtime::db_by_local_id(runtime, local_id)
            .map(|db| db.name.clone())
            .unwrap_or_default();
        let before = runtime.databases.len();
        runtime.databases.retain(|db| db.local_id != local_id);
        runtime.open_databases.retain(|open| open.local_id != local_id);
        log::info!(
            "Palm DmDeleteDatabase local_id=0x{local_id:08X} name={target_name:?} removed={}",
            runtime.databases.len() != before
        );
        if runtime.databases.len() == before {
            cpu.d[0] = Self::DM_ERR_CANT_FIND as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        }
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_get_database(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(2)).unwrap_or(0) as usize;
        let local_id = runtime
            .databases
            .get(index)
            .map(|db| db.local_id)
            .unwrap_or(0);
        let db_name = runtime
            .databases
            .get(index)
            .map(|db| db.name.clone())
            .unwrap_or_default();
        log::info!(
            "Palm DmGetDatabase index={} -> local_id=0x{local_id:08X} name={db_name:?}",
            index
        );
        cpu.d[0] = local_id;
        cpu.a[0] = local_id;
        if local_id == 0 {
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
        } else {
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
        }
    }

    fn dm_num_databases(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        log::info!(
            "Palm DmNumDatabases -> {}",
            runtime.databases.len()
        );
        cpu.d[0] = runtime.databases.len().min(u16::MAX as usize) as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_database_info(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let local_id = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let Some(db) = db_runtime::db_by_local_id(runtime, local_id).cloned() else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
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
        log::info!(
            "Palm DmDatabaseInfo local_id=0x{local_id:08X} name={:?} type={:08X} creator={:08X} res_db={} records={}",
            db.name,
            db.db_type,
            db.creator,
            db.is_resource_db,
            db.record_handles.len()
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_open_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let local_id = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let mode = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0);
        if db_runtime::db_by_local_id(runtime, local_id).is_none() {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        }
        let db_ref = db_runtime::open_ref_for_local_id(runtime, local_id, mode);
        let db_name = db_runtime::db_by_local_id(runtime, local_id)
            .map(|db| db.name.clone())
            .unwrap_or_default();
        log::info!(
            "Palm DmOpenDatabase local_id=0x{local_id:08X} name={db_name:?} mode=0x{mode:04X} -> db_ref=0x{db_ref:08X}"
        );
        cpu.a[0] = db_ref;
        cpu.d[0] = db_ref;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_close_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let Some(db_ref) = Self::resolve_open_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if let Some(pos) = runtime.open_databases.iter().position(|o| o.db_ref == db_ref) {
            runtime.open_databases.remove(pos);
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
        } else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
        }
    }

    fn dm_open_database_info(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let Some(db_ref) = Self::resolve_open_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Some(open) = runtime.open_databases.iter().find(|o| o.db_ref == db_ref).cloned() else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Some(db) = db_runtime::db_by_local_id(runtime, open.local_id).cloned() else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
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
        log::info!(
            "Palm DmOpenDatabaseInfo db_ref=0x{db_ref:08X} local_id=0x{:08X} name={:?} open_count={} mode=0x{:04X}",
            db.local_id,
            db.name,
            runtime
                .open_databases
                .iter()
                .filter(|o| o.local_id == db.local_id)
                .count()
                .max(1),
            open.mode
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
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
        let Some((local_id, name, records)) = runtime
            .databases
            .iter()
            .find(|db| db.db_type == db_type && db.creator == creator)
            .map(|db| (db.local_id, db.name.clone(), db.record_handles.len()))
        else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        };
        let db_ref = db_runtime::open_ref_for_local_id(runtime, local_id, mode);
        log::info!(
            "Palm DmOpenDatabaseByTypeCreator type={db_type:08X} creator={creator:08X} -> db_ref=0x{db_ref:08X} local_id=0x{local_id:08X} name={name:?} records={records}"
        );
        if records > 0 && runtime.trace_traps {
            runtime.trace_trap_budget = runtime.trace_trap_budget.max(256);
            log::info!(
                "Palm DmOpenDatabaseByTypeCreator armed focused trap trace for db_ref=0x{db_ref:08X} name={name:?}"
            );
        }
        cpu.a[0] = db_ref;
        cpu.d[0] = db_ref;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_get_next_database_by_type_creator(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let new_search = memory.read_u16_be(sp).unwrap_or(0) != 0;
        let state_info_p = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let db_type = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let creator = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
        let only_latest = memory.read_u16_be(sp.saturating_add(14)).unwrap_or(0) != 0;
        let card_no_p = memory.read_u32_be(sp.saturating_add(16)).unwrap_or(0);
        let db_id_p = memory.read_u32_be(sp.saturating_add(20)).unwrap_or(0);

        let search_pos = if new_search {
            runtime
                .db_search_states
                .retain(|state| state.state_ptr != state_info_p);
            runtime.db_search_states.push(crate::palm::runtime::RuntimeDbSearchState {
                state_ptr: state_info_p,
                db_type,
                creator,
                only_latest,
                next_match_index: 0,
            });
            runtime.db_search_states.len().saturating_sub(1)
        } else if let Some(pos) = runtime
            .db_search_states
            .iter()
            .position(|state| state.state_ptr == state_info_p)
        {
            pos
        } else {
            runtime.db_search_states.push(crate::palm::runtime::RuntimeDbSearchState {
                state_ptr: state_info_p,
                db_type,
                creator,
                only_latest,
                next_match_index: 0,
            });
            runtime.db_search_states.len().saturating_sub(1)
        };

        let start = runtime.db_search_states[search_pos].next_match_index;
        let matched = if only_latest && db_type != 0 && creator != 0 {
            runtime
                .databases
                .iter()
                .enumerate()
                .filter(|(_, db)| db.db_type == db_type && db.creator == creator)
                .max_by_key(|(_, db)| (db.version, db.card_no, db.local_id))
                .filter(|(idx, _)| *idx >= start)
        } else {
            runtime
                .databases
                .iter()
                .enumerate()
                .skip(start)
                .find(|(_, db)| (db_type == 0 || db.db_type == db_type) && (creator == 0 || db.creator == creator))
        };

        let Some((match_index, db)) = matched else {
            if card_no_p != 0 && memory.contains_addr(card_no_p) {
                let _ = memory.write_u16_be(card_no_p, 0);
            }
            if db_id_p != 0 && memory.contains_addr(db_id_p) {
                let _ = memory.write_u32_be(db_id_p, 0);
            }
            log::info!(
                "Palm DmGetNextDatabaseByTypeCreator new_search={} type={db_type:08X} creator={creator:08X} latest={} -> not found",
                new_search,
                only_latest
            );
            cpu.d[0] = Self::DM_ERR_CANT_FIND as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        };

        runtime.db_search_states[search_pos].next_match_index = match_index.saturating_add(1);
        if card_no_p != 0 && memory.contains_addr(card_no_p) {
            let _ = memory.write_u16_be(card_no_p, db.card_no);
        }
        if db_id_p != 0 && memory.contains_addr(db_id_p) {
            let _ = memory.write_u32_be(db_id_p, db.local_id);
        }
        log::info!(
            "Palm DmGetNextDatabaseByTypeCreator new_search={} type={db_type:08X} creator={creator:08X} latest={} -> local_id=0x{:08X} name={:?} card={} res_db={}",
            new_search,
            only_latest,
            db.local_id,
            db.name,
            db.card_no,
            db.is_resource_db
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_set_database_info(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let local_id = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let Some(db) = db_runtime::db_by_local_id_mut(runtime, local_id) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
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
        log::info!(
            "Palm DmSetDatabaseInfo local_id=0x{local_id:08X} name={:?} type={:08X} creator={:08X} attrs=0x{:04X}",
            db.name,
            db.db_type,
            db.creator,
            db.attributes
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn category_matches(_category: u16) -> bool {
        // The runtime DB bridge does not yet preserve Palm record category
        // metadata, so treat all records as visible in any category for now.
        true
    }

    fn dm_num_records_in_category(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let category = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(Self::DM_UNFILED_CATEGORY);
        let count = if Self::category_matches(category) {
            db_runtime::record_count(runtime, db_ref).unwrap_or(0)
        } else {
            0
        };
        cpu.d[0] = (count.min(u16::MAX as usize) as u16) as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_position_in_category(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let category = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(Self::DM_UNFILED_CATEGORY);
        let count = db_runtime::record_count(runtime, db_ref).unwrap_or(0);
        let pos = if Self::category_matches(category) && index < count {
            index
        } else {
            count
        };
        cpu.d[0] = (pos.min(u16::MAX as usize) as u16) as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_query_next_in_category(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
        let category = memory.read_u16_be(sp.saturating_add(8)).unwrap_or(Self::DM_UNFILED_CATEGORY);
        let mut index = memory.read_u16_be(index_p).unwrap_or(0) as usize;
        let resolved = db_runtime::resolved_record_db(runtime, db_ref);
        let count = db_runtime::record_count(runtime, db_ref).unwrap_or(0);
        log::info!(
            "Palm DmQueryNextInCategory db_ref=0x{db_ref:08X} index={} category={} count={} resolved={:?}",
            index,
            category,
            count,
            resolved.map(|db| (&db.name, db.local_id, db.is_resource_db))
        );
        if !Self::category_matches(category) {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        }
        while index < count {
            if let Some(handle) = db_runtime::record_handle_by_index(runtime, db_ref, index) {
                let _ = memory.write_u16_be(index_p, index as u16);
                log::info!(
                    "Palm DmQueryNextInCategory -> index={} handle=0x{handle:08X}",
                    index
                );
                cpu.a[0] = handle;
                cpu.d[0] = handle;
                db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
                return;
            }
            index += 1;
        }
        cpu.a[0] = 0;
        cpu.d[0] = 0;
        log::info!("Palm DmQueryNextInCategory -> not found");
        db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
    }

    fn dm_seek_record_in_category(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
        let offset = memory.read_u16_be(sp.saturating_add(8)).unwrap_or(0) as i32;
        let direction = memory
            .read_u16_be(sp.saturating_add(10))
            .map(|v| v as i16)
            .unwrap_or(1) as i32;
        let category = memory.read_u16_be(sp.saturating_add(12)).unwrap_or(Self::DM_UNFILED_CATEGORY);
        let resolved = db_runtime::resolved_record_db(runtime, db_ref);
        let count = db_runtime::record_count(runtime, db_ref).unwrap_or(0) as i32;
        log::info!(
            "Palm DmSeekRecordInCategory db_ref=0x{db_ref:08X} index={} offset={} direction={} category={} count={} resolved={:?}",
            memory.read_u16_be(index_p).unwrap_or(0),
            offset,
            direction,
            category,
            count,
            resolved.map(|db| (&db.name, db.local_id, db.is_resource_db))
        );
        if !Self::category_matches(category) || count <= 0 {
            cpu.d[0] = Self::DM_ERR_CANT_FIND as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        }

        let mut index = memory.read_u16_be(index_p).unwrap_or(0) as i32;
        let step = if direction < 0 { -1 } else { 1 };
        let mut remaining = offset.max(0);

        while remaining > 0 {
            index += step;
            if index < 0 || index >= count {
                cpu.d[0] = Self::DM_ERR_CANT_FIND as u32;
                db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
                return;
            }
            remaining -= 1;
        }

        if index < 0 || index >= count {
            cpu.d[0] = Self::DM_ERR_CANT_FIND as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        }

        let _ = memory.write_u16_be(index_p, index as u16);
        log::info!("Palm DmSeekRecordInCategory -> index={}", index);
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
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

        let Ok((index, handle)) =
            db_runtime::create_new_record(runtime, memory, db_ref, insert_at, size)
        else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if at_p != 0 && memory.contains_addr(at_p) {
            let _ = memory.write_u16_be(at_p, index as u16);
        }
        let target = db_runtime::resolved_record_db(runtime, db_ref)
            .map(|db| (db.name.clone(), db.local_id, db.record_handles.len()));
        log::info!(
            "Palm DmNewRecord db_ref=0x{db_ref:08X} target={target:?} size={} insert_at={} -> index={} handle=0x{handle:08X}",
            size,
            insert_at,
            index
        );
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_new_handle(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let size = memory
            .read_u32_be(sp)
            .or_else(|| memory.read_u32_be(sp.saturating_add(4)))
            .unwrap_or(0)
            .clamp(16, 1_048_576);
        let handle = db_runtime::alloc_mem(runtime, memory, vec![0u8; size as usize], None, None);
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_num_records(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Ok(count) = db_runtime::record_count(runtime, stack_db_ref) else {
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        cpu.d[0] = count as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_record_info(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let attr_p = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let unique_id_p = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
        let chunk_id_p = memory.read_u32_be(sp.saturating_add(14)).unwrap_or(0);
        let Ok((attributes, unique_id, handle)) =
            db_runtime::record_info(runtime, stack_db_ref, index)
        else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if attr_p != 0 && memory.contains_addr(attr_p) {
            let _ = memory.write_u8(attr_p, attributes);
        }
        if unique_id_p != 0 && memory.contains_addr(unique_id_p) {
            let _ = memory.write_u32_be(unique_id_p, unique_id);
        }
        if chunk_id_p != 0 && memory.contains_addr(chunk_id_p) {
            let _ = memory.write_u32_be(chunk_id_p, handle);
        }
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_set_record_info(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let attr_p = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let unique_id_p = memory.read_u32_be(sp.saturating_add(10)).unwrap_or(0);
        let Ok((_attributes, _unique_id, _handle)) =
            db_runtime::record_info(runtime, stack_db_ref, index)
        else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let _ = attr_p;
        let _ = unique_id_p;
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_attach_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let at_p = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0);
        let new_h_raw = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
        let old_hp = memory.read_u32_be(sp.saturating_add(12)).unwrap_or(0);
        let new_h = db_runtime::handle_from_any(runtime, new_h_raw).unwrap_or(new_h_raw);
        let insert_at = memory.read_u16_be(at_p).unwrap_or(u16::MAX) as usize;
        let replacing = old_hp != 0 && memory.contains_addr(old_hp);
        let target = db_runtime::resolved_record_db(runtime, stack_db_ref)
            .map(|db| (db.name.clone(), db.local_id, db.record_handles.len()));
        let result = if replacing {
            db_runtime::replace_record_handle(runtime, stack_db_ref, insert_at, new_h).map(|old| {
                let _ = memory.write_u32_be(old_hp, old);
                insert_at
            })
        } else {
            db_runtime::attach_record_handle(runtime, stack_db_ref, insert_at, new_h)
        };
        let Ok(index) = result else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if at_p != 0 && memory.contains_addr(at_p) {
            let _ = memory.write_u16_be(at_p, index as u16);
        }
        if !replacing && old_hp != 0 && memory.contains_addr(old_hp) {
            let _ = memory.write_u32_be(old_hp, 0);
        }
        let final_count = db_runtime::record_count(runtime, stack_db_ref).unwrap_or(0);
        log::info!(
            "Palm DmAttachRecord db_ref=0x{stack_db_ref:08X} target={target:?} at={} new_h=0x{new_h:08X} replacing={} -> index={} count={}",
            insert_at,
            replacing,
            index,
            final_count
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_detach_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let old_hp = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0);
        let target = db_runtime::resolved_record_db(runtime, stack_db_ref)
            .map(|db| (db.name.clone(), db.local_id, db.record_handles.len()));
        let Ok(handle) = db_runtime::detach_record_handle(runtime, stack_db_ref, index) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        if old_hp != 0 && memory.contains_addr(old_hp) {
            let _ = memory.write_u32_be(old_hp, handle);
        }
        let final_count = db_runtime::record_count(runtime, stack_db_ref).unwrap_or(0);
        log::info!(
            "Palm DmDetachRecord db_ref=0x{stack_db_ref:08X} target={target:?} index={} -> handle=0x{handle:08X} count={}",
            index,
            final_count
        );
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_query_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        lock_record: bool,
    ) {
        let sp = cpu.a[7];
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let Ok(handle) =
            db_runtime::query_record(runtime, memory, stack_db_ref, index, lock_record)
        else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let target = db_runtime::resolved_record_db(runtime, stack_db_ref)
            .map(|db| (db.name.clone(), db.local_id, db.record_handles.len()));
        log::info!(
            "Palm {} db_ref=0x{stack_db_ref:08X} target={target:?} index={} -> handle=0x{handle:08X}",
            if lock_record { "DmGetRecord" } else { "DmQueryRecord" },
            index
        );
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_resize_record(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let stack_db_ref = Self::resolve_record_db_ref(cpu, runtime, memory).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let new_size = memory.read_u32_be(sp.saturating_add(6)).unwrap_or(0) as usize;

        let mut handle = db_runtime::record_handle_by_index(runtime, stack_db_ref, index);
        if handle.is_none() {
            // Some glue paths pass stale/zero stack args; fall back to register candidates.
            for raw in [cpu.a[0], cpu.d[3], cpu.d[0], cpu.a[1], cpu.d[1]] {
                if let Some(h) = db_runtime::handle_from_any(runtime, raw) {
                    handle = Some(h);
                    break;
                }
            }
        }
        if handle.is_none() {
            // Last resort: use latest open DB + requested index.
            handle = runtime
                .open_databases
                .last()
                .and_then(|open| db_runtime::record_handle_by_index(runtime, open.db_ref, index));
        }
        let Some(handle) = handle else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let Ok(handle) = db_runtime::resize_record_by_handle(runtime, memory, handle, new_size) else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        log::info!(
            "Palm DmResizeRecord db_ref=0x{stack_db_ref:08X} index={} -> handle=0x{handle:08X} new_size={}",
            index,
            new_size
        );
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_release_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
    ) {
        let sp = cpu.a[7];
        let Some(stack_db_ref) = Self::resolve_record_db_ref(cpu, runtime, memory) else {
            cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let dirty = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0) != 0;
        let Err(_) = db_runtime::release_record(runtime, stack_db_ref, index, dirty) else {
            let target = db_runtime::resolved_record_db(runtime, stack_db_ref)
                .map(|db| (db.name.clone(), db.local_id, db.record_handles.len()));
            log::info!(
                "Palm DmReleaseRecord db_ref=0x{stack_db_ref:08X} target={target:?} index={} dirty={}",
                index,
                dirty
            );
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
            return;
        };
        cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
    }

    fn dm_set(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let record_p = memory.read_u32_be(sp).unwrap_or(0);
        let offset = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let bytes = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0) as usize;
        let value = memory.read_u16_be(sp.saturating_add(12)).unwrap_or(0) as u8;
        let Err(_) = db_runtime::set_record_bytes(runtime, memory, record_p, offset, bytes, value) else {
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
            return;
        };
        cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
    }

    fn dm_write(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let record_p = memory.read_u32_be(sp).unwrap_or(0);
        let offset = memory.read_u32_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let src_p = memory.read_u32_be(sp.saturating_add(8)).unwrap_or(0);
        let bytes = memory.read_u32_be(sp.saturating_add(12)).unwrap_or(0) as usize;
        let Err(_) =
            db_runtime::write_record_bytes(runtime, memory, record_p, offset, src_p, bytes)
        else {
            log::info!(
                "Palm DmWrite record_ptr=0x{record_p:08X} offset={} src=0x{src_p:08X} bytes={}",
                offset,
                bytes
            );
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
            return;
        };
        cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
    }
}
