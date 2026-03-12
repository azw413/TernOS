# Platform Abstraction Plan

## Goal

Define a hardware abstraction layer that allows TernOS to run on multiple targets without changing OS, PRC, UI, or app logic.

Initial supported targets:

- `desktop`
- `x4`
- `m5paper`

The `m5paper` target refers to the touchscreen ESP32 E-Ink device.

## Design Constraints

The platform layer must support:

- different display sizes and densities
- button-only devices
- touch devices
- host debugging on desktop
- SD-card or filesystem-backed storage
- RTC/time services
- power/sleep control
- future network-capable devices without forcing networking into the core

The platform layer must not force `tern_core` to know about:

- GPIO pins
- SPI/I2C/UART instances
- specific display controllers
- specific touch controllers
- ESP-IDF or `esp-hal` details
- desktop windowing APIs

## Target Architecture

Split the system into three layers:

1. `tern_core`
   - OS/runtime/app model
   - PRC install/load/runtime selection
   - Palm compatibility layer
   - shared UI runtime
   - framebuffer rendering

2. platform API
   - traits and shared hardware-facing event types
   - stable contract used by `tern_core`

3. platform implementations
   - `desktop`
   - `x4`
   - `m5paper`

## Recommended Structure

Near-term, this can be done as modules. Long-term, separate crates are cleaner.

### Preferred long-term crate layout

- `tern_core`
- `tern_platform_api`
- `tern_platform_desktop`
- `tern_platform_x4`
- `tern_platform_m5paper`

### Acceptable short-term layout

Inside the current repository:

- `core/src/platform/`
- `desktop/` implements the platform traits
- `x4/` implements the platform traits
- `m5paper/` new binary crate implementing the platform traits

## M5Paper Constraint

`m5paper` is currently the one target where the runtime root differs from the cleaner Rust-owned model.

Working arrangement:

- `m5paper_bridge` owns `app_main()`
- the bridge starts and hosts backend services
- Rust consumes those services through FFI and platform wrappers

Non-working arrangement so far:

- C/C++ bridge code calling upward into Rust runtime entrypoints
- Rust directly owning the ESP-IDF `app_main()` root under the current `esp-idf-sys` + local-component build arrangement

So the current rule is:

- keep the bridge backend-only
- keep the service API shaped for Rust consumption
- avoid C++ -> Rust callbacks
- preserve the same higher-level concepts as `x4`, even if the root entry differs

This is a temporary platform-specific compromise, not a separate platform architecture.

## Core Principle

The platform layer should expose **capabilities and normalized events**, not board identities.

The OS should never branch on:

- “if x4”
- “if m5paper”

It should branch only on capabilities such as:

- has touch
- has buttons
- supports partial refresh
- supports grayscale
- supports RTC writeback

## Platform Capabilities

Define a capability struct:

```rust
pub struct PlatformCaps {
    pub has_touch: bool,
    pub has_buttons: bool,
    pub has_keyboard: bool,
    pub has_wifi: bool,
    pub has_bluetooth: bool,
    pub supports_partial_refresh: bool,
    pub supports_grayscale: bool,
    pub supports_sleep: bool,
    pub supports_rtc_set: bool,
}
```

This should be queryable at runtime from the platform.

## Normalized Input Model

The most important abstraction is normalized input.

All board-specific inputs should be converted into one stream of platform input events.

Suggested event type:

```rust
pub enum PlatformInputEvent {
    ButtonDown(ButtonId),
    ButtonUp(ButtonId),
    TouchDown { x: i32, y: i32 },
    TouchMove { x: i32, y: i32 },
    TouchUp { x: i32, y: i32 },
    KeyDown { chr: u16, key_code: u16, modifiers: u16 },
    KeyUp { key_code: u16 },
    Tick,
}
```

Suggested button ids:

```rust
pub enum ButtonId {
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    Power,
    Menu,
}
```

Rules:

- `desktop` emits keyboard events and synthesized directional/button events
- `x4` emits directional/button events
- `m5paper` emits touch events and any available hardware buttons
- the OS consumes the same `PlatformInputEvent` regardless of source

## Display Abstraction

The display path should remain framebuffer-oriented.

The OS should render into an abstract framebuffer or draw target. The platform is responsible for presenting it.

Suggested trait:

