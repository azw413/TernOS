extern crate alloc;

use alloc::{string::String, vec::Vec};

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
        cpu.d[0] = 0;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_find_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let _card_no = memory.read_u16_be(sp).unwrap_or(0);
        let name_p = memory.read_u32_be(sp.saturating_add(2)).unwrap_or(0);
        let name = Self::read_c_string(memory, name_p);
        let local_id = db_runtime::db_by_name(runtime, &name).map(|db| db.local_id).unwrap_or(0);
        cpu.d[0] = local_id;
        cpu.a[0] = local_id;
        if local_id == 0 {
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
        } else {
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
        }
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
        cpu.a[0] = db_ref;
        cpu.d[0] = db_ref;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_close_database(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let db_ref = memory.read_u32_be(sp).unwrap_or(cpu.a[0]);
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
        let db_ref = memory.read_u32_be(sp).unwrap_or(cpu.a[0]);
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
        let Some(local_id) = runtime
            .databases
            .iter()
            .find(|db| db.db_type == db_type && db.creator == creator)
            .map(|db| db.local_id)
        else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_CANT_FIND);
            return;
        };
        let db_ref = db_runtime::open_ref_for_local_id(runtime, local_id, mode);
        cpu.a[0] = db_ref;
        cpu.d[0] = db_ref;
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
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_num_records(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
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
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
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

    fn dm_query_record(
        cpu: &mut CpuState68k,
        runtime: &mut PrcRuntimeContext,
        memory: &mut MemoryMap,
        lock_record: bool,
    ) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let Ok(handle) =
            db_runtime::query_record(runtime, memory, stack_db_ref, index, lock_record)
        else {
            cpu.a[0] = 0;
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
            return;
        };
        cpu.a[0] = handle;
        cpu.d[0] = handle;
        db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
    }

    fn dm_resize_record(cpu: &mut CpuState68k, runtime: &mut PrcRuntimeContext, memory: &mut MemoryMap) {
        let sp = cpu.a[7];
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
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
        let stack_db_ref = memory.read_u32_be(sp).unwrap_or(0);
        let index = memory.read_u16_be(sp.saturating_add(4)).unwrap_or(0) as usize;
        let dirty = memory.read_u16_be(sp.saturating_add(6)).unwrap_or(0) != 0;
        let Err(_) = db_runtime::release_record(runtime, stack_db_ref, index, dirty) else {
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
            cpu.d[0] = 0;
            db_runtime::set_last_err(runtime, Self::DM_ERR_NONE);
            return;
        };
        cpu.d[0] = Self::DM_ERR_INVALID_PARAM as u32;
        db_runtime::set_last_err(runtime, Self::DM_ERR_INVALID_PARAM);
    }
}
