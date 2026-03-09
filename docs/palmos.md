# PalmOS Runtime + Database Plan

## Why This Exists

We are building a PalmOS-compatible runtime path for TernReader.
This document captures:

- what we currently support (UI + app runtime),
- what is deterministic in PalmOS launch,
- and how we should move from direct PRC execution to Palm-style installed databases.

## Deterministic vs App-Specific

- Deterministic (OS lifecycle):
  - System launches an app with a launch command (`sysAppLaunchCmd*`), including `sysAppLaunchCmdNormalLaunch`.
  - `SysAppStartup` / `SysAppExit` participate in launch/teardown around app execution.
- App-specific:
  - Internal control flow from entry to event loop.
  - How/when forms are initialized and event handlers installed.
  - Whether `code` resource `0` is trampoline/glue and where real app code starts.

Conclusion: lifecycle is deterministic at the OS contract level, but entry wiring inside PRCs is not uniform enough to hardcode one offset for all apps.

## POSE References

Primary reference in local clone `../pose`:

- `../pose/SrcShared/Patches/EmPatchModuleSys.cpp:2704`
  - `SysTailpatch::SysAppStartup` patch path.
  - Reads `SysAppInfoType.cmd` and tracks launch context.
  - Explicitly checks `cmd == sysAppLaunchCmdNormalLaunch`.

- `../pose/SrcShared/Patches/EmPatchModuleSys.cpp:1402`
  - `SysHeadpatch::SysAppExit` path.
  - Reads launch command + flags and performs cleanup/leak checks.
  - Also treats `sysAppLaunchCmdNormalLaunch` as special path.

- `../pose/SrcShared/Patches/EmPatchModuleSys.cpp:1487`
  - `SysAppLaunch` logging/inspection path with launch args:
    - `cardNo`, `dbID`, `launchFlags`, `cmd`, `cmdPBP`, `resultP`.

These confirm the launch contract is command-driven and app-info-structure-driven.

Additional public references:

- Palm OS Programmer's Companion TOC:
  - `https://www.fuw.edu.pl/~michalj/palmos/CompanionTOC.html`
- Application startup/stop chapter:
  - `https://www.fuw.edu.pl/~michalj/palmos/AppStartupAndStop.html`

## Current Tern Runtime Status (March 2026)

### Module Structure

- `core/src/prc_app/`:
  - CPU/decode + runtime state (`cpu/`, `runtime.rs`, `runner.rs`)
  - trap handling (`trap_stub.rs`, `traps/*`)
  - PRC parsing (`prc.rs`, `form_preview.rs`, `bitmap.rs`, `menu_preview.rs`)
  - UI rendering + controllers (`ui.rs`, `controller.rs`, `ui_component.rs`)
- `core/src/ui/prc_components.rs`:
  - shared Palm-style widget drawing helpers (buttons, alerts, scroll indicators, etc.)
- `core/src/application.rs`:
  - app shell orchestration and PRC session lifecycle.

### What Works

- PRC launch path with `SysAppStartup`/event loop/`SysAppExit` semantics.
- Event dispatch path with `EvtGetEvent`, `SysHandleEvent`, `MenuHandleEvent`, `FrmDispatchEvent`.
- Form rendering primitives used by current test apps:
  - form init/open,
  - bitmap draws,
  - field draws and text handle flow (`FldSetTextHandle`, `FldDrawField`),
  - menu bar/dropdown navigation,
  - help dialog flow (`FrmHelp`) with Palm-style visual treatment.
- Hardware-key navigation for non-touch usage (focus, selection, menu traversal).
- Embedded Palm font loading on desktop + x4 firmware (no SD font dependency).

### Known Gaps / Limits

- Not a full 68k implementation; still trap/probe-heavy in parts.
- Menu parsing is improved but not yet broad enough for all apps/resources.
- App compatibility remains narrow (Yoda works; Noah Lite still fails at runtime).
- PRC execution is still mostly file-backed/direct-run, not installed-database-backed.

## Bootstrap / Lifecycle Plan

1. Launch context fidelity:
   - Model `launch cmd`, `cmdPBP`, `launchFlags` explicitly in runtime context and register setup.
