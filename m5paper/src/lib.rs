#[cfg(feature = "cshim")]
pub mod ffi;
#[cfg(feature = "cshim")]
pub mod platform;

use esp_idf_sys as _;

pub fn m5paper_idf_link_anchor() {}
