use core::ffi::c_char;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct EpdInfo {
    pub panel_width: u16,
    pub panel_height: u16,
    pub image_buffer_addr: u32,
    pub vcom_mv: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct RtcDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub week: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct StorageEntry {
    pub is_dir: bool,
    pub size: u32,
    pub name: [c_char; 256],
}

impl Default for StorageEntry {
    fn default() -> Self {
        Self {
            is_dir: false,
            size: 0,
            name: [0; 256],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct InputEvent {
    pub event_type: u8,
    pub button_id: u8,
    pub x: u16,
    pub y: u16,
    pub touch_count: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Ok = 0,
    IoError = 1,
    Timeout = 2,
    NotFound = 3,
}

pub const INPUT_NONE: u8 = 0;
pub const INPUT_BUTTON_DOWN: u8 = 1;
pub const INPUT_BUTTON_UP: u8 = 2;
pub const INPUT_TOUCH_DOWN: u8 = 3;
pub const INPUT_TOUCH_MOVE: u8 = 4;
pub const INPUT_TOUCH_UP: u8 = 5;

unsafe extern "C" {
    fn tern_m5paper_backend_start() -> Status;
    fn tern_m5paper_backend_epd_init(out_info: *mut EpdInfo) -> Status;
    fn tern_m5paper_backend_epd_clear(init: bool) -> Status;
    fn tern_m5paper_backend_epd_fill_white() -> Status;
    fn tern_m5paper_backend_epd_update_region(
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        data: *const u8,
        data_len: u32,
    ) -> Status;
    fn tern_m5paper_backend_epd_test_pattern(x: u16, y: u16, width: u16, height: u16) -> Status;
    fn tern_m5paper_backend_rtc_init() -> Status;
    fn tern_m5paper_backend_rtc_read(out_datetime: *mut RtcDateTime) -> Status;
    fn tern_m5paper_backend_storage_init() -> Status;
    fn tern_m5paper_backend_storage_list_begin(path: *const c_char) -> Status;
    fn tern_m5paper_backend_storage_list_next(out_entry: *mut StorageEntry) -> Status;
    fn tern_m5paper_backend_storage_list_end();
    fn tern_m5paper_backend_storage_file_size(path: *const c_char, out_size: *mut u32) -> Status;
    fn tern_m5paper_backend_storage_read_chunk(
        path: *const c_char,
        offset: u32,
        out_buf: *mut u8,
        out_buf_len: u32,
        out_read: *mut u32,
    ) -> Status;
    fn tern_m5paper_backend_input_next(out_event: *mut InputEvent) -> Status;
    pub fn ets_printf(fmt: *const u8, ...) -> i32;
    fn vTaskDelay(ticks: u32);
}

pub fn backend_start() -> Status {
    unsafe { tern_m5paper_backend_start() }
}

pub fn epd_init() -> Result<EpdInfo, Status> {
    let mut info = EpdInfo::default();
    let status = unsafe { tern_m5paper_backend_epd_init(&mut info) };
    if status == Status::Ok {
        Ok(info)
    } else {
        Err(status)
    }
}

pub fn epd_clear(init: bool) -> Status {
    unsafe { tern_m5paper_backend_epd_clear(init) }
}

pub fn epd_fill_white() -> Status {
    unsafe { tern_m5paper_backend_epd_fill_white() }
}

pub fn epd_update_region(x: u16, y: u16, width: u16, height: u16, data: &[u8]) -> Status {
    unsafe {
        tern_m5paper_backend_epd_update_region(x, y, width, height, data.as_ptr(), data.len() as u32)
    }
}

pub fn epd_test_pattern(x: u16, y: u16, width: u16, height: u16) -> Status {
    unsafe { tern_m5paper_backend_epd_test_pattern(x, y, width, height) }
}

pub fn rtc_init() -> Status {
    unsafe { tern_m5paper_backend_rtc_init() }
}

pub fn rtc_read() -> Result<RtcDateTime, Status> {
    let mut dt = RtcDateTime::default();
    let status = unsafe { tern_m5paper_backend_rtc_read(&mut dt) };
    if status == Status::Ok {
        Ok(dt)
    } else {
        Err(status)
    }
}

pub fn storage_init() -> Status {
    unsafe { tern_m5paper_backend_storage_init() }
}

pub fn storage_list_begin(path: *const c_char) -> Status {
    unsafe { tern_m5paper_backend_storage_list_begin(path) }
}

pub fn storage_list_next() -> Result<Option<StorageEntry>, Status> {
    let mut entry = StorageEntry::default();
    let status = unsafe { tern_m5paper_backend_storage_list_next(&mut entry) };
    match status {
        Status::Ok => Ok(Some(entry)),
        Status::NotFound => Ok(None),
        _ => Err(status),
    }
}

pub fn storage_list_end() {
    unsafe { tern_m5paper_backend_storage_list_end() }
}

pub fn storage_file_size(path: *const c_char) -> Result<u32, Status> {
    let mut size = 0u32;
    let status = unsafe { tern_m5paper_backend_storage_file_size(path, &mut size) };
    if status == Status::Ok {
        Ok(size)
    } else {
        Err(status)
    }
}

pub fn storage_read_chunk(path: *const c_char, offset: u32, out: &mut [u8]) -> Result<u32, Status> {
    let mut read = 0u32;
    let status = unsafe {
        tern_m5paper_backend_storage_read_chunk(
            path,
            offset,
            out.as_mut_ptr(),
            out.len() as u32,
            &mut read,
        )
    };
    if status == Status::Ok {
        Ok(read)
    } else {
        Err(status)
    }
}

pub fn input_next() -> Result<Option<InputEvent>, Status> {
    let mut event = InputEvent::default();
    let status = unsafe { tern_m5paper_backend_input_next(&mut event) };
    if status == Status::Ok && event.event_type != INPUT_NONE {
        Ok(Some(event))
    } else if status == Status::Ok {
        Ok(None)
    } else {
        Err(status)
    }
}

pub fn log_line(msg: &str) {
    unsafe { ets_printf(msg.as_ptr()) };
}

pub fn log_status(prefix: &'static [u8], status: Status) {
    unsafe { ets_printf(prefix.as_ptr(), status as i32) };
}

pub fn log_storage_entry(entry: &StorageEntry) {
    let type_str: &[u8] = if entry.is_dir { b"dir\0" } else { b"file\0" };
    unsafe {
        ets_printf(
            b"m5paper-rust: entry type=%s name=%s size=%u\n\0".as_ptr(),
            type_str.as_ptr(),
            entry.name.as_ptr(),
            entry.size,
        );
    }
}

pub fn delay_ms(ms: u32) {
    unsafe { vTaskDelay(ms) };
}