2. Trap ABI fidelity:
   - Move from generic register guesses to per-trap argument/return conventions.
3. Resource/memory fidelity:
   - Keep stable handle table, lock/unlock semantics, and resource mapping by type/id.
4. Event loop realism:
   - Provide deterministic synthetic event sequence (`nilEvent`/`appStopEvent`) for bootstrap.
5. Multi-app validation:
   - Validate with small app (`Yoda`) and complex app (`Noah Lite`) after each semantic increment.

## UI/App Support Snapshot

- `Yoda`:
  - launch and event loop: working
  - bitmap animation/blink: working
  - control focus + select via keys: working
  - menu open/navigation/select: working
  - help dialog from menu: working
- `Noah Lite`:
  - currently returns early (`EntryReturn`) and does not reach usable UI.
  - useful as the next compatibility target after DB install model lands.

## Database Support Proposal (Install-Then-Run)

### Goal

Match Palm behavior: PRC/PDB files are install payloads; execution/data access happens through installed databases (card + local ID + open refs), not by running raw files directly.

### Storage Location Proposal

Use a versioned database root under the image source:

- `sdcard/palmdb/v1/` on device
- equivalent host path on desktop image source

Layout:

- `sdcard/palmdb/v1/catalog.bin`
- `sdcard/palmdb/v1/db/<db_uid>.tdb`
- `sdcard/install/` (drop-in inbox for `*.prc` / `*.pdb`)
- `sdcard/install/done/` (processed successfully)
- `sdcard/install/failed/` (parse/install failures)

Where `<db_uid>` is a stable internal ID (u32 or u64) assigned on install.

### Record Format Proposal

#### `catalog.bin` (global index)

Binary, little-endian:

- Header:
  - magic: `TDB1`
  - schema version: `1`
  - entry count
- Entries:
  - `db_uid`
  - `card_no`
  - `name` (Palm DB name, fixed 32 bytes or len+bytes)
  - `type` (u32 4cc)
  - `creator` (u32 4cc)
  - `version` (u16)
  - `attributes` (u16)
  - `mod_number` (u32)
  - `created/modified/backup` times (u32 each)
  - flags: resource DB vs record DB, dirty, deleted, etc.
  - path pointer/index to `<db_uid>.tdb`

#### `<db_uid>.tdb` (single-db container)

Binary container with section directory:

- Header:
  - magic: `TDBD`
  - schema version
  - db metadata mirror (type/creator/name/attrs/version/etc.)
  - section table count
- Sections:
  - `app_info` blob (optional)
  - `sort_info` blob (optional)
  - `records` section (for record DBs)
  - `resources` section (for resource DBs)

`records` entries:

- per record:
  - record id (u32, unique within db)
  - attrs/category (u8/u8)
  - data offset + length

`resources` entries:

- per resource:
  - type (u32 4cc)
  - id (u16)
  - data offset + length

This keeps install data lossless enough for Dm APIs while remaining simple for firmware I/O.

### `/install` Inbox Workflow

1. On app startup, scan `sdcard/install/`.
2. For each `.prc`/`.pdb`:
   - parse DB identity (`name`, `type`, `creator`, `version`)
   - compute payload hash (for same-version replacement detection)
   - compare with installed catalog entry if present
3. Decide:
   - install new
   - upgrade existing (higher version or hash mismatch)
   - skip already installed (same version + same hash)
4. Move source file:
   - success -> `install/done/`
   - failure -> `install/failed/` + reason in log
5. Present user feedback:
   - popup/toast: “Installing Palm databases…”
   - completion summary: installed/upgraded/skipped/failed

Current code status:

- `core/src/palm_db/` contains initial scaffolding:
  - install decision model (`install.rs`)
  - installed DB metadata + identity (`types.rs`)
  - catalog trait (`catalog.rs`)
- `ImageSource::scan_palm_install_inbox()` exists as an integration hook.
- `Application` calls the hook at startup and logs returned summary.
- Actual file-system scanner + persistent catalog implementation is next.

### Runtime Integration