```rust
pub trait DisplayDevice {
    fn size_px(&self) -> (u32, u32);
    fn logical_density(&self) -> UiDensity;
    fn caps(&self) -> DisplayCaps;
    fn present(&mut self, frame: &FrameBuffer, mode: RefreshMode);
}
```

Suggested caps:

```rust
pub struct DisplayCaps {
    pub partial_refresh: bool,
    pub grayscale: bool,
    pub rotation: DisplayRotation,
}
```

Notes:

- `present(...)` should hide panel-specific waveform/update behavior
- `RefreshMode` stays a logical request from TernOS
- the board implementation maps it to the physical display controller

## Touch Abstraction

Touch hardware should not be exposed directly to apps.

The platform translates touch controller data into `PlatformInputEvent::{TouchDown, TouchMove, TouchUp}`.

The OS layer then converts that into:

- Palm pen events for `palm68k`
- `UiEvent::PenDown/PenMove/PenUp` for native apps

Touch coordinates should be normalized into display logical space before they reach the OS.

That means board code owns:

- axis inversion
- calibration
- rotation compensation
- raw-to-screen mapping

## Time Abstraction

Suggested trait:

```rust
pub trait ClockDevice {
    fn monotonic_ms(&self) -> u64;
    fn rtc_seconds(&self) -> u32;
    fn set_rtc_seconds(&mut self, value: u32) -> Result<(), PlatformError>;
}
```

Rules:

- `monotonic_ms()` drives runtime tick scheduling
- `rtc_seconds()` backs Palm time/date traps and native time APIs
- `set_rtc_seconds()` is optional in capability terms, but should be present on devices that support RTC updates

## Storage Abstraction

The OS should not know whether files come from:

- host filesystem
- FAT on SD card
- flash-backed VFS

Suggested trait:

```rust
pub trait StorageDevice {
    fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError>;
    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), PlatformError>;
    fn list(&self, path: &str) -> Result<Vec<StorageEntry>, PlatformError>;
    fn exists(&self, path: &str) -> bool;
}
```

This should sit below the existing app/image source abstractions.

## Power Abstraction

Suggested trait:

```rust
pub trait PowerDevice {
    fn battery_status(&self) -> BatteryStatus;
    fn sleep(&mut self, mode: SleepMode) -> Result<(), PlatformError>;
}
```

The OS should request sleep; the platform decides how to implement it.

Examples:

- `desktop`: no-op or simulated sleep
- `x4`: existing sleep path
- `m5paper`: touchscreen wake/suspend implementation

## Platform Facade

The simplest shape for the OS to depend on is one facade aggregating devices.

Suggested trait:

```rust
pub trait Platform {
    type Display: DisplayDevice;
    type Clock: ClockDevice;
    type Storage: StorageDevice;
    type Power: PowerDevice;

    fn caps(&self) -> PlatformCaps;
    fn display(&mut self) -> &mut Self::Display;
    fn clock(&mut self) -> &mut Self::Clock;
    fn storage(&mut self) -> &mut Self::Storage;
    fn power(&mut self) -> &mut Self::Power;
    fn poll_input(&mut self, sink: &mut dyn FnMut(PlatformInputEvent));
}
```

This allows a single OS runtime loop like:

```rust
platform.poll_input(&mut |event| os.handle_input(event));
os.tick(platform.clock().monotonic_ms());
os.render_if_needed(platform.display());
```

## Integration With TernOS

### OS Runtime Expectations

`tern_core` or future `ternos` runtime should consume only:

- normalized input events
- a framebuffer/display target
- clock service
- storage service
- power service

It should not talk to board implementations directly.

### Palm Runtime Expectations

The Palm emulator path should also sit on the same platform services.

Examples:

- Palm `TimGetSeconds` trap uses `ClockDevice`
- Palm `PrefGetPreference` uses OS pref service backed by platform storage if necessary
- Palm pen events on `m5paper` come from normalized touch input

## Platform-Specific Responsibilities

### `desktop`

Responsibilities:

- keyboard input
- optional mouse/touch simulation
- host filesystem access
- desktop window display backend
- host monotonic clock
- simulated RTC persistence if needed

Should map:

- keyboard arrows / enter / backspace to button and key events
- mouse to touch/pen events

### `x4`

Responsibilities:

- current E-Ink display implementation
- current button GPIO handling
- SD card filesystem
- RTC/time source
- battery and sleep handling

