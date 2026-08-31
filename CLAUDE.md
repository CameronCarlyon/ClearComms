# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

ClearComms is a Windows desktop companion app for Microsoft Flight Simulator: a Tauri 2.x shell with a Svelte 5 (TypeScript) frontend and a Rust backend. It lets hardware axes/buttons (and, increasingly, in-sim cockpit audio panel controls via SimConnect/MobiFlight) drive per-application Windows volume/mute (vPilot, GSX, Discord, etc.), so pilots don't have to leave the sim to touch the Windows volume mixer.

The overriding design constraint is **performance and minimal footprint** — see the SimConnect section below, which is non-negotiable project policy, not a suggestion.

## Commands

```bash
npm run dev            # vite dev (frontend only, no Tauri shell)
npm run tauri dev      # full app: Vite dev server (port 1420) + cargo build/run
npm run tauri build    # production build → installer + standalone binary
npm run check          # svelte-kit sync + svelte-check (TS/Svelte types, must be zero errors/warnings)
npm run check:watch    # same, in watch mode
```

Rust linting (run from `src-tauri/`):

```bash
cd src-tauri && cargo clippy   # must be zero warnings
cd src-tauri && cargo build    # debug build without the Tauri CLI wrapper
```

There is no automated test suite (no `cargo test` targets, no frontend test runner configured) — verification is via `svelte-check`, `cargo clippy`, and manual exercise of `npm run tauri dev`.

## Architecture

### Three-tier layout

```
Svelte 5 frontend  →  Tauri 2.x IPC (invoke/emit, JSON)  →  Rust backend (Win32/COM/SimConnect FFI)
```

- `src/routes/+page.svelte` is the application orchestrator — all `invoke()` calls are centralised there (~2800 lines); components communicate outward via dispatched events, not by calling Tauri commands directly.
- `src/lib/components/` — presentational Svelte components, each intended to stay under ~150 lines and single-purpose (`ApplicationChannel`, `Mixer`, `VolumeSlider`, `ButtonSimFunction`, `Dock`, etc.). Own their `<style>` blocks; no shared CSS framework.
- `src/lib/stores/` — **not** a bag of reactive stores. `audioStore.ts` is plain utility functions/constants (`formatProcessName`, `isSystemVolume`, `SYSTEM_VOLUME_ID`); `simStore.svelte.ts` and `mixerMenuStore.svelte.ts` are genuinely reactive (Svelte 5 runes) — sim connection status and "only one mixer dropdown open at a time" respectively; `themeStore.ts` is theme state.
- `src/lib/data/aircraftProfiles.ts` — curated aircraft→LVar registry (see Sim Channel integration below). This is the one place aircraft-specific knowledge lives.
- `src-tauri/src/` — one module directory per subsystem, each exposing `#[tauri::command]`-annotated `pub fn`s and owning its own background thread(s):
  - `audio_management/` — Windows Core Audio API (WASAPI) per-app + system volume/mute
  - `hardware_input/` — Joystick API + HID API axis/button polling
  - `simconnect/` — SimConnect connection lifecycle + MobiFlight LVar bridge (`mod.rs` = lifecycle/commands, `connection.rs` = dispatch loop + FFI, `state.rs` = shared state types)
  - `sim_detection/` — Toolhelp32-based MSFS process watcher that drives the SimConnect lifecycle
  - `native_menu.rs`, `theme.rs`, `notification.rs`, `window_utils.rs` — tray/menu, theme detection, native Windows toast notifications, DPI-aware window positioning

### Threading model

Every subsystem that touches a blocking OS/COM/FFI API gets its own dedicated OS thread; Tauri commands are thin wrappers that forward work to the owning thread via a channel and block on the reply. Nothing does cross-thread COM or blocking FFI calls directly from a Tauri command handler.

