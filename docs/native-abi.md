# Native PRC ABI

This document describes the Tern-side runtime model for loading and executing a native PRC. It complements the packaging and builder notes in `../PRCBuilder`: that project defines how a native PRC is produced, while this document defines how Tern detects, materializes, relocates, and runs it.

The goal is to keep Palm-style application semantics while avoiding literal 68K trap mechanics. A native app should still feel like a Palm app in terms of resources, forms, menus, events, and launch codes, but it should execute through a simple, explicit host ABI rather than a CPU trap emulation layer.

## Design Goals

- Keep the PRC container as the installable unit.
- Preserve Palm resource semantics where possible.
- Distinguish classic 68K PRCs from native PRCs through Tern-specific execution resources.
- Keep the runtime linker simple:
  - no arbitrary symbol lookup
  - no shared libraries
  - no desktop-style dynamic loader
- Keep Palm semantics at the API level, not at the instruction level.
- Expose a stable versioned host API table instead of raw RISC-V `ecall` or interrupt conventions.

## Native Detection

Tern should treat every installed app as a PRC first.

At launch time:

1. Parse the PRC database header and resource table.
2. Check for Tern execution metadata resources.
3. If those resources are absent, use the classic Palm/68K runtime path.
4. If those resources are present, use the native runtime path.

The loader rule is:

- No `tABI` / `tEXE`: classic Palm path.
- `tABI` and `tEXE` present: native path.

This keeps a single installable format while allowing Tern to host both:

- classic Palm applications
- native Tern applications

## Expected Tern-Specific Resources

The exact binary layout belongs in PRCBuilder docs, but on the Tern side we expect a native PRC to expose metadata/resources along these lines:

- `tABI`
  - target architecture
  - ABI version
  - required host features
- `tEXE`
  - executable manifest
  - entry point
  - section inventory
  - memory sizes/alignment
- `tCOD`
  - executable code payload
- `tROD`
  - read-only data payload
- `tDAT`
  - initialized writable data payload
- `tREL`
  - relocation records, if needed

Additional resources may be added later, but Tern should keep the native loader centered around a small, explicit set of execution resources rather than ad hoc conventions.

## Runtime Load Flow

The runtime should load a native PRC in the following stages.

### 1. Parse and Validate

- Parse the PRC resource table.
- Read `tABI`.
- Verify:
  - architecture matches the current device/runtime
  - ABI version is supported
  - required features are available

If validation fails, the app should not launch. Tern should present a normal error dialog rather than attempting a partial load.

### 2. Read the Executable Manifest

- Parse `tEXE`.
- Determine:
  - entry point offset
  - section resources and sizes
  - alignment requirements
  - BSS size
  - relocation presence

At this stage Tern should not interpret arbitrary symbols. It should treat the native image as a small self-contained program image with structured metadata.

### 3. Materialize the Image

Allocate runtime memory for:

- code
- rodata
- data
- bss

Then:

- copy `tCOD` into the code region
- copy `tROD` into the read-only region
- copy `tDAT` into the writable data region
- zero-fill BSS

The builder should do as much layout work as possible up front. The runtime loader should be left with a minimal amount of fixup work.

### 4. Apply Relocations

If `tREL` is present, Tern applies relocation fixups after section materialization and before the app entry point is called.

This is described in more detail below.

### 5. Build the Host ABI Table

Before entering the app, Tern builds the host API table that the app will use for all host services.

Conceptually:

```rust
#[repr(C)]
pub struct TernPrcApi {
    pub abi_version: u32,
    pub sys: *const SysApi,
    pub mem: *const MemApi,
    pub evt: *const EventApi,
    pub frm: *const FormApi,
    pub menu: *const MenuApi,
    pub win: *const WindowApi,
    pub res: *const ResourceApi,
    pub db: *const DbApi,
}
```

This is the native equivalent of the old Palm trap surface:

- explicit
- versioned
- easy to validate
- easy to stub in tests

### 6. Enter the App

Tern constructs launch arguments and calls the app entry point, for example:

```rust
#[repr(C)]
pub struct TernLaunchArgs {
    pub launch_code: u16,
    pub launch_flags: u16,
    pub launch_param: *const core::ffi::c_void,
}

pub type TernPrcMain =
    extern "C" fn(api: *const TernPrcApi, args: *const TernLaunchArgs) -> u32;
```

The app should receive Palm-like launch semantics, but through a native function entry rather than 68K launch glue.

### 7. Normal Event/Resource Execution

Once entered, the app uses the host API table for all interaction with Tern:

- event retrieval
- form/menu interaction
- resource lookup
- drawing primitives
- storage/database calls
- memory allocation

The native app should not receive direct low-level access to:

- display buffers
- device interrupts
- raw UI composition internals

Those remain Tern responsibilities.

## Why Not Use Real CPU Traps?

RISC-V does have `ecall`, which is the nearest equivalent to a trap or software interrupt. However, the native PRC ABI should not be built directly around raw `ecall`.

Reasons:

- it adds complexity without clear benefit
- it forces a register-level service ABI
- it complicates testing and debugging
- it makes embedded integration harder for little gain

The better approach is:

- Palm semantics at the API level
- explicit host table at the binary ABI level

