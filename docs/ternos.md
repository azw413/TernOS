# TernOS Plan

## Goal

Build a single application platform with these properties:

- Palm OS 68k applications run through emulation.
- Native Tern applications run as pre-linked Rust modules.
- Both use the same underlying UI engine, event model, and as much OS surface as practical.
- Both are packaged and installed as PRC databases from `sdcard`.
- UI can render at Palm-native logical sizes such as `160x160` and `320x320`, and also scale to the full device framebuffer.

The target architecture is:

- thin 68k trap adapters
- a native Rust OS API as the canonical implementation
- PRC as the shared package/resource format
- a Tern-specific manifest resource for runtime and architecture selection

## Non-Goals

These are explicitly not the primary goal:

- producing PRCs that run on real Palm hardware
- carrying raw ESP32/RISC-V executable blobs loaded directly from PRC at runtime
- preserving Palm ABI internals as the canonical native API
- implementing every Palm OS trap before creating native app support

## End State

At the end of this plan, the system should support:

1. `palm68k` PRCs
   - launched through the 68k core
   - Palm traps adapt to native Rust subsystems

2. `tern-native` PRCs
   - launched through a native app registry
   - resources loaded from PRC
   - code linked into firmware
   - manifest identifies runtime, architecture, ABI, and entrypoint id

3. Shared UI runtime
   - forms, controls, fields, lists, tables, menus
   - one focus/navigation model
   - one rendering model
   - one event queue model

## Design Principles

### Canonical State Lives in Rust

Canonical state should live in Rust-native structs, not in emulated Palm memory.

Palm-facing structures should be treated as:

- ABI inputs
- ABI outputs
- compatibility projections

This is essential to keep 68k traps thin.

### Traps Are Adapters

A trap implementation should only:

1. decode Palm ABI arguments
2. resolve Palm pointers/handles into internal ids or data
3. call a Rust subsystem API
4. write results back into Palm-visible memory/registers

Trap code should not own business logic.

### PRC Is a Package Format, Not a Runtime Definition

PRC should remain the installable container format.

Runtime selection should come from Tern metadata inside the PRC, not from assumptions based on `type` or `creator` alone.

### Resolution Independence Is Mandatory

All UI components must work against logical coordinates and a renderer scale transform.

No widget should assume:

- a fixed physical framebuffer size
- a fixed 1:1 mapping from logical Palm pixels to device pixels

### Runtime Root Can Be Platform-Specific

The higher Tern layers should remain shared, but the lowest runtime root is allowed to differ by platform.

Current practical split:

- `desktop`
  - Rust owns the process root and runtime loop
- `x4`
  - Rust owns the process root and runtime loop
- `m5paper`
  - ESP-IDF/Arduino bridge owns `app_main()`
  - Rust is not currently the startup root

This is a build-system constraint, not a design goal.

The architectural rule is:

- startup ownership may differ
- policy, app state, rendering decisions, and shared UI concepts should still converge in shared layers
- platform-specific root code must stay thin

### One-Way Bridge Rule For M5Paper

For `m5paper`, the only stable integration model proven so far is:

- Rust may call the bridge/backend
- the bridge/backend must not call upward into Rust runtime code

Implications:

- `m5paper_bridge` is a backend service host
- it may own startup and hardware polling
- it must not become the place where long-term application logic lives
- temporary bridge-hosted screens are acceptable only as bootstrap/debug surfaces

## Target Module Layout

### Existing `tern_core`

The long-term split inside `core/src` should look like this:

- `prc_app/`
  - Palm PRC parsing and 68k runtime integration
- `ternos/`
  - new native OS/runtime layer
- `ui/`
  - shared rendering primitives and Palm-like component painting

### Current M5Paper Runtime Shape

`m5paper` currently uses:

- `m5paper/components/m5paper_bridge`
  - ESP-IDF/Arduino-backed hardware services
  - current runtime root
- `m5paper/src/ffi.rs`
  - Rust FFI wrapper
- `m5paper/src/platform.rs`
  - Rust platform wrapper aligned to the shared platform traits

The stable service surface already includes:

- EPD init / clear / region update
- touch init / read
- side button init / read
- input event queue
- RTC init / read / set
- storage init / exists / list / read

That is enough to keep the shared design moving even though Rust is not yet the on-device runtime root for this target.

### New `core/src/ternos/`

Create a new module tree:

