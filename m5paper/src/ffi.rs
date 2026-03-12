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
    pub fn tern_m5paper_rtc_init() -> Status;
    pub fn tern_m5paper_rtc_read(out_datetime: *mut RtcDateTime) -> Status;
    pub fn tern_m5paper_rtc_set(datetime: *const RtcDateTime) -> Status;
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
