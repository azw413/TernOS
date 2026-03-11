# M5Paper ESP-IDF Experiment

This crate is an isolated experiment for an ESP-IDF-backed M5Paper target.

Purpose:
- test whether an ESP-IDF/Arduino-style runtime is a better foundation for the M5Paper board
- keep the existing `m5paper` no_std/esp-hal path intact while we experiment
- reuse the `m5paper/cshim` FFI boundary later if we bridge to known-good M5EPD code

Current scope:
- power up GPIO2 / GPIO5 / GPIO23
- log M5Paper board-state basics
- prepare a place to enable the `cshim` feature later

Build target:
- `xtensa-esp32-espidf`
