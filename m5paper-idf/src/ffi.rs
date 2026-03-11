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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Ok = 0,
    Unsupported = 1,
    IoError = 2,
    Timeout = 3,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Ok => f.write_str("ok"),
            Status::Unsupported => f.write_str("unsupported"),
            Status::IoError => f.write_str("io error"),
            Status::Timeout => f.write_str("timeout"),
        }
    }
}

#[cfg(feature = "cshim")]
unsafe extern "C" {
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
