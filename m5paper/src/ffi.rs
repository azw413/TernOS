use core::fmt;

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
pub struct TouchState {
    pub touched: bool,
    pub x: u16,
    pub y: u16,
    pub count: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct ButtonState {
    pub up_pressed: bool,
    pub power_pressed: bool,
    pub down_pressed: bool,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum InputEventType {
    None = 0,
    ButtonDown = 1,
    ButtonUp = 2,
    TouchDown = 3,
    TouchMove = 4,
    TouchUp = 5,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ButtonId {
    Up = 1,
    Down = 2,
    Power = 3,
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
    pub name: [u8; 256],
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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Ok = 0,
    Unsupported = 1,
    IoError = 2,
    Timeout = 3,
    NotFound = 4,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Ok => f.write_str("ok"),
            Status::Unsupported => f.write_str("unsupported"),
            Status::IoError => f.write_str("io error"),
            Status::Timeout => f.write_str("timeout"),
            Status::NotFound => f.write_str("not found"),
        }
    }
}

#[cfg(feature = "cshim")]
unsafe extern "C" {
    pub fn tern_m5paper_bridge_start() -> Status;
    pub fn tern_m5paper_board_init() -> Status;
    pub fn tern_m5paper_epd_init(out_info: *mut EpdInfo) -> Status;
    pub fn tern_m5paper_epd_clear(init: bool) -> Status;
    pub fn tern_m5paper_epd_update_region(
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        data: *const u8,
        data_len: u32,
    ) -> Status;
    pub fn tern_m5paper_touch_init() -> Status;
    pub fn tern_m5paper_touch_read(out_state: *mut TouchState) -> Status;
    pub fn tern_m5paper_buttons_read(out_state: *mut ButtonState) -> Status;
    pub fn tern_m5paper_input_next(out_event: *mut InputEvent) -> Status;
    pub fn tern_m5paper_rtc_init() -> Status;
    pub fn tern_m5paper_rtc_read(out_datetime: *mut RtcDateTime) -> Status;
    pub fn tern_m5paper_rtc_set(datetime: *const RtcDateTime) -> Status;
    pub fn tern_m5paper_storage_init() -> Status;
    pub fn tern_m5paper_storage_exists(path: *const core::ffi::c_char) -> bool;
    pub fn tern_m5paper_storage_list_begin(path: *const core::ffi::c_char) -> Status;
    pub fn tern_m5paper_storage_list_next(out_entry: *mut StorageEntry) -> Status;
    pub fn tern_m5paper_storage_list_end();
    pub fn tern_m5paper_storage_file_size(
        path: *const core::ffi::c_char,
        out_size: *mut u32,
    ) -> Status;
    pub fn tern_m5paper_storage_read_chunk(
        path: *const core::ffi::c_char,
        offset: u32,
        out_buf: *mut u8,
        buf_len: u32,
        out_read: *mut u32,
    ) -> Status;
}

#[cfg(feature = "cshim")]
pub fn bridge_start() -> Status {
    unsafe { tern_m5paper_bridge_start() }
}

#[cfg(feature = "cshim")]
pub fn board_init() -> Status {
    unsafe { tern_m5paper_board_init() }
}

#[cfg(feature = "cshim")]
pub fn epd_init() -> Result<EpdInfo, Status> {
    let mut info = EpdInfo::default();
    match unsafe { tern_m5paper_epd_init(&mut info) } {
        Status::Ok => Ok(info),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn epd_clear(init: bool) -> Result<(), Status> {
    match unsafe { tern_m5paper_epd_clear(init) } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn epd_update_region(
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    data: &[u8],
) -> Result<(), Status> {
    match unsafe {
        tern_m5paper_epd_update_region(
            x,
            y,
            width,
            height,
            data.as_ptr(),
            data.len() as u32,
        )
    } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn touch_init() -> Result<(), Status> {
    match unsafe { tern_m5paper_touch_init() } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn touch_read() -> Result<TouchState, Status> {
    let mut state = TouchState::default();
    match unsafe { tern_m5paper_touch_read(&mut state) } {
        Status::Ok => Ok(state),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn buttons_read() -> Result<ButtonState, Status> {
    let mut state = ButtonState::default();
    match unsafe { tern_m5paper_buttons_read(&mut state) } {
        Status::Ok => Ok(state),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn input_next() -> Result<Option<InputEvent>, Status> {
    let mut event = InputEvent::default();
    match unsafe { tern_m5paper_input_next(&mut event) } {
        Status::Ok => Ok(Some(event)),
        Status::NotFound => Ok(None),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn rtc_init() -> Result<(), Status> {
    match unsafe { tern_m5paper_rtc_init() } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn rtc_read() -> Result<RtcDateTime, Status> {
    let mut state = RtcDateTime::default();
    match unsafe { tern_m5paper_rtc_read(&mut state) } {
        Status::Ok => Ok(state),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn rtc_set(datetime: &RtcDateTime) -> Result<(), Status> {
    match unsafe { tern_m5paper_rtc_set(datetime) } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn storage_init() -> Result<(), Status> {
    match unsafe { tern_m5paper_storage_init() } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn storage_exists(path: &std::ffi::CString) -> bool {
    unsafe { tern_m5paper_storage_exists(path.as_ptr()) }
}

#[cfg(feature = "cshim")]
pub fn storage_list_begin(path: &std::ffi::CString) -> Result<(), Status> {
    match unsafe { tern_m5paper_storage_list_begin(path.as_ptr()) } {
        Status::Ok => Ok(()),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn storage_list_next() -> Result<Option<StorageEntry>, Status> {
    let mut entry = StorageEntry::default();
    match unsafe { tern_m5paper_storage_list_next(&mut entry) } {
        Status::Ok => Ok(Some(entry)),
        Status::NotFound => Ok(None),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn storage_list_end() {
    unsafe { tern_m5paper_storage_list_end() }
}

#[cfg(feature = "cshim")]
pub fn storage_file_size(path: &std::ffi::CString) -> Result<u32, Status> {
    let mut size = 0;
    match unsafe { tern_m5paper_storage_file_size(path.as_ptr(), &mut size) } {
        Status::Ok => Ok(size),
        err => Err(err),
    }
}

#[cfg(feature = "cshim")]
pub fn storage_read_chunk(
    path: &std::ffi::CString,
    offset: u32,
    out_buf: &mut [u8],
) -> Result<u32, Status> {
    let mut read = 0;
    match unsafe {
        tern_m5paper_storage_read_chunk(
            path.as_ptr(),
            offset,
            out_buf.as_mut_ptr(),
            out_buf.len() as u32,
            &mut read,
        )
    } {
        Status::Ok => Ok(read),
        err => Err(err),
    }
}