| Thread | Owns | Wake mechanism |
|---|---|---|
| `audio-com` | All WASAPI/COM calls (`AudioManager`) | `mpsc` command channel |
| `audio-notify` | `IMMNotificationClient` device-change callbacks | COM callback, forwards via channel |
| `input-poll` | Joystick + HID polling, emits `input-axis-data` | 50ms sleep loop (push-based to frontend) |
| `sim-detection` | Toolhelp32 process snapshot for MSFS 2020/2024 | `WaitForSingleObject`, 2s poll |
| `simconnect-ctrl` | Lifecycle controller: spawns/tears down the connection thread on `SimDetectionEvent` | blocking `mpsc::recv` |
| `simconnect` | SimConnect dispatch loop + MobiFlight LVar I/O | `WaitForMultipleObjects` on the SimConnect event handle + a wake event |
| `window-anim` | Window resize easing (singleton, lazily spawned) | channel-driven, ~240fps while animating |
| `theme-monitor` | Windows theme registry key watch | `WaitForMultipleObjects` on a `RegNotifyChangeKeyValue` notification |
| `menu-defer` | Native tray/context menu display | one-shot spawn |

Resource cleanup follows RAII throughout: `Drop` impls close COM objects/handles (`ProcessHandle`, `AudioManager`, `HidInputManager`, `PopupMenu`); all long-lived threads shut down via an `AtomicBool` or Win32 event signal on `quit_application`/`restart_application`, not by killing the process. `main.rs` routes that through `shutdown_core_subsystems`, `shutdown_sim_detection` and `shutdown_theme_monitor` rather than repeating the sequence per exit path.

Two hot paths are deliberately allocation-free when idle, and should stay that way: `hardware_input` compares raw `JOYINFOEX` readings before building any payload, and `audio_management` caches `ISimpleAudioVolume` per process ID so a volume write is a map lookup rather than a walk over every endpoint and session.

### IPC command surface

~30 commands registered in `main.rs`'s `invoke_handler!`, grouped by owning module (`hardware_input::*`, `audio_management::*`, `simconnect::*`, `theme::*`, plus window/config utility commands defined in `main.rs` itself). Every command returns `Result<T, String>`; the frontend wraps every `invoke()` in try/catch. Hardware axis/button data is **pushed** to the frontend via the `input-axis-data` Tauri event rather than polled via `invoke()` — same pattern for `sim-status-changed`. Toast notifications (`notification.rs`) are called directly from Rust and expose no commands — the frontend never triggers one.

### Persistence

Two separate mechanisms, not one:
- **`localStorage`** (WebView-scoped) — axis mappings, button mappings, pinned app list. Loaded on mount, saved after every config change.
- **`ui-state.json`** in the Tauri app config directory, read/written via the `save_config_value` / `load_config_value` commands (key-value, mutex-guarded on the Rust side) — general UI state that needs to survive independent of the WebView's storage profile.

### SimConnect / MobiFlight integration — mandatory performance rules

This is the most performance-sensitive part of the codebase and has hard rules, not guidelines, because ClearComms runs alongside MSFS, one of the most CPU-hungry applications on the system. **The integration must be event-driven and produce zero CPU usage while idle.** Before touching anything under `simconnect/` or `sim_detection/`, internalise these:

1. **Dispatch loop must block on a Win32 event**, never spin or `sleep`-poll. Pattern: `WaitForSingleObject`/`WaitForMultipleObjects(event_handle, timeout)` where `event_handle` was passed as `hEventHandle` to `SimConnect_Open`. Drain all pending messages in a tight inner loop until `GetNextDispatch` returns `E_FAIL` (queue empty — this is a normal, expected return, not an error), then return to waiting. Never call `GetNextDispatch` exactly once per wake.
2. **`TITLE` SimVar** requests must use `SIMCONNECT_PERIOD_SECOND` + `SIMCONNECT_DATA_REQUEST_FLAG_CHANGED` — the sim only sends a message when the value actually changed.
3. **MobiFlight response/LVar channels** must subscribe with `SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET` — data is pushed only when the WASM module writes, never on a timer.
4. **No fixed-timer health-check pings.** Ping once on connect to confirm WASM presence; rely on `ON_SET` subscriptions thereafter.
5. **No log spam on hot paths.** Anything inside the dispatch loop or a per-second data path must be `TRACE`-level or gated on an actual value change; `INFO` only on real state transitions (aircraft changed, connection state changed, WASM state changed).

