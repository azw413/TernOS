# UI Layer

This document describes the current UI-layer module structure and the intended ownership boundaries.

The architectural rule is:

- app modules declare UI state, content, and business actions
- `ternos::ui` owns shared widgets, rendering, hit-testing, focus, selection, damage, and composition
- shell adapter modules translate app-specific or Palm-host-specific behavior into the shared UI layer
- `application.rs` should only coordinate app switching and high-level state transitions

## Ownership Rules

The target ownership chain is:

- cell renderer owns cell painting and cell-local text/layout rules
- table owns row visibility, selection, scrolling, clipping, and table damage
- form owns child controls and form-local focus
- modal owns modal chrome, child composition, modal-local hit routing, and modal-local damage
- shell owns status bar semantics and app-shell transitions
- app owns domain data and the meaning of user actions

Apps must not own:

- repaint rectangles for shared widgets
- duplicate touch and keyboard behavior for the same shared control
- popup, menu, table, or modal draw order
- low-level hit-testing for shared controls

## Core Shared UI Modules

### `core/src/ternos/ui/runtime.rs`

Shared retained UI state and invalidation.

Owns:

- retained `UiForm` and `UiObject` state
- current focus
- invalidation and damage tracking
- runtime-owned `UiTableState`
- runtime-owned `UiPopupState`
- focus helpers such as:
  - `focusable_object_ids(...)`
  - `adjacent_focusable_object(...)`
  - `move_focus_linear(...)`
- shared state application helpers such as:
  - `apply_table_interaction(...)`
  - `set_popup_state_and_focus(...)`

Should increasingly own:

- composed sibling focus traversal
- more shared interaction state transitions across children

### `core/src/ternos/ui/view.rs`

Shared rendering orchestration.

Owns:

- `UiContext`
- `RenderQueue`
- `PositionedView`
- layer-aware view rendering
- queue flush behavior
- tracked damage presentation helpers

This is where shared composition and present behavior lives.

### `core/src/ternos/ui/table_view.rs`

Shared interactive table component.

Owns:

- table geometry
- visible row calculation
- clipping rules
- row and cell hit-testing
- scrollbar hit-testing
- selection movement
- horizontal selection movement
- row/cell damage for selection changes
- shared wrapped Palm text cell rendering via `PalmWrappedTextCellRenderer`

Important types:

- `TableView`
- `TableHit`
- `TableInteraction`
- `TableScrollBarView`
- `TableScrollBarHit`
- `TableCellRenderer`

The table should behave the same regardless of container.

### `core/src/ternos/ui/modal_form.rs`

Shared modal form container.

Owns:

- modal spec and child model
- modal-local focus and activation
- modal child hit-testing
- modal child composition
- modal-local dirty-region tracking

Supports shared widgets including:

- labels
- fields
- buttons
- embedded tables
- embedded scrollbars

Important types:

- `ModalFormSpec`
- `ModalWidget`
- `ModalFormView`
- `ModalFormController`
- `ModalHit`
- `ModalFormAction`

### `core/src/ternos/ui/popup_view.rs`

Shared popup/dropdown view.

Owns:

- popup geometry
- trigger geometry
- popup item bounds
- popup rendering
- popup hit-testing

Used by Home category selection and intended for any other Palm-style pull-down popup.

### `core/src/ternos/ui/popup_controller.rs`

Shared popup keyboard behavior.

Owns:

- popup selection movement
- popup confirm behavior
- popup close behavior

Important type:

- `PopupMenuAction`

### `core/src/ternos/ui/status_bar_view.rs`

Shared top status bar.

Owns:

- top bar rendering
- Home/Menu action slots
- battery indicator rendering
- right-hand app status slot
- status-bar touch hit-testing

Used by Home, reader, and Palm-hosted apps.

### `core/src/ternos/ui/status_bar_controller.rs`

Shared status-bar keyboard behavior.

Owns:

- left/right/up/down/confirm handling inside the status bar
- preferred status-bar focus selection

Important types:

- `StatusBarButtons`
- `StatusBarNavResult`

### `core/src/ternos/ui/form.rs`

Shared Palm-style form primitives.

Owns:

- button frames
- fields
- form title bars
- button layout helpers
- density-aware metrics for low/high DPI Palm-style controls

Important types:

- `PalmDensity`
- `PalmMetrics`
- `ButtonLayout`
- `FormTitleLayout`

### `core/src/ternos/ui/chrome.rs`

