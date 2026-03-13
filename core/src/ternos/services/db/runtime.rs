extern crate alloc;

use alloc::{string::String, vec::Vec};

use crate::palm::{
    cpu::memory::MemoryMap,
    runtime::{MemBlock, PrcRuntimeContext, ResourceBlob, RuntimeDatabase, RuntimeOpenDatabase},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    InvalidParam,
}

pub fn set_last_err(runtime: &mut PrcRuntimeContext, err: u16) {
    runtime.dm_last_err = err;
}

pub fn current_app_resource_db(runtime: &PrcRuntimeContext) -> Option<&RuntimeDatabase> {
    runtime
        .databases
        .iter()
        .find(|db| db.is_resource_db && db.creator != 0 && db.db_type != 0)
}

pub fn db_by_local_id(runtime: &PrcRuntimeContext, local_id: u32) -> Option<&RuntimeDatabase> {
    runtime.databases.iter().find(|db| db.local_id == local_id)
}

pub fn db_by_name<'a>(runtime: &'a PrcRuntimeContext, name: &str) -> Option<&'a RuntimeDatabase> {
    runtime.databases.iter().find(|db| db.name == name)
}

pub fn db_by_ref(runtime: &PrcRuntimeContext, db_ref: u32) -> Option<&RuntimeDatabase> {
    let local_id = runtime
        .open_databases
        .iter()
        .find(|open| open.db_ref == db_ref)
        .map(|open| open.local_id)?;
    db_by_local_id(runtime, local_id)
}

pub fn open_ref_for_local_id(runtime: &mut PrcRuntimeContext, local_id: u32, mode: u16) -> u32 {
    if let Some(existing) = runtime
        .open_databases
        .iter_mut()
        .find(|open| open.local_id == local_id)
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

pub fn create_database(
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

pub fn db_by_local_id_mut(
    runtime: &mut PrcRuntimeContext,
    local_id: u32,
) -> Option<&mut RuntimeDatabase> {
    runtime.databases.iter_mut().find(|db| db.local_id == local_id)
}

pub fn db_by_ref_mut(
    runtime: &mut PrcRuntimeContext,
    db_ref: u32,
) -> Option<&mut RuntimeDatabase> {
    let local_id = runtime
        .open_databases
        .iter()
        .find(|open| open.db_ref == db_ref)
        .map(|open| open.local_id)?;
    db_by_local_id_mut(runtime, local_id)
}

pub fn resource_entries_for_db<'a>(
    runtime: &'a PrcRuntimeContext,
    _db: &RuntimeDatabase,
) -> Vec<&'a ResourceBlob> {
    runtime.resources.iter().collect()
}

pub fn alloc_mem(
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

pub fn handle_from_any(runtime: &PrcRuntimeContext, raw: u32) -> Option<u32> {
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

pub fn resolved_record_db<'a>(
    runtime: &'a PrcRuntimeContext,
    requested_db_ref: u32,
) -> Option<&'a RuntimeDatabase> {
    db_by_ref(runtime, requested_db_ref).or_else(|| {
        runtime
            .open_databases
            .last()
            .and_then(|open| db_by_local_id(runtime, open.local_id))
    })
}

pub fn resolved_record_db_mut<'a>(
    runtime: &'a mut PrcRuntimeContext,
    requested_db_ref: u32,
) -> Option<&'a mut RuntimeDatabase> {
    let effective_db_ref = if db_by_ref(runtime, requested_db_ref).is_some() {
        requested_db_ref
    } else {
        runtime.open_databases.last().map(|open| open.db_ref)?
    };
    db_by_ref_mut(runtime, effective_db_ref)
}

pub fn create_new_record(
    runtime: &mut PrcRuntimeContext,
    memory: &mut MemoryMap,
    db_ref: u32,
    insert_at: usize,
    size: usize,
) -> Result<(usize, u32), RecordError> {
    let mut data = Vec::new();
    data.resize(size, 0);
    let handle = alloc_mem(runtime, memory, data, None, None);
    let Some(db) = db_by_ref_mut(runtime, db_ref) else {
        return Err(RecordError::InvalidParam);
    };
    let index = insert_at.min(db.record_handles.len());
    db.record_handles.insert(index, handle);
    Ok((index, handle))
}