Sim Channel (radio LVar) architecture, layered on top of this:
- **Aircraft knowledge lives entirely in the frontend** (`src/lib/data/aircraftProfiles.ts`), mapping generic categories (COM1–3, HF1–2, CAB, PA, INT) to per-aircraft LVars sourced from MobiFlight HubHop. Only verified LVars belong here — a nonexistent LVar reads back as a constant 0, which the mute logic would misread as "always muted." Adding aircraft support is a data-only change to this file.
- **The Rust backend is a dumb transport** — no aircraft profiles live in Rust. The frontend matches the `TITLE` SimVar to a profile and sends flat LVar name lists via `subscribe_lvars`; writes go through `set_sim_lvar`.
- ClearComms registers its own MobiFlight client (`MF.Clients.Add.ClearComms`) with dedicated `ClearComms.Command`/`ClearComms.LVars` channels so it never collides with a separately-running MobiFlight Connector app.
- **Loop prevention is value-based**: an inbound LVar event whose raw value equals what current app state already maps to is treated as an echo and dropped. LVar-driven UI updates use `fromLvar` code paths that never write back to the sim. A user actively dragging a slider (tracked in `manuallyControlledSessions`) always wins over inbound sim state.
- LVar names are embedded directly into MobiFlight calculator-code strings sent to the WASM module, so `validate_lvar_name` (`simconnect/mod.rs`) rejects anything outside `[A-Za-z0-9_]` before it reaches that boundary — treat this as the injection boundary if you touch LVar handling.
- Currently captain-side audio panel only — the profile schema already carries both seats, F/O support is a known future increment.

### State management (frontend)

All reactive UI state is local `$state` runes inside `+page.svelte` (no global reactive store for app data). Alongside it, a dozen-plus plain `Map`/`Set` instances hold performance-critical, non-reactive tracking state (activation guards, animation cancellation signals, throttle state, etc.) deliberately kept *outside* `$state` to avoid triggering re-renders on every hardware poll tick. All these caches are bounded and periodically swept — if you add a new cache, bound it and wire it into the existing cleanup pass rather than leaving it unbounded.

## Conventions

- **British English** in all comments, docs, and UI copy.
- Comments explain *why*, not *what* — mechanics should be obvious from naming.
- Rust: `snake_case` fns/vars, `PascalCase` structs/enums, `Result<T, E>` everywhere, one module = one subsystem with a `pub fn` surface in `mod.rs`.
- TypeScript/Svelte: `camelCase` fns/vars, `PascalCase` components/interfaces, interfaces over `type` aliases, components stay small and single-purpose, props down / events up (no component calls `invoke()` directly except `+page.svelte`).
- Prefer the simplest solution that meets the performance constraint; don't add abstraction, dependencies, or "optimisations" without evidence they're needed.

## Reference docs in this repo

- [.github/README.md](.github/README.md) — user-facing feature overview and install/usage instructions.
- [.github/DOCUMENTATION.md](.github/DOCUMENTATION.md) — deep technical reference (system diagrams, full API/data-structure reference, latency budget). Accurate for `audio_management`/`hardware_input`; its "Future Considerations" section describing SimConnect as a not-yet-built scaffold is **stale** — SimConnect/MobiFlight integration is now fully implemented in `simconnect/` and `sim_detection/` as described above.

## External references

- [Tauri v2 docs](https://v2.tauri.app/)
- [Svelte docs](https://svelte.dev/docs) / [SvelteKit docs](https://kit.svelte.dev/docs)
- [MobiFlight WASM module docs](https://docs.mobiflight.com/guides/wasm-module/) — canonical source for the `SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET` subscription pattern the SimConnect rules above depend on.