- New DB layer in `core/src/palm_db/`:
  - `catalog.rs` (open/list/find by name/type/creator)
  - `storage.rs` (`.tdb` read/write) [planned]
  - `install.rs` (PRC/PDB import pipeline)
  - `dm_backend.rs` (maps Dm traps to installed DB handles/records/resources) [planned]
- PRC launch path:
  - launch by DB identity (`cardNo`, `localID`) from catalog, not raw file.
- Dm trap behavior:
  - `DmGetResource`, `DmOpenDatabase`, `DmFindDatabase`, etc. resolve against installed DBs.

### Migration Plan

1. Add installer + catalog + `.tdb` storage format.
2. Install selected PRC/PDB when user opens/imports a file.
3. Launch PRCs only from installed catalog entries.
4. Wire Dm resource calls fully to installed DB backend.
5. Add record DB write paths and persistence semantics.

## Working Rule

Use runtime trace for “next blocker” and keep static scan as optional diagnostics.
Default logs should stay focused on:

- `PRC exec_trace start ...`
- `PRC exec_trace event ...`
- `PRC exec_trace stop ...`

## Launcher UX Proposal

### Goal

Support installed Palm apps and current content (books/images) in one launcher without introducing non-Palm interaction patterns.

### Recommended Pattern (Category Launcher, not tabs)

Use a Palm-style category selector and content pane instead of tab strips:

- Header/title bar: `Launcher`
- Category trigger (popup):
  - `Recent`
  - `Apps`
  - future: `Books`, `Images`, `All`
- Main content:
  - `Apps`: installed app list/grid (icon + title) from DB catalog (`type='appl'`)
  - `Recent`: mixed recent targets (apps/books/images)
- Bottom action row:
  - `Open`
  - `Menu`
  - optional `Prefs`

This preserves Palm look-and-feel and maps well onto hardware-button navigation.

### Why not tabs

- Palm Launcher conventions are category/popups, not tab bars.
- Category model scales better for installed databases.
- It reuses controls we already support (trigger/list/menu/buttons).

### Data Model

Keep recents unified:

- `RecentEntry::App { card_no, db_uid }`
- `RecentEntry::Book { path }`
- `RecentEntry::Image { path }`

App entries come from installed DB catalog and store stable DB identity for launch (`cardNo`, `localID/db_uid` mapping).

### Input Model (non-touch)

- `Up/Down`: move selection
- `Left/Right`: move focus between launcher regions (category/content/actions)
- `OK`: activate selection
- `Menu key`: open launcher menu
- `Back`: dismiss menu/dialog or leave launcher

### Rollout

1. Add `Apps` category (text list first).
2. Add mixed `Recent` model including apps.
3. Promote File Manager as an app entry rather than hard-coded launcher icon.
4. Add app icon grid mode and category persistence.

## Text Input Without Graffiti

### Problem

Palm apps expect Graffiti/stroke or keyboard events. Device has no touch/Graffiti area.

### Proposed Solution

Provide an on-screen QWERTY keyboard panel below the app viewport, navigated with existing directional buttons:

- directional keys move keyboard focus
- `OK` emits selected key into Palm event queue
- `Shift`/`123`/`Sym` toggle modes
- `Back` emits backspace (or closes keyboard if no field is focused)

### Layout

- Keep Palm app viewport at top.
- Reserve a fixed-height keyboard pane at bottom.
- Keyboard rows:
  - letters (`qwerty...`)
  - modifiers (`Shift`, `Space`, `Backspace`, `Enter`)
  - numeric/symbol page toggle

### Event Mapping to Palm

Keyboard actions should generate Palm key events (not direct field mutation):

- emit `keyDownEvent` with Palm-style `chr` and keycode/modifier data
- route via normal event loop (`EvtGetEvent` -> app handlers)
- keep field behavior app-driven (`FldHandleEvent`, handlers, etc.)

### Activation Policy

- Auto-show keyboard when focused object is a text field, or when app requests text input.
- Manual toggle via dedicated hardware key or launcher/menu action.

### Incremental Plan

1. Implement key event injection path and verify with simple field editing app.
2. Add minimal alpha keyboard (`a-z`, space, backspace, enter).
3. Add shift/caps and number/symbol layers.
4. Add repeat behavior for backspace/cursor keys.