Should remain the reference button-only hardware target.

### `m5paper`

Responsibilities:

- E-Ink display driver integration
- touch controller integration
- coordinate calibration and transform
- storage integration
- RTC/time integration
- power/suspend/wake integration
- optional Wi-Fi/Bluetooth exposure as capabilities only, not as core OS requirements

Important rule:

- touch calibration and controller quirks stay in `m5paper`, not in `tern_core`

## Migration Plan From Current Code

### Step 1: Introduce Platform Types

Add new module:

- `core/src/platform/mod.rs`

Initial contents:

- `PlatformCaps`
- `PlatformInputEvent`
- `ButtonId`
- `DisplayCaps`
- `PlatformError`
- initial device traits

This is just type introduction. No behavior move yet.

### Step 2: Adapt `desktop`

Current files likely involved:

- `desktop/src/display.rs`
- `desktop/src/image_source.rs`
- `desktop/src/main.rs`

Tasks:

- wrap current display code behind `DisplayDevice`
- replace `ButtonState`-only assumption with event emission
- keep current keyboard passthrough logic, but emit normalized events
- keep existing filesystem-backed behavior, but present it through storage/device traits

### Step 3: Adapt `x4`

Current files likely involved:

- `x4/src/main.rs`
- `x4/src/input.rs`
- `x4/src/eink_display.rs`
- `x4/src/image_source.rs`

Tasks:

- wrap E-Ink display behind `DisplayDevice`
- convert button polling into `PlatformInputEvent`
- expose storage/time/power through the platform facade

### Step 4: Change OS Entry Path

Current code:

- `Application::update(&ButtonState, elapsed_ms)` assumes button snapshot input

Target:

- move toward event-driven input handling

Recommended transition:

1. keep `ButtonState` temporarily for compatibility
2. add `handle_platform_event(...)`
3. route desktop and x4 input through both until migration is complete
4. then remove direct `ButtonState` dependency from the app runtime

### Step 5: Add `m5paper` Target

Create a new target crate:

- `m5paper/`

Suggested initial files:

- `m5paper/src/main.rs`
- `m5paper/src/platform.rs`
- `m5paper/src/display.rs`
- `m5paper/src/touch.rs`
- `m5paper/src/storage.rs`
- `m5paper/src/power.rs`
- `m5paper/src/image_source.rs`

First milestone for `m5paper`:

- boot to launcher
- render framebuffer
- touch produces normalized events
- tap can select launcher items

## API Stability Rules

To avoid rework, adopt these rules early:

- platform traits should be additive, not rewritten casually
- board-specific quirks should never leak into `tern_core`
- all input gets normalized before reaching the OS
- all display drivers receive complete frames or logical refresh requests, not widget-level draw commands
- platform-specific async/concurrency details stay in the board crate

## Suggested Near-Term Deliverables

### Milestone A

- `core/src/platform/mod.rs` exists
- normalized input event types exist
- `desktop` implements the display and input portions

### Milestone B

- `x4` implements the same platform traits
- app runtime can be driven through normalized input events

### Milestone C

- `m5paper` crate exists
- display and touch are integrated
- launcher usable by touch

### Milestone D

- Palm apps receive pen events on touch devices
- native apps receive the same touch stream through shared UI runtime

## Interaction With Native PRCs

This platform layer fits directly with the Tern-native PRC plan.

The flow becomes:

1. platform provides storage/input/display/time/power
2. TernOS discovers PRCs from storage
3. PRC manifest selects `palm68k` or `tern-native`
4. app runs through shared OS/UI/runtime
5. framebuffer is presented by the platform display backend
6. input events originate from buttons, touch, or keyboard through the same platform event type

That means `m5paper` support does not require any change to PRC format design.

## Immediate Next Steps

1. create `docs/platform.md`
2. add `core/src/platform/mod.rs` with the first trait and event definitions
3. adapt `desktop` to emit `PlatformInputEvent`
4. adapt `x4` to the same input/display interfaces
5. create `m5paper/` as a new target crate once the common API is stable enough

## Summary

The correct abstraction is:

- one OS/runtime/app model
- one UI system
- one PRC packaging model
- multiple platform backends

The board-specific work for `m5paper` should live entirely in its platform crate. TernOS should only see normalized events, capabilities, and services.
