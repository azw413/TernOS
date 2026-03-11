# M5Paper C Shim

Purpose:
- keep TernOS platform and UI layers in Rust
- isolate M5Paper-specific board support in a narrow C/C++ bridge
- reuse known-good M5Paper code paths from `../M5EPD` and `../M5Paper_FactoryTest`

Planned bridge surface:
- `tern_m5paper_board_init`
- `tern_m5paper_epd_init`
- `tern_m5paper_epd_clear`
- `tern_m5paper_epd_update_region`
- `tern_m5paper_touch_init`
- `tern_m5paper_touch_read`

Reference sources:
- `../M5EPD/src/M5EPD.cpp`
- `../M5EPD/src/M5EPD_Driver.cpp`
- `../M5EPD/src/utility/GT911.cpp`
- `../M5Paper_FactoryTest/src/systeminit.cpp`

Important board facts confirmed from the vendor code:
- `GPIO23` is EPD power enable, not reset
- bring-up order is main power -> ext power -> EPD power -> delay -> EPD begin
- GT911 uses I2C at 100 kHz on this board
- SD is initialized on the same SPI object used by the EPD driver

Current state:
- `m5paper_bridge_stub.c` is a placeholder backend
- Rust FFI lives in `m5paper/src/ffi.rs`
- enable with Cargo feature `cshim`

Next implementation step:
- replace the stub with a real C++ wrapper that builds a minimal subset of the M5EPD stack
- call the wrapper from Rust behind `#[cfg(feature = "cshim")]`