Shared Palm-style chrome primitives.

Owns:

- alert/modal frames
- popup/menu boxes
- low-DPI and high-DPI Palm chrome variants

This is shared frame and shadow treatment, not app-specific dialog drawing.

### `core/src/ternos/ui/text.rs`

Shared Palm text helpers.

Owns:

- Palm font lookup
- Palm text metrics
- Palm text drawing
- scaled Palm text drawing

### Other Shared UI Modules

These are smaller or more specialized shared modules:

- `event.rs`
  - shared `UiEvent`
- `geom.rs`
  - shared `Point`, `Rect`, `Size`
- `resource.rs`
  - form/object resource mapping
- `reader_view.rs`
  - reader content view primitives
- `text_view.rs`
  - shared text block rendering
- `list_view.rs`
  - list interaction/geometry primitives
- `prc_alert.rs`
  - Palm-style alert helpers on top of shared chrome/form primitives

## Shell Adapter Modules

These modules sit above shared UI and below app or Palm host logic.

### `core/src/app/reader_shell.rs`

Reader-specific shell adapter.

Owns:

- reader status-bar focus enum
- reader nav-event mapping from button input
- reader shell hit-testing helpers for:
  - status bar
  - reader help dialog
  - native reader menu overlay

This module exists because the reader has app-specific shell semantics, but it should still consume shared UI primitives.

### `core/src/palm/shell.rs`

Palm-host shell adapter.

Owns:

- Palm-host status-bar focus enum
- Palm status-bar hit translation
- Palm-pane layout under the shared status bar
- shared status-bar rendering for hosted PRC apps

This is Palm-host policy, not generic shared widget behavior.

## Palm Host UI Modules

These modules are still Palm-host specific and should increasingly consume shared `ternos::ui` primitives rather than becoming a parallel UI stack.

### `core/src/palm/ui.rs`

Palm-specific rendering and hit-testing bridge.

Currently owns:

- form preview drawing
- Palm menu drawing/hit-testing
- Palm help-dialog drawing/hit-testing
- Palm control preview helpers

This is still larger than ideal. The direction is to keep moving shared behavior down into `ternos::ui`.

### `core/src/palm/controller.rs`

Palm-host interaction controllers.

Owns:

- Palm menu controller
- Palm help-dialog controller
- Palm focus controller for PRC forms

This is legitimate Palm-host policy, but should rely on shared controls where possible.

## App Modules That Still Need Cleanup

### `core/src/app/home.rs`

Current role:

- declares launcher content and cache data
- reacts to user actions
- builds launcher form/table models

Recent cleanup already moved:

- popup state into `UiRuntime`
- table state into `UiRuntime`
- popup keyboard behavior into `popup_controller`
- status-bar keyboard behavior into `status_bar_controller`
- launcher section ownership from duplicated app state toward runtime focus

Still needs more cleanup:

- remaining launcher-specific focus policy
- more composition ownership pushed into shared form/runtime controllers

### `core/src/app/book_reader.rs`

Current role:

- declares reader content and reader-specific overlays/dialogs
- reacts to reader actions

Recent cleanup already moved:

- TOC composition and interaction into shared modal/table components
- help behavior onto the shared Palm `FrmHelp` path
- more shell/menu hit-testing into `reader_shell`

Still needs more cleanup:

- some menu/dialog lifecycle still coordinated externally
- more reader-shell action application can move out of `application.rs`

### `core/src/application.rs`

This should not be a god module.

Current intended role:

- top-level app switching
- high-level lifecycle coordination
- delegating interaction to app, shell, Palm host, and shared UI modules

It has improved, but still owns too much:

- some reader touch action application
- some PRC touch action application
- high-level orchestration that should continue moving into shell/controller modules

## Current Direction

The refactor direction is:

1. move shared interaction state into `UiRuntime`
2. move widget behavior into shared `ternos::ui` modules
3. keep shell-specific translation in small adapter modules like:
   - `reader_shell.rs`
   - `palm/shell.rs`
4. keep app modules declarative
5. keep `application.rs` thin

When adding new UI behavior, the preferred order is:

1. shared widget or primitive in `ternos::ui`
2. shell adapter if behavior is app-shell or Palm-host specific
3. app declaration and action handling

The anti-pattern to avoid is:

- app module draws a shared control
- app module separately hit-tests it
- app module separately invents repaint policy for it

That is exactly the duplication this architecture is intended to remove.
