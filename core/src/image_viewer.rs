extern crate alloc;

use alloc::string::String;
use alloc::rc::Rc;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
}

#[derive(Clone, Debug)]
pub struct ImageEntry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Clone, Debug)]
pub enum ImageData {
    Gray8 {
        width: u32,
        height: u32,
        pixels: Vec<u8>, // 8-bit grayscale, row-major
    },
    Gray2 {
        width: u32,
        height: u32,
        data: Vec<u8>, // concatenated planes: base | lsb | msb
    },
    Gray2Stream {
        width: u32,
        height: u32,
        key: String,
    },
    Mono1 {
        width: u32,
        height: u32,
        bits: Vec<u8>, // 1-bit packed, row-major, MSB first
    },
}

#[derive(Clone, Debug)]
pub enum ImageError {
    Io,
    Decode,
    Unsupported,
    Message(String),
}

pub trait ImageSource {
    fn refresh(&mut self, path: &[String]) -> Result<Vec<ImageEntry>, ImageError>;
    fn load(&mut self, path: &[String], entry: &ImageEntry) -> Result<ImageData, ImageError>;
    fn load_prc_info(
        &mut self,
        _path: &[String],
        _entry: &ImageEntry,
    ) -> Result<crate::palm::PrcInfo, ImageError> {
        Err(ImageError::Unsupported)
    }
    fn load_prc_code_resource(
        &mut self,
        _path: &[String],
        _entry: &ImageEntry,
        _resource_id: u16,
    ) -> Result<Vec<u8>, ImageError> {
        Err(ImageError::Unsupported)
    }
    fn load_prc_bytes(
        &mut self,
        _path: &[String],
        _entry: &ImageEntry,
    ) -> Result<Vec<u8>, ImageError> {
        Err(ImageError::Unsupported)
    }
    /// Optional app-specific extra resources (e.g. `ovly` DB paired by creator).
    fn load_prc_app_resources(
        &mut self,
        _path: &[String],
        _entry: &ImageEntry,
        _info: &crate::palm::PrcInfo,
    ) -> Vec<crate::palm::runtime::ResourceBlob> {
        Vec::new()
    }
    fn load_prc_system_resources(&mut self) -> Vec<crate::palm::runtime::ResourceBlob> {
        Vec::new()
    }
    fn load_prc_system_fonts(&mut self) -> Vec<crate::palm::runtime::PalmFont> {
        Vec::new()
    }
    /// Optional high-density Palm font set for full-resolution shell UI.
    ///
    /// Default falls back to runtime/system PRC fonts.
    fn load_home_system_fonts(&mut self) -> Vec<crate::palm::runtime::PalmFont> {
        self.load_prc_system_fonts()
    }
    /// Optional `/install` inbox scan hook.
    ///
    /// Implementations can return `Some(summary)` when they support Palm DB
    /// install scanning, or `None` to opt out.
    fn scan_palm_install_inbox(&mut self) -> Option<crate::ternos::services::db::InstallSummary> {
        None
    }

    /// Optional installed-app catalog for launcher Apps category.
    fn list_installed_apps(&mut self) -> Vec<InstalledAppEntry> {
        Vec::new()
    }
}

#[derive(Clone, Debug)]
pub struct InstalledAppEntry {
    pub title: String,
    pub path: String,
    pub icon: Option<ImageData>,
}

pub trait BookSource {
    fn load_trbk(
        &mut self,
        _path: &[String],
        _entry: &ImageEntry,
    ) -> Result<crate::trbk::TrbkBook, ImageError> {
        Err(ImageError::Unsupported)
    }
    fn open_trbk(
        &mut self,
        _path: &[String],
        _entry: &ImageEntry,
    ) -> Result<Rc<crate::trbk::TrbkBookInfo>, ImageError> {
        Err(ImageError::Unsupported)
    }
    fn trbk_page(&mut self, _page_index: usize) -> Result<crate::trbk::TrbkPage, ImageError> {
        Err(ImageError::Unsupported)
    }
    fn trbk_image(&mut self, _image_index: usize) -> Result<ImageData, ImageError> {
        Err(ImageError::Unsupported)
    }
    fn close_trbk(&mut self) {}
}

pub trait Gray2StreamSource {
    fn load_gray2_stream(
        &mut self,
        _key: &str,
        _width: u32,
        _height: u32,
        _rotation: crate::framebuffer::Rotation,
        _base: &mut [u8],
        _lsb: &mut [u8],
        _msb: &mut [u8],
    ) -> Result<(), ImageError> {
        Err(ImageError::Unsupported)
    }
    fn load_gray2_stream_region(
        &mut self,
        _key: &str,
        _width: u32,
        _height: u32,
        _rotation: crate::framebuffer::Rotation,
        _base: &mut [u8],
        _lsb: &mut [u8],
        _msb: &mut [u8],
        _dst_x: i32,
        _dst_y: i32,
    ) -> Result<(), ImageError> {
        Err(ImageError::Unsupported)
    }
    fn load_gray2_stream_thumbnail(
        &mut self,
        _key: &str,
        _width: u32,
        _height: u32,
        _thumb_w: u32,
        _thumb_h: u32,
    ) -> Option<ImageData> {
        None
    }
}

pub trait PersistenceSource {
    fn save_resume(&mut self, _name: Option<&str>) {}
    fn load_resume(&mut self) -> Option<String> {
        None
    }
    fn save_book_positions(&mut self, _entries: &[(String, usize)]) {}
    fn load_book_positions(&mut self) -> Vec<(String, usize)> {
        Vec::new()
    }
    fn save_recent_entries(&mut self, _entries: &[String]) {}
    fn load_recent_entries(&mut self) -> Vec<String> {
        Vec::new()
    }
    fn load_thumbnail(&mut self, _key: &str) -> Option<ImageData> {
        None
    }
    fn save_thumbnail(&mut self, _key: &str, _image: &ImageData) {}
    fn load_thumbnail_title(&mut self, _key: &str) -> Option<String> {
        None
    }
    fn save_thumbnail_title(&mut self, _key: &str, _title: &str) {}
}

pub trait PowerSource {
    fn sleep(&mut self) {}
    fn wake(&mut self) {}
}

pub trait AppSource:
    ImageSource + BookSource + Gray2StreamSource + PersistenceSource + PowerSource
{
}

impl<T> AppSource for T where
    T: ImageSource + BookSource + Gray2StreamSource + PersistenceSource + PowerSource
{
}