- `ternos/mod.rs`
- `ternos/app.rs`
- `ternos/manifest.rs`
- `ternos/runtime.rs`
- `ternos/registry.rs`
- `ternos/services/`
- `ternos/ui/`
- `ternos/compat/`

Detailed purpose:

- `ternos/app.rs`
  - native app trait definitions
  - app lifecycle types
  - app context passed to native modules

- `ternos/manifest.rs`
  - parse and validate Tern manifest resource from PRC
  - runtime kind, architecture, ABI version, entrypoint id, capabilities

- `ternos/runtime.rs`
  - app session management
  - launch/close/app switching
  - event pump
  - service access

- `ternos/registry.rs`
  - map manifest entrypoint ids to built-in Rust apps
  - architecture and ABI checks

- `ternos/services/`
  - stable native APIs for OS services
  - `time.rs`
  - `prefs.rs`
  - `storage.rs`
  - `clipboard.rs`
  - `launcher.rs`
  - `resources.rs`

- `ternos/ui/`
  - canonical UI state and behavior
  - `form.rs`
  - `control.rs`
  - `field.rs`
  - `list.rs`
  - `table.rs`
  - `menu.rs`
  - `event.rs`
  - `render.rs`
  - `layout.rs`
  - `theme.rs`

- `ternos/compat/`
  - adapters for Palm semantics where they do not map 1:1
  - event translation
  - control style translation
  - Palm date/time format translation

## Shared UI Runtime

### Purpose

The shared UI runtime is the most important subsystem. It is the layer both Palm-emulated apps and native Rust apps should use.

It should own:

- active form
- form stack
- focused control
- focused field
- control state
- list state
- table state
- menu state
- help dialog state
- invalidation/redraw state
- event queue

### Core Types

Suggested public types:

```rust
pub struct UiRuntime {
    pub forms: FormStore,
    pub active_form: Option<FormId>,
    pub focus: FocusState,
    pub menu: MenuState,
    pub help: Option<HelpDialogState>,
    pub queue: EventQueue,
    pub invalidation: InvalidationState,
}

pub type FormId = u16;
pub type ObjectId = u16;
pub type ObjectIndex = u16;
```

### Event Model

Canonical event model:

```rust
pub enum UiEvent {
    Nil,
    FormLoad { form_id: FormId },
    FormOpen { form_id: FormId },
    FormClose { form_id: FormId },
    ControlSelect { control_id: ObjectId },
    FieldEnter { field_id: ObjectId },
    FieldChanged { field_id: ObjectId },
    KeyDown { chr: u16, key_code: u16, modifiers: u16 },
    PenDown { x: i16, y: i16 },
    PenUp { x: i16, y: i16 },
    PenMove { x: i16, y: i16 },
    TableSelect { table_id: ObjectId, row: u16, col: u16 },
    MenuCommand { item_id: u16 },
    AppStop,
}
```

Palm trap adapters should translate Palm `EventType` to and from this enum.

Native apps should receive this enum directly.

### Object Model

Suggested canonical object model:

```rust
pub enum UiObject {
    Label(LabelState),
    Button(ButtonState),
    Field(FieldState),
    List(ListState),
    Table(TableState),
    Bitmap(BitmapState),
    Title(TitleState),
}
```

Important rule:

- Palm resource parsing may produce a `FormPreviewObject`
- runtime should instantiate a richer mutable `UiObject`
- behavior should operate on `UiObject`, not on preview structs

### Rendering Model

Renderer input should be:

- logical form state
- logical viewport size
- output surface size
- theme and density
- device display capabilities

Suggested context:

```rust
pub struct UiRenderContext {
    pub logical_width: i32,
    pub logical_height: i32,
    pub surface_width: i32,
    pub surface_height: i32,
    pub scale_x: i32,
    pub scale_y: i32,
    pub density: UiDensity,
    pub theme: UiTheme,
}
```

### Device Display Capabilities

The UI runtime must explicitly model the display characteristics of the target device.

Suggested canonical type:

```rust
pub struct DisplayProfile {
    pub surface_width: u32,
    pub surface_height: u32,
    pub native_rotation: DisplayRotation,
    pub gray_levels: u8,
    pub bits_per_pixel: u8,
    pub has_partial_refresh: bool,
    pub preferred_refresh: RefreshStrategy,
    pub logical_style: LogicalStyle,
}
```

Suggested related enums:

