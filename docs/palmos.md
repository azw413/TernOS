# PalmOS PRC Launch Notes

## Why This Exists

We are building a PRC runtime path for TernReader. This document captures what is deterministic in PalmOS launch, what is app-specific, and what we should emulate first.

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

## Current Tern Runtime Status

- Runtime trace now executes enough code to reach traps and classify stop reasons.
- Minimal trap semantics exist for startup/exit/form/event/mem/resource calls.
- We still use lightweight CPU decode and probe semantics (not full 68k correctness).

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

## Working Rule

Use runtime trace for “next blocker” and keep static scan as optional diagnostics.
Default logs should stay focused on:

- `PRC exec_trace start ...`
- `PRC exec_trace event ...`
- `PRC exec_trace stop ...`