If Tern later wants to use `ecall` internally, that can still be hidden behind generated stubs. It should not be the public app ABI.

## Relocation Model

The relocation model should stay deliberately small in v1.

The target design is not a desktop dynamic linker. It is a simple image loader that performs only the fixups needed to:

- connect sections within the loaded image
- bind native code to the Tern host ABI

### What Should Be Relocatable

Relocations should support these cases first:

1. Intra-image references
- pointers from one section to another
- references from data to rodata
- references from code to data/rodata where the packaged format requires fixups

2. Host API bindings
- references to Tern service entry points
- preferably through a stable import table rather than arbitrary symbol names

### What We Should Avoid in v1

- arbitrary dynamic symbol resolution by string
- general-purpose ELF loader semantics
- lazy binding
- shared library dependencies between apps
- runtime dependency graphs

### Anticipated Runtime Relocation Flow

Once code/data/rodata/BSS are allocated:

1. Parse `tREL`.
2. For each relocation:
  - identify the patch site
  - resolve the relocation target
  - write the patched value into the loaded image

Relocation target resolution should support two broad categories:

- section-relative target
  - target section plus offset
- host import target
  - target service group plus slot index, or similar compact import identifier

This keeps the runtime loader deterministic and independent of global symbol tables.

### Anticipated Relocation Record Shape

The exact on-disk layout is still a packaging concern, but logically each relocation record needs:

- source section
- source offset
- relocation kind
- target descriptor
- optional addend

For example:

- source: `tCOD + 0x1234`
- kind: absolute pointer
- target: `tDAT + 0x0010`
- addend: `0`

or:

- source: `tCOD + 0x00a8`
- kind: host import
- target: `frm.DispatchEvent`

### Preferred Resolution Strategy

The simplest model is:

- PRCBuilder resolves as much as possible at package time
- Tern performs only final load-address and host-import fixups

That means:

- section layout and offsets are already known when packaged
- relocation records remain compact
- runtime work stays small enough for embedded devices

### Host Import Binding

The cleanest host-binding model is an import table that references fixed ABI slots rather than string names.

For example:

- subsystem ID: `frm`
- function slot: `12`

Then the loader binds that relocation to:

- `api->frm[12]`

or to the concrete function pointer stored in the resolved host table for that slot.

This has several benefits:

- stable binary interface
- compact metadata
- no runtime name lookup
- easy ABI version gating

### When Relocations Might Be Omitted

Some native PRCs may be packaged so that no relocations are needed beyond the initial host API pointer passed at entry.

If the code model and packaging format allow:

- position-independent code
- relative internal references
- explicit host table usage only through the entry argument

then `tREL` may be empty or absent.

Tern should support that case naturally.

## Runtime Module Shape

On the Tern side, native PRC support should be introduced as a dedicated runtime path rather than folded into the existing Palm/68K interpreter code.

A reasonable module split would be:

- common PRC parsing/resource indexing
  - existing shared PRC parsing code
- classic Palm runtime
  - existing `core/src/palm/*`
- native PRC runtime
  - `core/src/native_prc/abi.rs`
  - `core/src/native_prc/metadata.rs`
  - `core/src/native_prc/loader.rs`
  - `core/src/native_prc/reloc.rs`
  - `core/src/native_prc/host_api.rs`

This keeps the responsibilities clear:

- PRC parsing is shared
- 68K hosting stays in the Palm runtime
- native loading/execution lives in its own path

## UI and System Contract

The native ABI must not bypass Tern's UI architecture.

Native PRCs should consume the same Palm-style abstractions as other apps:

- forms
- menus
- alerts
- tables
- events
- resources

They should not directly own:

- z-order
- damage rectangles
- low-level repaint policy
- raw device I/O

That keeps native apps aligned with the same architectural rules as Palm-hosted apps and native built-in apps.

## Failure Handling

If a native PRC cannot be loaded, the runtime should fail cleanly at a high level:

- unsupported architecture
- ABI version mismatch
- malformed metadata
- missing required resource section
- unsupported relocation kind
- host feature requirement not met

These should surface as a normal user-facing error dialog, not as a partial launch or crash.

## Initial Scope Recommendation

The first native loader implementation should deliberately stay small:

- one executable image
- one entry point
- one host ABI table
- fixed subsystem group layout
- compact relocation support
- no shared libraries
- no plugin-style cross-app linking

That is enough to make native PRCs practical without turning Tern into a full OS-level dynamic linker.

## Open Questions

These items still need to be nailed down alongside PRCBuilder:

- exact binary layouts for `tABI`, `tEXE`, and `tREL`
- architecture identifier encoding
- required feature bitset design
- import slot numbering and versioning policy
- whether code must be position-independent by default
- whether function-pointer imports should be copied into an app-local import table or referenced directly from `TernPrcApi`

## Summary

The native PRC runtime model should be:

- PRC container in, not a new package format
- Tern metadata selects native vs classic execution
- native image sections are materialized into memory
- relocations are applied only where necessary
- host services are provided through an explicit versioned API table
- Palm application semantics are preserved above the ABI line

That gives Tern a native app model that is:

- simple enough for embedded targets
- compatible with Palm-style application structure
- flexible enough to support future native PRC tooling