pub fn record_count(runtime: &PrcRuntimeContext, requested_db_ref: u32) -> Result<usize, RecordError> {
    let Some(db) = resolved_record_db(runtime, requested_db_ref) else {
        return Err(RecordError::InvalidParam);
    };
    Ok(db.record_handles.len())
}

pub fn record_info(
    runtime: &PrcRuntimeContext,
    requested_db_ref: u32,
    index: usize,
) -> Result<(u8, u32, u32), RecordError> {
    let Some(db) = resolved_record_db(runtime, requested_db_ref) else {
        return Err(RecordError::InvalidParam);
    };
    let Some(handle) = db.record_handles.get(index).copied() else {
        return Err(RecordError::InvalidParam);
    };
    Ok((0, (index as u32).saturating_add(1), handle))
}

pub fn query_record(
    runtime: &mut PrcRuntimeContext,
    memory: &mut MemoryMap,
    requested_db_ref: u32,
    index: usize,
    lock_record: bool,
) -> Result<u32, RecordError> {
    let handle = {
        let Some(db) = resolved_record_db(runtime, requested_db_ref) else {
            return Err(RecordError::InvalidParam);
        };
        let Some(handle) = db.record_handles.get(index).copied() else {
            return Err(RecordError::InvalidParam);
        };
        handle
    };
    if lock_record
        && let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.handle == handle)
    {
        block.locked = true;
        memory.upsert_overlay(block.ptr, block.data.clone());
    }
    Ok(handle)
}

pub fn record_handle_by_index(
    runtime: &PrcRuntimeContext,
    requested_db_ref: u32,
    index: usize,
) -> Option<u32> {
    resolved_record_db(runtime, requested_db_ref)?
        .record_handles
        .get(index)
        .copied()
}

pub fn resize_record_by_handle(
    runtime: &mut PrcRuntimeContext,
    memory: &mut MemoryMap,
    handle: u32,
    new_size: usize,
) -> Result<u32, RecordError> {
    let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.handle == handle) else {
        return Err(RecordError::InvalidParam);
    };
    let size = if new_size == 0 { block.data.len() } else { new_size };
    block.data.resize(size, 0);
    block.size = block.data.len() as u32;
    memory.upsert_overlay(block.ptr, block.data.clone());
    Ok(handle)
}

pub fn release_record(
    runtime: &mut PrcRuntimeContext,
    requested_db_ref: u32,
    index: usize,
    dirty: bool,
) -> Result<(), RecordError> {
    let Some(db) = resolved_record_db_mut(runtime, requested_db_ref) else {
        return Err(RecordError::InvalidParam);
    };
    if index >= db.record_handles.len() {
        return Err(RecordError::InvalidParam);
    }
    if dirty {
        db.mod_number = db.mod_number.saturating_add(1);
    }
    Ok(())
}

pub fn set_record_bytes(
    runtime: &mut PrcRuntimeContext,
    memory: &mut MemoryMap,
    record_ptr: u32,
    offset: usize,
    bytes: usize,
    value: u8,
) -> Result<(), RecordError> {
    let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.ptr == record_ptr) else {
        return Err(RecordError::InvalidParam);
    };
    if offset.saturating_add(bytes) > block.data.len() {
        return Err(RecordError::InvalidParam);
    }
    for i in offset..offset + bytes {
        block.data[i] = value;
    }
    memory.upsert_overlay(block.ptr, block.data.clone());
    Ok(())
}

pub fn write_record_bytes(
    runtime: &mut PrcRuntimeContext,
    memory: &mut MemoryMap,
    record_ptr: u32,
    offset: usize,
    src_ptr: u32,
    bytes: usize,
) -> Result<(), RecordError> {
    let Some(block) = runtime.mem_blocks.iter_mut().find(|b| b.ptr == record_ptr) else {
        return Err(RecordError::InvalidParam);
    };
    if offset.saturating_add(bytes) > block.data.len() {
        return Err(RecordError::InvalidParam);
    }
    for i in 0..bytes {
        let Some(b) = memory.read_u8(src_ptr.saturating_add(i as u32)) else {
            return Err(RecordError::InvalidParam);
        };
        block.data[offset + i] = b;
    }
    memory.upsert_overlay(block.ptr, block.data.clone());
    Ok(())
}