```rust
pub enum LogicalStyle {
    Palm160,
    Palm320,
    TernPortrait,
    TernLandscape,
}

pub enum RefreshStrategy {
    FullOnly,
    PartialPreferred,
    Mixed,
}
```

This lets the renderer distinguish between:

- X4:
  - `surface = 480x800`
  - `gray_levels = 4`
  - portrait-first UI
  - more conservative grayscale rendering
- M5Paper:
  - `surface = 540x960`
  - `gray_levels = 16`
  - touch-first UI
  - richer grayscale rendering and finer text

Important rule:

- device capabilities are not theme choices
- theme controls look and spacing
- display profile controls physical rendering limits

### Logical Coordinates vs Physical Surface

The shared UI must render in logical coordinates first, then map to the physical surface.

There should be at least two logical coordinate spaces:

- Palm-compatible logical space
  - for emulated Palm forms and Palm-like native tools
- Tern-native logical space
  - for full-screen native apps and launchers

Suggested canonical viewport type:

```rust
pub struct LogicalViewport {
    pub width: i32,
    pub height: i32,
    pub style: LogicalStyle,
}
```

Examples:

- Palm app on M5Paper:
  - logical viewport may still be `160x160` or `320x320`
  - renderer centers/scales inside `540x960`
- Native launcher on X4:
  - logical viewport may be `480x800`
  - no Palm scaling step required
- Native app on M5Paper:
  - logical viewport may be `540x960`
  - higher-resolution layout and text allowed

This is the key abstraction that lets us use higher resolution without breaking Palm compatibility.

### Gray Depth and Color Policy

The renderer must target a logical grayscale palette, then quantize to the device profile.

Suggested model:

```rust
pub enum LogicalColor {
    Bg0,
    Bg1,
    Bg2,
    Bg3,
    Fg0,
    Fg1,
    Fg2,
    Fg3,
    Accent0,
    Accent1,
}
```

Then theme + display profile map that logical palette to actual device output.

Rules:

- Palm compatibility widgets should render acceptably at 1-bit, 2-bit, and 4-bit grayscale.
- Native Tern UI may use more tonal steps when available.
- Rendering code should not directly assume:
  - 4 shades
  - 16 shades
  - monochrome only

Instead:

1. paint into a logical grayscale buffer
2. quantize using `DisplayProfile.gray_levels`
3. apply device-specific dithering only as a presentation step

That keeps widget logic consistent across X4 and M5Paper.

### Typography Strategy

Typography should scale by both logical style and display profile.

Rules:

- Palm-emulated forms:
  - preserve Palm font ids and Palm metrics semantics as closely as possible
- Native Tern UI:
  - choose font sizes using physical surface and gray depth
- Small text on 16-gray M5Paper can be materially finer than on 4-gray X4
- Layout metrics should come from theme tables, not ad hoc per-screen code

Suggested API extension:

```rust
pub trait UiThemeApi {
    fn frame_metrics(&self, kind: FrameKind) -> FrameMetrics;
    fn control_metrics(&self, kind: ControlKind) -> ControlMetrics;
    fn font_for_id(
        &self,
        font_id: u8,
        density: UiDensity,
        display: &DisplayProfile,
    ) -> FontHandle;
}
```

### Asset Policy

Static assets should exist in logical form, not just device-specific bitmaps.

Recommended rules:

- icons and glyphs:
  - prefer vector-ish draw routines or multi-resolution generated assets
- UI chrome:
  - prefer procedural drawing from theme metrics
- app bitmaps:
  - allow multiple resource variants where needed

Suggested resource selection scheme:

- same logical asset id
- optional variants by:
  - density
  - grayscale depth
  - orientation

For example:

- Palm launcher icon variant for `Palm160`
- native launcher icon variant for `TernPortrait`

### Layout Policy Across Devices

There are two layout modes the system must support:

1. Compatibility layout
   - exact or near-exact Palm geometry
   - used for Palm apps and Palm-style dialogs

2. Adaptive Tern layout
   - responsive to `DisplayProfile`
   - used for launcher, browser, reader, settings, and native apps

The mistake to avoid is trying to stretch Palm layouts into full-screen native layouts automatically.

Instead:

- Palm UI keeps Palm logical geometry
- native Tern UI gets its own adaptive layout system

### Rendering Pipeline

Recommended pipeline:

1. choose `LogicalViewport`
2. construct `UiRenderContext`
3. paint to a logical grayscale scene buffer
4. scale/composite into the physical surface buffer
5. quantize to device gray depth
6. submit with device refresh policy

