# TernReader Architecture Rules

This file defines non-optional engineering rules for work in this repository.

The purpose is to stop architectural drift, especially in the UI stack, where
shortcuts in the wrong layer create regressions, duplicate logic, and repeated
rework.

## Core Principle

The system is split into three layers:

1. App layer
2. UI layer
3. Hardware/platform layer

These responsibilities must stay separate.

## App Layer Rules

The app layer:
- declares screens, forms, menus, alerts, tables, buttons, fields, and dialogs
  using shared UI APIs
- owns business state only
- responds to high-level UI events only
- decides what action to take when a UI event occurs

The app layer must not:
- perform hit-testing
- compute repaint rectangles
- decide z-order or paint order
- implement widget-specific navigation behavior
- manually manage focus rules for shared controls
- manually preserve backing buffers for overlays
- implement touch behavior separately from keyboard behavior for the same control
- invent custom controls where a shared control already exists or should exist

If an app needs to know how a button/table/menu/modal is drawn or repainted,
the abstraction boundary is wrong and the logic belongs in `ternos::ui`.

## UI Layer Rules

The UI layer owns all interactive UI behavior. The UI should render components dependent on the display DPI so components look consistent at low resolution and high.

The UI layer must own:
- component geometry
- focus
- selection
- hit-testing
- keyboard navigation
- touch navigation
- component state transitions
- damage tracking / invalidation
- render ordering and layering
- clipping
- partial repaint policy
- shared Palm low-DPI vs high-DPI styling rules

The UI layer must provide shared implementations for:
- form controls
- buttons
- repeating buttons
- fields
- labels
- tables
- scrollbars
- popups
- menus
- alerts
- modal forms
- status bar / shell chrome
- Palm help dialogs

If two screens need the same interaction behavior, it must be implemented once
in `core/src/ternos/ui/*` or `core/src/palm/*`, not duplicated in app code.

## Hardware / Platform Rules

The hardware/platform layer:
- converts raw device input into platform input events
- presents framebuffer regions to a device or simulator
- exposes hardware capabilities like grayscale or partial refresh

The hardware layer must not:
- know app behavior
- know screen-specific navigation semantics
- implement UI widget logic

## Palm Compatibility Rules

Palm apps and native Tern apps should converge on the same shared UI behavior.

Rules:
- native UI must not be lower-level than Palm UI
- Palm PRC behavior should be adapted onto shared UI primitives whenever possible
- if native and Palm versions of the same control differ, the difference must be
  intentional and centralized
- low-DPI vs high-DPI behavior must be selected explicitly by density, never
  guessed from control pixel size

Palm-specific compatibility code belongs under `core/src/palm/*`.
Shared rendering and control behavior belongs under `core/src/ternos/ui/*`.

## Rendering And Damage Rules

Apps must not manage repaint strategy.

Required rules:
- damage ownership belongs to the shared UI layer
- component state changes must produce damage in the shared component/runtime
- old and new bounds must be considered for invalidation
- modal/popup/menu exposure damage belongs to the shared UI layer
- paint order belongs to shared composition, not app code
- clipping belongs to shared views, not app code

App code may request a full redraw only as a temporary fallback when a shared
UI path does not yet exist. That should be treated as debt to remove.

It is not acceptable to implement app-specific retained repaint logic for a
shared control such as a table, popup, menu, or modal.

## Input Rules

Touch and keyboard must drive the same control logic.

That means:
- touch hit-testing must use shared control geometry
- touch selection/activation must route through the same controller/state path
  as keyboard selection/activation
- app code must not maintain one logic path for keyboard and a different one for
  touch for the same control

If a control renders through a shared UI component, its hit-testing must also be
shared.

## Modal / Table / Menu Rules

These are the most common source of shortcuts. Explicit rules:

- a modal form is a shared UI component
- a table is a shared UI component
- a scrollbar is a shared UI component
- a menu is a shared UI component
- a popup is a shared UI component

An app may declare:
- the model contents
- which row/item is selected by meaning
- the actions for selection / activation / dismissal

An app may not declare:
- row repaint rectangles
- per-widget z-order rules
- custom scrollbar math for a shared scrollbar
- menu overlay cleanup logic
- modal frame cleanup logic

## State Ownership Rules

Shared UI state belongs in shared controllers/runtime, not ad hoc app fields,
unless the field is domain state.

Examples of shared UI state:
- focused control
- selected row
- top visible row
- active menu
- popup open/closed
- modal control focus
- status bar focus

Examples of app/domain state:
- current page number
- selected TOC target
- current book path
- current image path
- recent entries
- battery percentage

If a state field exists only to make rendering or navigation work, it likely
belongs in the UI layer.

## Module Boundaries

Preferred ownership:
- `core/src/ternos/ui/*`
  - shared controls, shared chrome, text, layout, modal logic, tables, popups,
    status bar, composition, invalidation
- `core/src/palm/*`
  - Palm runtime, Palm controllers, PRC adapters, Palm-specific shell behavior
- `core/src/app/*`
  - app business logic and declarative screen construction only
- `core/src/application.rs`
  - top-level app switching and coordination only

`application.rs` must not become a god module.

If logic is Palm-host specific, move it to `core/src/palm/*`.
If logic is shared UI behavior, move it to `core/src/ternos/ui/*`.

## Shortcut Policy

Shortcuts are allowed only if all of the following are true:
- the shared abstraction does not exist yet
- the shortcut is clearly marked as temporary
- the final intended ownership is explicitly stated
- the shortcut does not duplicate a second behavior path for an existing shared
  control

Shortcuts are not allowed if they:
- put widget logic in app code
- duplicate hit-testing
- duplicate repaint logic
- duplicate focus rules
- create separate touch and keyboard behavior for the same control

## Decision Rule

Before implementing a UI change, ask:

1. Is this business logic or UI behavior?
2. If UI behavior, can it be reused by another app or Palm screen?
3. If yes, it must go in `ternos::ui` or `palm`, not in the app.

If there is doubt, prefer moving logic downward into the shared layer rather
than upward into the app.

## Preferred Outcome

The desired end state is:
- apps declare Palm-style UI structures and handle events
- shared UI owns rendering, interaction, and invalidation
- hardware only presents pixels and reports input

Any change that weakens that boundary is a regression, even if it looks faster
in the short term.
