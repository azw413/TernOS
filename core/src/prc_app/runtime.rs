extern crate alloc;

use crate::prc_app::prc::PrcInfo;
use crate::prc_app::traps::{TrapGroup, table};

pub const SYS_APP_LAUNCH_CMD_NORMAL_LAUNCH: u16 = 0;
pub const EVT_NIL: u16 = 0;
pub const EVT_FRM_LOAD: u16 = 23;
pub const EVT_FRM_OPEN: u16 = 24;

#[derive(Clone, Debug)]
pub struct PrcRuntimeContext {
    pub launch_cmd: u16,
    pub launch_flags: u16,
    pub cmd_pbp: u32,
    pub active_form_id: Option<u16>,
    pub active_form_handle: u32,
    pub active_form_handler: u32,
    pub sys_app_info_ptr: u32,
    pub shutting_down: bool,
    pub event_queue: alloc::vec::Vec<RuntimeEvent>,
    pub mem_blocks: alloc::vec::Vec<MemBlock>,
    pub resources: alloc::vec::Vec<ResourceBlob>,
    pub prc_image: alloc::vec::Vec<u8>,
    pub next_handle: u32,
    pub next_ptr: u32,
    pub rand_state: u32,
    pub ticks: u32,
    pub evt_polls: u32,
    pub current_font: u16,
    pub dm_get_resource_probe_count: u32,
    pub dm_get_resource_last_log: Option<(u32, u16, u32, u16)>,
    pub features: alloc::vec::Vec<FeatureEntry>,
    pub default_stubbed_traps: alloc::vec::Vec<u16>,
    pub fonts: alloc::vec::Vec<PalmFont>,
    pub drawn_form_id: Option<u16>,
    pub drawn_bitmaps: alloc::vec::Vec<RuntimeBitmapDraw>,
    pub blink_next_tick: u32,
    pub blink_phase: u8,
    pub terminate_requested: bool,
    pub trace_traps: bool,
    pub trace_trap_budget: u32,
    pub block_on_evt_get_event: bool,
    pub blocked_on_evt_get_event: bool,
    pub blocked_evt_timeout_ticks: u32,
    pub evt_event_p: u32,
    pub code_handle: u32,
    pub globals_ptr: u32,
    pub prev_globals_ptr: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeEvent {
    pub e_type: u16,
    pub data_u16: u16,
}

#[derive(Clone, Debug)]
pub struct MemBlock {
    pub handle: u32,
    pub ptr: u32,
    pub size: u32,
    pub locked: bool,
    pub data: alloc::vec::Vec<u8>,
    pub resource_kind: Option<u32>,
    pub resource_id: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct ResourceBlob {
    pub kind: u32,
    pub id: u16,
    pub data: alloc::vec::Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct FeatureEntry {
    pub creator: u32,
    pub num: u16,
    pub value: u32,
}

#[derive(Clone, Debug)]
pub struct PalmFont {
    pub font_id: u16,
    pub first_char: u8,
    pub last_char: u8,
    pub max_width: u8,
    pub avg_width: u8,
    pub rect_height: u8,
    pub widths: alloc::vec::Vec<u8>,
    pub glyphs: alloc::vec::Vec<Option<PalmGlyphBitmap>>,
}

#[derive(Clone, Debug)]
pub struct PalmGlyphBitmap {
    pub width: u8,
    pub rows: alloc::vec::Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct RuntimeBitmapDraw {
    pub resource_id: u16,
    pub x: i16,
    pub y: i16,
}

impl Default for PrcRuntimeContext {
    fn default() -> Self {
        Self {
            launch_cmd: SYS_APP_LAUNCH_CMD_NORMAL_LAUNCH,
            // Typical foreground launch: new globals + UI app.
            launch_flags: 0x000C,
            cmd_pbp: 0,
            active_form_id: None,
            active_form_handle: 0x3000_0000,
            active_form_handler: 0,
            sys_app_info_ptr: 0,
            shutting_down: false,
            event_queue: alloc::vec::Vec::new(),
            mem_blocks: alloc::vec::Vec::new(),
            resources: alloc::vec::Vec::new(),
            prc_image: alloc::vec::Vec::new(),
            next_handle: 1,
            next_ptr: 0x2000_0000,
            rand_state: 0x1234_5678,
            ticks: 0,
            evt_polls: 0,
            current_font: 0,
            dm_get_resource_probe_count: 0,
            dm_get_resource_last_log: None,
            features: alloc::vec::Vec::new(),
            default_stubbed_traps: alloc::vec::Vec::new(),
            fonts: alloc::vec::Vec::new(),
            drawn_form_id: None,
            drawn_bitmaps: alloc::vec::Vec::new(),
            blink_next_tick: 175,
            blink_phase: 0,
            terminate_requested: false,
            trace_traps: true,
            trace_trap_budget: 0,
            block_on_evt_get_event: false,
            blocked_on_evt_get_event: false,
            blocked_evt_timeout_ticks: 0,
            evt_event_p: 0,
            code_handle: 0,
            globals_ptr: 0,
            prev_globals_ptr: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapHandleResult {
    Handled,
    Stubbed,
    Unimplemented,
}

#[derive(Clone, Copy, Debug)]
pub struct DryRunPolicy {
    pub stub_lib_dispatch: bool,
    pub stub_bootstrap_lib_dispatch: bool,
    pub stub_unknown: bool,
}

impl Default for DryRunPolicy {
    fn default() -> Self {
        Self {
            stub_lib_dispatch: false,
            stub_bootstrap_lib_dispatch: false,
            stub_unknown: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DryRunStop {
    pub resource_id: u16,
    pub code_offset: u32,
    pub file_offset: u32,
    pub trap_word: u16,
    pub trap15: bool,
    pub group: TrapGroup,
    pub name: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct LibDispatchProbe {
    pub resource_id: u16,
    pub code_offset: u32,
    pub file_offset: u32,
    pub selector: Option<u16>,
    pub next_word_1: Option<u16>,
    pub next_word_2: Option<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct DryRunReport {
    pub total_hits: usize,
    pub handled: usize,
    pub stubbed: usize,
    pub unimplemented: Option<DryRunStop>,
    pub lib_dispatch_probes: alloc::vec::Vec<LibDispatchProbe>,
}

pub trait TrapDispatcher {
    fn handle_atrap(&self, trap_word: u16, group: TrapGroup, name: &'static str) -> TrapHandleResult;
    fn handle_trap15(&self) -> TrapHandleResult;
}

#[derive(Clone, Copy, Debug)]
pub struct DefaultTrapDispatcher {
    pub policy: DryRunPolicy,
}

impl TrapDispatcher for DefaultTrapDispatcher {
    fn handle_atrap(&self, _trap_word: u16, group: TrapGroup, _name: &'static str) -> TrapHandleResult {
        match group {
            TrapGroup::Lib if self.policy.stub_lib_dispatch => TrapHandleResult::Stubbed,
            TrapGroup::Unknown if self.policy.stub_unknown => TrapHandleResult::Stubbed,
            TrapGroup::Unknown | TrapGroup::Lib => TrapHandleResult::Unimplemented,
            _ => TrapHandleResult::Stubbed,
        }
    }

    fn handle_trap15(&self) -> TrapHandleResult {
        TrapHandleResult::Stubbed
    }
}

pub fn dry_run(info: &PrcInfo, dispatcher: &impl TrapDispatcher) -> DryRunReport {
    let mut report = DryRunReport::default();
    report.total_hits = info.trap_hits.len();
    for hit in &info.trap_hits {
        if !hit.is_trap15 && hit.trap_word == 0xA9F0 {
            let selector = match (hit.next_word_1, hit.next_word_2) {
                (Some(w), _) if (w & 0xFF00) == 0x7400 => Some((w & 0x00FF) as u16), // moveq #imm,D2
                (Some(0x343C), Some(sel)) => Some(sel), // move.w #imm,D2
                _ => None,
            };
            report.lib_dispatch_probes.push(LibDispatchProbe {
                resource_id: hit.resource_id,
                code_offset: hit.code_offset,
                file_offset: hit.file_offset,
                selector,
                next_word_1: hit.next_word_1,
                next_word_2: hit.next_word_2,
            });
        }
        let result = if hit.is_trap15 {
            dispatcher.handle_trap15()
        } else {
            let meta = table::lookup(hit.trap_word);
            dispatcher.handle_atrap(hit.trap_word, meta.group, meta.name)
        };
        match result {
            TrapHandleResult::Handled => report.handled += 1,
            TrapHandleResult::Stubbed => report.stubbed += 1,
            TrapHandleResult::Unimplemented => {
                let meta = table::lookup(hit.trap_word);
                report.unimplemented = Some(DryRunStop {
                    resource_id: hit.resource_id,
                    code_offset: hit.code_offset,
                    file_offset: hit.file_offset,
                    trap_word: hit.trap_word,
                    trap15: hit.is_trap15,
                    group: meta.group,
                    name: meta.name,
                });
                return report;
            }
        }
    }
    report
}

pub fn dry_run_with_policy(info: &PrcInfo, policy: DryRunPolicy) -> DryRunReport {
    let mut report = DryRunReport::default();
    report.total_hits = info.trap_hits.len();
    let dispatcher = DefaultTrapDispatcher { policy };
    for hit in &info.trap_hits {
        if !hit.is_trap15 && hit.trap_word == 0xA9F0 {
            let selector = match (hit.next_word_1, hit.next_word_2) {
                (Some(w), _) if (w & 0xFF00) == 0x7400 => Some((w & 0x00FF) as u16), // moveq #imm,D2
                (Some(0x343C), Some(sel)) => Some(sel), // move.w #imm,D2
                _ => None,
            };
            report.lib_dispatch_probes.push(LibDispatchProbe {
                resource_id: hit.resource_id,
                code_offset: hit.code_offset,
                file_offset: hit.file_offset,
                selector,
                next_word_1: hit.next_word_1,
                next_word_2: hit.next_word_2,
            });
            if policy.stub_bootstrap_lib_dispatch && hit.resource_id == 0 {
                report.stubbed += 1;
                continue;
            }
        }
        let result = if hit.is_trap15 {
            dispatcher.handle_trap15()
        } else {
            let meta = table::lookup(hit.trap_word);
            dispatcher.handle_atrap(hit.trap_word, meta.group, meta.name)
        };
        match result {
            TrapHandleResult::Handled => report.handled += 1,
            TrapHandleResult::Stubbed => report.stubbed += 1,
            TrapHandleResult::Unimplemented => {
                let meta = table::lookup(hit.trap_word);
                report.unimplemented = Some(DryRunStop {
                    resource_id: hit.resource_id,
                    code_offset: hit.code_offset,
                    file_offset: hit.file_offset,
                    trap_word: hit.trap_word,
                    trap15: hit.is_trap15,
                    group: meta.group,
                    name: meta.name,
                });
                return report;
            }
        }
    }
    report
}

pub fn dry_run_default(info: &PrcInfo) -> DryRunReport {
    dry_run_with_policy(info, DryRunPolicy::default())
}

pub fn dry_run_ignore_lib(info: &PrcInfo) -> DryRunReport {
    dry_run_with_policy(
        info,
        DryRunPolicy {
            stub_lib_dispatch: true,
            stub_bootstrap_lib_dispatch: false,
            stub_unknown: false,
        },
    )
}

pub fn dry_run_ignore_bootstrap_lib(info: &PrcInfo) -> DryRunReport {
    dry_run_with_policy(
        info,
        DryRunPolicy {
            stub_lib_dispatch: false,
            stub_bootstrap_lib_dispatch: true,
            stub_unknown: false,
        },
    )
}