This is important because X4 and M5Paper differ in both:

- physical resolution
- usable grayscale depth

The pipeline should make those differences explicit, not incidental.

### Density and Scaling

Support at least:

- `Palm160`
- `Palm320`
- `DeviceNative`

Suggested enum:

```rust
pub enum UiDensity {
    Palm160,
    Palm320,
    DeviceNative,
}
```

Rules:

- form/object coordinates stay in logical units
- renderer applies scale and density transforms
- stroke widths and font choices come from theme metrics, not hardcoded per widget
- hit testing should work in logical units
- density is not a substitute for display profile
- density selects a logical UI family; display profile selects physical presentation limits

### Theme

The theme should encode Palm-like appearance variants, for example:

- classic monochrome Palm theme
- double-density Palm theme
- Tern full-resolution theme

Suggested theme API:

```rust
pub trait UiThemeApi {
    fn frame_metrics(&self, kind: FrameKind) -> FrameMetrics;
    fn control_metrics(&self, kind: ControlKind) -> ControlMetrics;
    fn font_for_id(&self, font_id: u8, density: UiDensity) -> FontHandle;
}
```

### Device Adaptation Rules

Concrete rules for current targets:

#### X4

- preferred native Tern layout: portrait `480x800`
- target gray depth: 4 levels
- avoid over-light intermediate tones for small text
- prefer stronger contrast and simpler fills

#### M5Paper

- preferred native Tern layout: portrait `540x960`
- target gray depth: 16 levels
- permit finer borders, softer fills, and smaller antialiased text
- touch hit targets should still be sized in logical UI units, not raw pixels

#### Desktop

- may emulate Palm logical displays
- may also render native Tern layouts at arbitrary window sizes
- should expose scaling/debug modes so layout assumptions are visible early

## Palm Compatibility Layer

### Trap File Split

`core/src/prc_app/trap_stub.rs` should be broken into:

- `prc_app/traps/dispatch.rs`
- `prc_app/traps/abi.rs`
- `prc_app/traps/helpers.rs`
- `prc_app/traps/sys.rs`
- `prc_app/traps/mem.rs`
- `prc_app/traps/dm.rs`
- `prc_app/traps/evt.rs`
- `prc_app/traps/frm.rs`
- `prc_app/traps/ctl.rs`
- `prc_app/traps/fld.rs`
- `prc_app/traps/lst.rs`
- `prc_app/traps/tbl.rs`
- `prc_app/traps/win.rs`
- `prc_app/traps/fnt.rs`
- `prc_app/traps/str.rs`
- `prc_app/traps/tim.rs`

### ABI Module Responsibilities

`abi.rs` should own:

- stack argument decoding
- 24-bit pointer normalization
- handle/pointer resolution helpers
- event write helpers
- common structure read/write helpers

That logic should not be duplicated in subsystem modules.

### Trap-to-Native Flow

Example for `FldHandleEvent`:

1. decode `FieldType*` and `EventType*`
2. resolve to native `form_id` and `field_id`
3. convert Palm event to `UiEvent`
4. call `ternos::ui::field::handle_event(...)`
5. sync text/selection/insertion point back to Palm memory if required
6. set return register to Palm-compatible handled/not-handled value

The same pattern should be used for:

- `Frm*`
- `Ctl*`
- `Lst*`
- `Tbl*`

## Native OS API

### App Trait

Native applications should be pre-linked Rust modules implementing a small trait.

Suggested trait:

```rust
pub trait NativeApp {
    fn app_id(&self) -> &'static str;
    fn launch(&mut self, ctx: &mut AppContext, req: LaunchRequest) -> AppResult<()>;
    fn handle_event(&mut self, ctx: &mut AppContext, event: UiEvent) -> AppResult<AppFlow>;
    fn suspend(&mut self, ctx: &mut AppContext) -> AppResult<()>;
    fn resume(&mut self, ctx: &mut AppContext) -> AppResult<()>;
    fn close(&mut self, ctx: &mut AppContext) -> AppResult<()>;
}
```

### App Context

`AppContext` should expose services, not raw internals.

Suggested shape:

```rust
pub struct AppContext<'a> {
    pub ui: &'a mut UiFacade,
    pub prefs: &'a mut dyn PrefsService,
    pub time: &'a mut dyn TimeService,
    pub storage: &'a mut dyn StorageService,
    pub resources: &'a dyn ResourceService,
    pub launcher: &'a mut dyn LauncherService,
}
```

