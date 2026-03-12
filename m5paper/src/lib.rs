#[cfg(feature = "cshim")]
pub mod ffi;
#[cfg(feature = "cshim")]
pub mod image_source;
#[cfg(feature = "cshim")]
pub mod platform;
#[cfg(feature = "cshim")]
pub mod runtime_host;

use esp_idf_sys as _;

pub fn m5paper_idf_link_anchor() {}