### Stable Native Service APIs

Define stable traits first.

Examples:

```rust
pub trait TimeService {
    fn now_seconds(&self) -> u32;
    fn set_seconds(&mut self, seconds: u32) -> Result<(), OsError>;
    fn date_to_ascii(&self, date: Date, fmt: DateFormat) -> String;
    fn time_to_ascii(&self, time: Time, fmt: TimeFormat) -> String;
}

pub trait PrefsService {
    fn get_u16(&self, key: PrefKey) -> u16;
    fn set_u16(&mut self, key: PrefKey, value: u16) -> Result<(), OsError>;
}
```

These are the canonical implementations that Palm traps should call under the covers.

## PRC Format Strategy

### Standard PRC Outer Container

Keep standard PRC database structure for:

- discovery
- installation
- metadata
- resource storage
- launcher integration

This means existing PRCs remain installable.

### Tern Manifest Resource

Add a Tern-specific manifest resource.

Recommended resource type:

- `TMNF`

Resource id:

- `0`

### `TMNF` Fields

Suggested binary manifest contents:

```text
magic        4 bytes   = 'TMNF'
version      u16       = 1
runtime      u16       = 0 palm68k, 1 tern-native
arch         u16       = 0 any, 1 xtensa, 2 riscv32, 3 host
abi_version  u16       = tern native ABI version
entry_id     u32       = native app registry entry id
flags        u32       = capability / behavior flags
min_os       u16       = minimum TernOS runtime version
pref_form    u16       = preferred initial form id, optional
reserved     ...
```

The important fields are:

- `runtime`
- `arch`
- `abi_version`
- `entry_id`

### Runtime Values

Suggested values:

```text
0 = palm68k
1 = tern-native
```

### Architecture Values

Suggested values:

```text
0 = any
1 = xtensa
2 = riscv32
3 = host-debug
```

This is Tern-defined metadata, not standard Palm metadata.

### Launch Decision Logic

When opening a PRC:

1. parse normal PRC metadata
2. look for `TMNF/0`
3. if no `TMNF`
   - default to legacy Palm behavior
   - treat as `palm68k`
4. if `TMNF.runtime == palm68k`
   - run emulator path
5. if `TMNF.runtime == tern-native`
   - verify architecture compatibility
   - verify ABI compatibility
   - resolve `entry_id` in native registry
   - run native path

### Resource Sharing

Both Palm and native PRCs should be able to use:

- `tFRM`
- `tBTN`
- `tFLD`
- `tLBL`
- `tSTR`
- `tBMP`
- table/list/menu resources

The difference is execution backend, not resource model.

## Native App Registry

### Why Registry-Based Native Apps

Pre-linked native is the correct first implementation for safety and simplicity.

The PRC should not contain raw native machine code loaded at runtime.

Instead:

- PRC manifest contains `entry_id`
- firmware contains a registry mapping `entry_id -> NativeAppFactory`

Suggested API:

```rust
pub struct NativeAppDescriptor {
    pub entry_id: u32,
    pub app_id: &'static str,
    pub abi_version: u16,
    pub arch: NativeArch,
    pub create: fn() -> Box<dyn NativeApp>,
}
```

Registry API:

```rust
pub trait NativeAppRegistry {
    fn find(&self, entry_id: u32) -> Option<&NativeAppDescriptor>;
}
```

## Launcher and Install Pipeline

### Install

Installer should continue to accept PRCs copied to SD card.

During catalog/import:

- parse standard PRC header
- parse `TMNF` if present
- record runtime kind and architecture in catalog metadata
- extract icon/title resources as normal

### Launcher Presentation

Launcher should show:

- Palm apps
- Tern-native apps
- control panels / panels

with runtime-specific compatibility checks.

If architecture is incompatible, show disabled entry or compatibility message.

## API Surface Mapping

### Palm APIs That Should Map Directly to Native Services

These should become thin wrappers early:

- `TimGetSeconds`
- `DateToAscii`
- `TimeToAscii`
- `PrefGetPreference`
- `PrefSetPreference`
- `Frm*`
- `Ctl*`
- `Fld*`
- `Lst*`
- `Tbl*`

### Palm APIs That Need Compatibility Adapters

These need more care:

- `MemHandle*`
- `MemPtr*`
- `Dm*`
- low-level graphics/window APIs

These should still call native backends where possible, but the Palm adapter will remain thicker.

## Migration Plan

### Phase 1: Trap Refactor

1. split `trap_stub.rs` by subsystem
2. move common ABI helpers into `traps/abi.rs`
3. keep behavior unchanged where possible
4. add subsystem-level tests/logging

Deliverable:

- modular trap code with no behavior regressions

### Phase 2: Shared UI Runtime Extraction

1. create `ternos/ui` canonical state structs
2. move form/control/field/list/table/menu logic out of trap code
3. make renderer consume canonical UI state
4. keep Palm trap adapters writing into and reading from this state

Deliverable:

- Palm apps still run
- UI behavior now owned by native Rust modules

### Phase 3: Native Service Layer

1. create `ternos/services`
2. move time/prefs/string formatting into native services
3. change Palm traps to call native service traits
4. reduce Palm-specific logic in trap implementations

Deliverable:

- thin service traps
- reusable APIs for native apps

### Phase 4: Tern Manifest and Runtime Selection

1. define `TMNF`
2. add parser and validator
3. extend install catalog to record manifest metadata
4. route app launch to `palm68k` or `tern-native`

Deliverable:

- shared PRC package format with explicit runtime selection

### Phase 5: Native App Registry

1. define `NativeApp` trait
2. implement registry lookup by `entry_id`
3. create `ternos/runtime` native app session management
4. add first built-in native PRC app using shared resources

Deliverable:

- first pre-linked native app launched from a PRC manifest

### Phase 6: Resolution-General UI

1. formalize logical coordinate system
2. centralize scale/density in renderer
3. remove ad hoc widget scaling rules from individual draw paths
4. verify `160x160`, `320x320`, and full device rendering

Deliverable:

- same form resource renders correctly across densities

### Phase 7: Replace Home UI with Shared UI Runtime

1. migrate launcher/home UI to `ternos/ui`
2. stop treating home as a separate UI system
3. keep only platform shell concerns outside shared UI runtime

Deliverable:

- all app-facing UI uses one engine

## Testing Strategy

### Unit Tests

Add unit tests for:

- manifest parsing
- runtime selection
- field editing behavior
- control selection behavior
- list/trigger behavior
- table state updates
- event translation
- focus navigation across densities

### Integration Tests

Create scenario tests for:

- Timesheet
- Date & Time panel
- To Do List
- one native Tern app using shared forms/resources

### Golden Rendering Tests

Add bitmap snapshot tests for:

- `160x160`
- `320x320`
- device-native scale

for key forms and dialogs.

## Immediate Next Steps

The next practical work should be:

1. split `trap_stub.rs` into subsystem modules without changing behavior
2. create `ternos/ui/event.rs` and `ternos/ui/runtime.rs`
3. move field logic into shared native state first
4. then move control/trigger/list logic
5. define `TMNF` parser and data structures

That order keeps current app progress moving while aligning the codebase with the final architecture.

## Concrete Implementation Checklist

This section maps the plan onto the current codebase so work can start without further architecture translation.

### Step 1: Split Palm Trap Dispatch

Goal:

- keep behavior unchanged
- reduce `trap_stub.rs` into a dispatcher plus subsystem modules

Current files:

- `core/src/prc_app/trap_stub.rs`
- `core/src/prc_app/traps/table.rs`
- `core/src/prc_app/runtime.rs`

Target files:

- `core/src/prc_app/traps/dispatch.rs`
- `core/src/prc_app/traps/abi.rs`
- `core/src/prc_app/traps/helpers.rs`
- `core/src/prc_app/traps/sys.rs`
- `core/src/prc_app/traps/mem.rs`
- `core/src/prc_app/traps/dm.rs`
- `core/src/prc_app/traps/evt.rs`
- `core/src/prc_app/traps/frm.rs`
- `core/src/prc_app/traps/ctl.rs`
- `core/src/prc_app/traps/fld.rs`
- `core/src/prc_app/traps/lst.rs`
- `core/src/prc_app/traps/tbl.rs`
- `core/src/prc_app/traps/win.rs`
- `core/src/prc_app/traps/fnt.rs`
- `core/src/prc_app/traps/str.rs`
- `core/src/prc_app/traps/tim.rs`

Checklist:

- move pointer decoding and event writing helpers from `trap_stub.rs` into `traps/abi.rs`
- move trap-name lookup and group classification to remain under `traps/table.rs` or rename it to `traps/catalog.rs`
- keep `is_prc_runtime_trap_handled(...)` and trap dispatch in `traps/dispatch.rs`
- re-export the dispatcher from `core/src/prc_app/mod.rs`
- preserve existing logs during the split

Recommended order:

1. `abi.rs`
2. `evt.rs`
3. `frm.rs`
4. `fld.rs`
5. `ctl.rs`
6. `lst.rs`
7. `tbl.rs`
8. remaining service families

### Step 2: Introduce `ternos` Skeleton

Goal:

- create the native API home before moving logic into it

New files to create:

- `core/src/ternos/mod.rs`
- `core/src/ternos/app.rs`
- `core/src/ternos/runtime.rs`
- `core/src/ternos/registry.rs`
- `core/src/ternos/manifest.rs`
- `core/src/ternos/services/mod.rs`
- `core/src/ternos/ui/mod.rs`
- `core/src/ternos/ui/event.rs`
- `core/src/ternos/ui/runtime.rs`

Checklist:

- add `pub mod ternos;` to `core/src/lib.rs`
- keep these modules mostly type definitions at first
- avoid moving Palm code into them yet

Minimum API to define first:

```rust
pub enum RuntimeKind {
    Palm68k,
    TernNative,
}

pub enum NativeArch {
    Any,
    Xtensa,
    Riscv32,
    Host,
}

pub trait NativeApp {
    fn app_id(&self) -> &'static str;
}
```

### Step 3: Establish Canonical UI Event Type

Goal:

- stop Palm `EventType` from being the only UI event model

Current files:

- `core/src/prc_app/runtime.rs`
- `core/src/prc_app/runner.rs`
- `core/src/prc_app/controller.rs`
- `core/src/application.rs`

New file:

- `core/src/ternos/ui/event.rs`

Checklist:

- define `UiEvent`
- define conversion helpers from Palm event ids to `UiEvent`
- define conversion helpers from desktop input to `UiEvent`
- do not remove Palm event ids yet; wrap them

First Palm mappings to implement:

- `EVT_FRM_LOAD`
- `EVT_FRM_OPEN`
- `EVT_CTL_SELECT`
- `EVT_FLD_ENTER`
- `EVT_FLD_CHANGED`
- `EVT_KEY_DOWN`

### Step 4: Move Field Logic First

Goal:

- prove the adapter architecture on the smallest useful UI subsystem

Current files:

- `core/src/prc_app/trap_stub.rs`
- `core/src/prc_app/runner.rs`
- `core/src/prc_app/runtime.rs`
- `core/src/prc_app/ui.rs`

New files:

- `core/src/ternos/ui/field.rs`
- `core/src/ternos/ui/runtime.rs`

Checklist:

- define canonical `FieldState`
- move insertion point, selection, and text mutation logic into `ternos/ui/field.rs`
- keep Palm field structs as compatibility storage only
- update `FldHandleEvent` trap to call native field functions
- keep `RuntimeUiSnapshot.field_draws` working during migration

Minimal canonical type:

```rust
pub struct FieldState {
    pub form_id: u16,
    pub field_id: u16,
    pub text: alloc::string::String,
    pub ins_pt: u16,
    pub sel_start: u16,
    pub sel_end: u16,
    pub editable: bool,
}
```

Success condition:

- desktop keyboard input updates a selected field through native field logic
- Palm trap only decodes and syncs

### Step 5: Move Controls, Triggers, and Lists

Goal:

- make panels like Date & Time work through native control logic

Current files:

- `core/src/prc_app/form_preview.rs`
- `core/src/prc_app/controller.rs`
- `core/src/prc_app/ui.rs`
- `core/src/prc_app/trap_stub.rs`

New files:

- `core/src/ternos/ui/control.rs`
- `core/src/ternos/ui/list.rs`
- `core/src/ternos/ui/form.rs`

Checklist:

- define canonical control kinds:
  - push button
  - button
  - selector trigger
  - checkbox
  - repeating button
  - graphic button
- add label/value state to controls
- move `Ctl*` and `Lst*` behavior into native functions
- keep `FormPreviewObject` as parse-time representation only

Important immediate need:

- style `4` controls in Date & Time are trigger-like controls and should become focusable/selectable native controls

### Step 6: Replace `FormPreview` Runtime Dependence

Goal:

- separate parsed resources from live UI state

Current files:

- `core/src/prc_app/form_preview.rs`
- `core/src/application.rs`
- `core/src/prc_app/runner.rs`

Problem today:

- runtime rendering and focus navigation still rely heavily on `FormPreview`
- parsed resource objects and live state are too tightly coupled

Target split:

- `FormPreview`
  - immutable parse result
- `UiFormState`
  - mutable instantiated runtime form

Checklist:

- add a form-instantiation step: `FormPreview -> UiFormState`
- have traps and native apps mutate `UiFormState`
- make renderer consume `UiFormState`
- keep `FormPreview` available for launcher previews and debugging

### Step 7: Introduce `TMNF` Manifest Parsing

Goal:

- make runtime choice explicit and future-proof

Current files:

- `core/src/prc_app/prc.rs`
- `desktop/src/image_source.rs`
- `x4/src/image_source.rs`

New files:

- `core/src/ternos/manifest.rs`

Checklist:

- define `TernManifest`
- parse `TMNF/0` from a resource PRC
- expose:
  - runtime kind
  - architecture
  - ABI version
  - entry id
- keep absence of `TMNF` meaning legacy Palm app

Suggested type:

```rust
pub struct TernManifest {
    pub runtime: RuntimeKind,
    pub arch: NativeArch,
    pub abi_version: u16,
    pub entry_id: u32,
    pub min_os: u16,
    pub preferred_form: Option<u16>,
}
```

### Step 8: Add Native Runtime Selection Path

Goal:

- open PRCs either as Palm 68k or native apps

Current files:

- `core/src/application.rs`
- `desktop/src/image_source.rs`
- `x4/src/image_source.rs`

Checklist:

- extend PRC metadata loading to look for `TMNF`
- teach launcher/catalog about runtime kind and arch
- add runtime branch in `Application::open_prc_entry(...)` and `open_prc_path(...)`
- keep Palm path untouched for legacy PRCs

### Step 9: Add Native App Registry

Goal:

- support pre-linked native apps from PRC manifests

New files:

- `core/src/ternos/registry.rs`
- one first native app module under `core/src/ternos/apps/`

Checklist:

- define `NativeAppDescriptor`
- define registry lookup by `entry_id`
- add one test native app that uses shared resources
- verify PRC launch through native path

Suggested first native app candidates:

- a simple Preferences-style test panel
- a diagnostics panel
- a clock/date setter panel

### Step 10: Generalize Rendering for Density

Goal:

- remove widget-specific ad hoc scaling

Current files:

- `core/src/prc_app/ui.rs`
- `core/src/ui/*.rs`
- `core/src/application.rs`

Checklist:

- add explicit render context with logical size and target size
- move scale calculations out of individual widgets where possible
- centralize frame/button/field metrics
- verify monochrome Palm look at `160x160`
- verify sharper layout at `320x320`
- verify full-device mode does not distort hit testing

## First Milestone Deliverables

The first milestone should stop at this point:

1. trap modules split
2. `ternos` skeleton exists
3. `UiEvent` exists
4. field logic moved to native state
5. trigger/control logic moved enough for Date & Time to operate

That is the right cut because it proves:

- thin trap adapter direction
- shared UI runtime direction
- practical support for control-panel style apps

## Suggested Work Sequence For The Next Few Sessions

Session 1:

- split `trap_stub.rs` into `dispatch.rs`, `abi.rs`, `evt.rs`, `frm.rs`, `fld.rs`

Session 2:

- create `ternos` skeleton
- define `UiEvent`
- define `FieldState`

Session 3:

- move field editing logic to `ternos/ui/field.rs`
- adapt `FldHandleEvent`

Session 4:

- define `ControlState`
- move trigger/button selection behavior for Date & Time

Session 5:

- define and parse `TMNF`
- add runtime selection metadata to install/catalog code

This sequencing keeps app-visible progress while reducing architectural debt.

## Summary

The endpoint is:

- PRC remains the install/package/resource format
- `TMNF` defines runtime and architecture
- `palm68k` apps run through emulation
- `tern-native` apps run through a pre-linked registry
- a shared Rust UI/runtime layer implements the real behavior
- Palm traps become thin adapters over that layer
- the same UI resources and logical layouts render at Palm-native and full device resolutions

This is a realistic architecture for TernReader and does not require abandoning Palm compatibility to gain a clean native application model.
