# ClearComms — Technical Documentation

**Version:** 1.0.0
**Platform:** Windows 10/11
**Architecture:** Tauri 2.x | Rust | Svelte 5 | TypeScript
**Last Updated:** April 2026

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Technical Architecture](#2-technical-architecture)
   - 2.1 [System Architecture](#21-system-architecture)
   - 2.2 [Design Patterns](#22-design-patterns)
   - 2.3 [Data Flow](#23-data-flow)
3. [Technology Stack](#3-technology-stack)
   - 3.1 [Dependency Matrix](#31-dependency-matrix)
   - 3.2 [Stack Rationale](#32-stack-rationale)
4. [Core Systems](#4-core-systems)
   - 4.1 [Audio Management](#41-audio-management)
   - 4.2 [Hardware Input](#42-hardware-input)
   - 4.3 [State Management](#43-state-management)
   - 4.4 [Animation Systems](#44-animation-systems)
5. [Performance and Optimisation](#5-performance-and-optimisation)
   - 5.1 [Release Profile](#51-release-profile)
   - 5.2 [Memory Management](#52-memory-management)
   - 5.3 [Threading Model](#53-threading-model)
   - 5.4 [Latency Budget](#54-latency-budget)
6. [Security and Reliability](#6-security-and-reliability)
7. [Code Quality](#7-code-quality)
8. [Build and Deployment](#8-build-and-deployment)
9. [Feature Specification](#9-feature-specification)
   - 9.1 [Hardware Axis Binding](#91-hardware-axis-binding)
   - 9.2 [Hardware Button Binding](#92-hardware-button-binding)
   - 9.3 [System Volume Control](#93-system-volume-control)
   - 9.4 [Window Management](#94-window-management)
   - 9.5 [Visual Design](#95-visual-design)
10. [API Reference](#10-api-reference)
    - 10.1 [Audio Management Commands](#101-audio-management-commands)
    - 10.2 [Hardware Input Commands](#102-hardware-input-commands)
    - 10.3 [Window Management Commands](#103-window-management-commands)
    - 10.4 [Utility Commands](#104-utility-commands)
11. [Data Structures](#11-data-structures)
    - 11.1 [TypeScript Interfaces](#111-typescript-interfaces)
    - 11.2 [Rust Structures](#112-rust-structures)
12. [Architecture Decisions](#12-architecture-decisions)
13. [Future Considerations](#13-future-considerations)

---

## 1. Project Overview

ClearComms is a high-performance desktop application providing real-time per-application audio mixing control for Microsoft Flight Simulator environments. The application enables hardware-based volume control — mapping physical axes (knobs, sliders, throttle levers) and buttons on flight simulation peripherals directly to individual Windows audio sessions — allowing volume adjustments to be made without leaving the simulator.

The application addresses a specific workflow gap in the flight simulation ecosystem: pilots running multiple audio applications simultaneously (vPilot for online ATC, Discord for group communication, GSX for ground services, MSFS for game audio) must typically Alt-Tab to the Windows Volume Mixer to adjust levels. ClearComms eliminates this context switch by providing a persistent, hardware-driven audio control surface that operates transparently alongside the simulator.

The system is built on a three-tier architecture: a Rust native backend providing direct Windows API integration (Core Audio, Joystick, HID), a Tauri 2.x IPC bridge for type-safe command invocation with JSON serialisation, and a Svelte 5 reactive frontend with runes-based state management. This architecture delivers sub-10ms input-to-audio latency, a steady-state memory footprint well under 100MB, and a release binary optimised for minimal size through fat link-time optimisation.

---

## 2. Technical Architecture

### 2.1 System Architecture

ClearComms employs a three-tier architecture optimised for low-latency native system integration and responsive user interaction:

```
┌──────────────────────────────────────────────────────────────────────┐
│                        PRESENTATION LAYER                            │
│                                                                      │
│  Svelte 5 + TypeScript              WebView2 (Windows)               │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  +page.svelte (Application Orchestrator)                       │  │
│  │  ├── $state runes for reactive UI state (~30 variables)        │  │
│  │  ├── $effect blocks for derived behaviour (3 effects)          │  │
│  │  ├── Map/Set caches for non-reactive performance state         │  │
│  │  ├── Tauri event listener (hardware), setInterval (200ms audio)│  │
│  │  ├── requestAnimationFrame volume animations                   │  │
│  │  └── localStorage persistence (3 keys)                         │  │
│  │                                                                │  │
│  │  12 Reusable Components                                        │  │
│  │  └── Props-down, events-up communication                       │  │
│  └────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────┤
│                       COMMUNICATION LAYER                            │
│                                                                      │
│  Tauri 2.x IPC                                                       │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  26 registered commands via invoke_handler                     │  │
│  │  ├── invoke() — Frontend → Backend (JSON serialisation)        │  │
│  │  ├── emit()   — Backend → Frontend (event bus)                 │  │
│  │  └── Result<T, String> — Typed error propagation               │  │
│  └────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────┤
│                         NATIVE LAYER                                 │
│                                                                      │
│  Rust (Edition 2021)                 Windows APIs                    │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  audio_management    COM → IAudioSessionManager2               │  │
│  │                      → ISimpleAudioVolume / IAudioEndpointVol  │  │
│  │  hardware_input      joyGetPosEx (axes) + hidapi (device names)│  │
│  │  lvar_input          WASM bridge scaffold (future)             │  │
│  │  native_menu         Win32 popup menu (TrackPopupMenu)         │  │
│  │  window_utils        DPI-aware positioning                     │  │
│  │  main                Tray, theming, layout, resize animation   │  │
│  │                                                                │  │
│  │  Concurrency: Dedicated threads, mpsc channels, AtomicBool     │  │
│  │  Resource Management: RAII via Drop (COM, HANDLE, caches)      │  │
│  └────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

### 2.2 Design Patterns

| Pattern | Implementation | Purpose |
|---------|---------------|---------|
| **RAII / Drop** | `ProcessHandle`, `AudioManager`, `HidInputManager` | Deterministic resource cleanup for COM objects, Windows handles, and caches |
| **Dedicated Thread + Channel** | `AudioThreadHandle` (mpsc), `input-poll` thread | Confines COM and joystick calls to their own threads; Tauri commands forward via channels |
| **Observer / Event Bus** | `tauri::Emitter` for `window-pin-changed`, `input-axis-data` | Backend-to-frontend state synchronisation and push-based input data |
| **Command** | 26 `#[tauri::command]` functions | Decoupled IPC interface with typed parameters and returns |
| **Throttle** | `scheduleLiveVolumeUpdate` (40ms minimum interval) | Rate-limited backend calls during slider interaction |
| **Bounded Cache** | `MAX_SESSION_CACHE_SIZE: 1000`, `MAX_CACHE_SIZE: 1000` | Memory leak prevention with automatic eviction |
| **Adapter** | `HidInputManager` combining Joystick API + HID API | Unified device abstraction merging data from two distinct APIs |
| **Interpolation / Easing** | `animateVolumeTo`, `animate_window_resize` | Cubic ease-out transitions for volume and window animations |
| **Guard / Activation** | `axisActivated` Map with 5% threshold | Prevents axis mapping from applying until deliberate user input |

### 2.3 Data Flow

**Hardware Input → Audio Change (complete cycle):**

```
Hardware Device (physical axis movement)
       │
       ▼
[1] Dedicated input-poll thread (50ms sleep loop) reads all devices
       │
       ▼
[2] Rust: joyGetPosEx reads raw axis value (0–65535)
       │
       ▼
[3] Rust: Normalise to 0.0–1.0, merge with HID device names
       │
       ▼
[4] Rust: app.emit("input-axis-data", &data) → Tauri event bus
       │
       ▼
[5] Frontend: listen("input-axis-data") callback receives AxisData[]
    → applyAxisMappings() compares against lastHardwareAxisValues
    ├── Change < 1%  →  Skip (dead-zone filtering)
    └── Change ≥ 1%  →  Check activation guard
        ├── Not activated (cumulative < 5%)  →  Skip
        └── Activated  →  Apply volume
            │
            ▼
[6] invoke("set_session_volume") → Tauri IPC → JSON serialisation
       │
       ▼
[7] Rust: Forward to audio-com thread via mpsc channel
       │
       ▼
[8] Audio thread: Find process_id from session cache
       │
       ▼
[9] Audio thread: Enumerate ALL audio devices, find ALL sessions for process
       │
       ▼
[10] Audio thread: ISimpleAudioVolume::SetMasterVolume(volume) on each session
       │
       ▼
[11] Frontend: startHardwareVolumeInterpolation()
     → requestAnimationFrame with exponential smoothing (factor: 0.3)
     → Visual slider converges to target
```

---

## 3. Technology Stack

### 3.1 Dependency Matrix

| Technology | Version | Role |
|-----------|---------|------|
| **Tauri** | 2.x | Desktop application framework, IPC, window management |
| **Rust** | Edition 2021 | Native backend, Windows API integration |
| **Svelte** | ^5.0.0 | Reactive frontend framework (runes-based) |
| **SvelteKit** | ^2.9.0 | Application scaffolding, static adapter |
| **TypeScript** | ~5.6.2 | Type-safe frontend logic |
| **Vite** | ^6.0.3 | Frontend build tooling |
| `windows` | 0.58 | Win32 API bindings (COM, Audio, DWM, Registry, Shell) |
| `hidapi` | 2.6 | HID device enumeration and identification |
| `window-vibrancy` | 0.5 | Windows Acrylic/Mica visual effects |
| `lazy_static` | 1.4 | Global singleton initialisation |
| `image` | 0.25 | PNG decoding for tray icons |
| `serde` / `serde_json` | 1.x | JSON serialisation for IPC |
| `@tauri-apps/api` | ^2.10.1 | Frontend Tauri bindings (invoke, listen) |

### 3.2 Stack Rationale

**Tauri 2.x** was selected over Electron for its significantly smaller binary footprint (leveraging the system's native WebView2 rather than bundling Chromium), first-class Rust integration enabling direct Windows API access without Node.js FFI overhead, and a refined security model with capability-based permissions.

**Rust** provides the memory safety guarantees and zero-cost abstractions required for an application interfacing with low-level COM objects, Windows handles, and HID devices. The ownership model ensures deterministic resource cleanup through RAII patterns — critical for COM objects that hold system-level audio references. Rust's `unsafe` blocks confine Win32 FFI calls to well-defined boundaries while the surrounding safe code prevents resource leaks.

**Svelte 5** with its runes-based reactivity system (`$state`, `$derived`, `$effect`) was selected for its compilation-time optimisation model. Unlike virtual-DOM frameworks that perform runtime diffing, Svelte compiles reactive declarations directly into targeted DOM update instructions. This architecture is particularly suited to the application's 20Hz polling pattern, where frequent state updates must propagate to the UI with minimal overhead.

**TypeScript** enforces structural type contracts across the IPC boundary: the TypeScript interfaces mirror the Rust `Serialize`/`Deserialize` structs, ensuring that data flowing through Tauri's JSON serialisation layer is structurally correct at compile time on both sides.

---

## 4. Core Systems

### 4.1 Audio Management

**Module:** `src-tauri/src/audio_management/mod.rs`

The audio management system provides per-application and system-level volume control through the Windows Core Audio API. All COM operations run on a single dedicated thread (`audio-com`) that owns the COM apartment, eliminating cross-thread COM access violations. Tauri commands forward requests to this thread via an `mpsc` channel and block on the reply.

**Thread Architecture:**

```
Tauri Command (any thread pool thread)
       │
       ▼
AudioThreadHandle::send_and_recv()
  → mpsc::Sender<AudioCommand>  ──→  audio-com thread
  ← mpsc::Receiver<Result<T>>   ←──  (processes command, sends reply)
```

The `AudioCommand` enum carries a reply channel per variant, enabling typed request/response pairs without shared mutable state.

**Cached COM Objects:**

The `AudioManager` caches three frequently used COM objects as struct fields:
- `IMMDeviceEnumerator` — created once on initialisation
- `IMMDevice` — the default audio endpoint
- `IAudioEndpointVolume` — the system volume interface

These are rebuilt only when `check_device_changed()` detects that the default endpoint has changed (e.g., headphones plugged/unplugged). This eliminates millions of redundant COM allocations over extended uptime.

**COM Interface Chain:**

```
CoInitializeEx(COINIT_APARTMENTTHREADED)
       │
       ▼
CoCreateInstance<IMMDeviceEnumerator>
       │
       ├── EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
       │          │
       │          ▼
       │   IMMDevice::Activate<IAudioSessionManager2>
       │          │
       │          ▼
       │   GetSessionEnumerator → IAudioSessionControl2 (PID, state)
       │                        → ISimpleAudioVolume (per-app volume)
       │
       └── GetDefaultAudioEndpoint(eRender, eConsole)
                  │
                  ▼
           IMMDevice::Activate<IAudioEndpointVolume> (system volume)
```

**Cached Endpoint Volume:**

System-level volume operations (master volume, system mute) use the cached `IAudioEndpointVolume` stored on the `AudioManager` struct. The cache is populated by `rebuild_com_cache()` which creates the full COM chain once:

```rust
fn rebuild_com_cache(&mut self) -> std::result::Result<(), String> {
    // Build the full chain: Enumerator → Device → EndpointVolume
    let enumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let endpoint_volume = device.Activate(CLSCTX_ALL, None)?;

    self.cached_enumerator = Some(enumerator);
    self.cached_device = Some(device);
    self.cached_endpoint_volume = Some(endpoint_volume);
    Ok(())
}
```

The helper `get_endpoint_volume()` returns a reference to the cached interface rather than creating a new one per call. Similarly, `get_enumerator()` provides cached access for session enumeration.

**Process-Based Volume Targeting:**

When setting volume or mute state, the system does not target individual session IDs alone. Instead, it resolves the `process_id` from the cached session, then enumerates all audio devices and applies the change to every session matching that process. This handles applications like Discord that maintain multiple audio sessions across different output devices (e.g., voice channel on headphones, notification sounds on speakers).

**RAII Resource Management:**

Windows process handles are wrapped in a newtype with a `Drop` implementation ensuring `CloseHandle` is called regardless of the code path:

```rust
struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe { let _ = CloseHandle(self.0); }
    }
}
```

The `AudioManager` itself implements `Drop` to clear internal caches and release cached COM objects. The COM library (`CoUninitialize`) is called by the `audio-com` thread after the manager is dropped, ensuring proper cleanup on the same thread that called `CoInitializeEx`.

**Session Cache:** Active sessions are maintained in a `HashMap<String, AudioSession>` bounded at `MAX_SESSION_CACHE_SIZE` (1,000 entries). When the cache exceeds this limit, it is pruned to 500 entries. Initial capacity is pre-allocated at 64 entries (`INITIAL_SESSION_CAPACITY`) to avoid early reallocations.

**Device Change Detection:** The `check_device_changed` method compares the current default audio endpoint's device ID against the stored ID, allowing the frontend to detect headphone plug/unplug events and refresh the session list accordingly. When a device change is detected, the cached COM objects are invalidated and rebuilt via `rebuild_com_cache()`.

### 4.2 Hardware Input

**Module:** `src-tauri/src/hardware_input/mod.rs`

The hardware input system reads axis positions and button states from game controllers using a dual-API approach that combines the Windows Joystick API for data acquisition with the HID API for device identification. All joystick and HID calls run on a dedicated `input-poll` thread, which pushes axis/button data to the frontend via `input-axis-data` Tauri events at ~50ms intervals. This eliminates cross-thread calls into `winmm.dll` that previously caused access violations.

**Dual-API Strategy:**

| API | Crate/Function | Provides |
|-----|---------------|----------|
| Windows Joystick API | `joyGetDevCapsW`, `joyGetPosEx` | Axis values (6 axes), button states (32 buttons), device presence |
| HID API | `hidapi` crate | Device name, manufacturer string, vendor ID, product ID |

The Joystick API supports up to 16 devices but provides only generic identifiers ("Joystick 1"). The HID API provides human-readable names ("Honeycomb Bravo Throttle Quadrant") but no axis/button data. The `HidInputManager` correlates devices across both APIs by matching vendor ID and product ID.

**Axis Normalisation:**

Raw Windows joystick values (0–65,535) are normalised to a 0.0–1.0 floating-point range:

```rust
let normalised = (raw_value as f32 / 65535.0).clamp(0.0, 1.0);
```

The POV hat switch is normalised from centidegrees (0–35,900 representing 0.0°–359.0°) to 0.0–1.0, and additionally decomposed into four discrete directional buttons (`POV_Up`, `POV_Right`, `POV_Down`, `POV_Left`) using 45°–135° angular ranges.

**Available Axes:** X, Y, Z, R (rudder), U (5th axis), V (6th axis), POV (hat switch).

**Caching Strategy:** Axis and button values are cached per device in `HashMap<u32, HashMap<String, f32>>` and `HashMap<u32, HashMap<String, bool>>` respectively. Caches are cleared on full device re-enumeration and serve as fallback values if a read operation fails. Memory is released via `shrink_to_fit()` during cleanup.

**Activation Guard (Frontend):** After a mapping is created, the bound axis does not immediately control volume. The `axisActivated` Map tracks whether each axis has moved more than 5% from its initial position since application startup. This prevents volume from jumping to an arbitrary position when the application launches with a physical axis resting at an intermediate value.

### 4.3 State Management

All reactive application state resides within the `+page.svelte` component as local `$state` rune declarations. The `lib/stores/` directory exports only utility functions (`formatProcessName`, `isSystemVolume`) and constants (`SYSTEM_VOLUME_ID`, `SYSTEM_VOLUME_PROCESS_NAME`, `SYSTEM_VOLUME_DISPLAY_NAME`) — it contains no reactive stores.

**Reactive State (~30 variables):**

State is categorised into four groups:

| Category | Examples | Persistence |
|----------|----------|-------------|
| **Backend Data** | `audioSessions`, `axisData` | Ephemeral (polled) |
| **User Configuration** | `axisMappings`, `buttonMappings`, `pinnedApps` | localStorage |
| **UI State** | `isEditMode`, `isBindingMode`, `windowPinned`, `dockOpen` | Ephemeral |
| **Internal** | `initStatus`, `audioInitialised`, `previousDisplayCount` | Ephemeral |

**Non-Reactive Caches (12 Map/Set instances):**

Performance-critical tracking state is stored in plain `Map` and `Set` instances (not wrapped in `$state`) to avoid triggering unnecessary UI re-renders:

| Cache | Type | Purpose |
|-------|------|---------|
| `previousAxisValues` | `Map<string, Record<string, number>>` | Axis snapshot for binding detection |
| `previousButtonStates` | `Map<string, Record<string, boolean>>` | Button snapshot for binding detection |
| `lastHardwareAxisValues` | `Map<string, number>` | Last applied axis value per mapping |
| `axisActivated` | `Map<string, boolean>` | Activation guard state per axis |
| `preMuteVolumes` | `Map<string, number>` | Volume before mute (for unmute restoration) |
| `animatingSliders` | `Set<string>` | Session IDs with active volume animations |
| `animationSignals` | `Map<string, AnimationSignal>` | Cancellation signals for volume animations |
| `manuallyControlledSessions` | `Set<string>` | Sessions being actively dragged by user |
| `hardwareVolumeTargets` | `Map<string, number>` | Target volumes for hardware interpolation |
| `hardwareVolumeAnimations` | `Map<string, number>` | `requestAnimationFrame` IDs for cleanup |
| `liveVolumeState` | `Map<string, LiveVolumeState>` | Throttle state for live volume updates |
| `memorySnapshots` | `Array<{timestamp, heapUsed, heapTotal}>` | Dev-mode memory profiler data |

All caches are bounded at `MAX_CACHE_SIZE` (1,000 entries) with enforcement checked every 30 seconds. Caches exceeding the limit are cleared entirely.

**Persistence Layer:**

User configuration is persisted to the WebView's `localStorage` under three keys:

```
clearcomms_axis_mappings    → JSON array of AxisMapping objects
clearcomms_button_mappings  → JSON array of ButtonMapping objects
clearcomms_pinned_apps      → JSON array of process name strings (from Set)
```

Values are loaded on mount and saved after every configuration change. The Tauri WebView maintains a persistent storage profile across application restarts.

**Derived Behaviour ($effect):**

Three `$effect` blocks handle derived state changes:

1. **Onboarding enforcement** — Activates edit mode when no applications are pinned
2. **Pin state synchronisation** — Fetches window pin state when settings menu opens
3. **Layout measurement** — Triggers frontend dimension measurement when channels render

### 4.4 Animation Systems

The application implements three distinct animation systems, each optimised for its specific use case:

**1. UI Volume Animation (`animateVolumeTo`)**

Smooths visual slider transitions for external volume changes, mute/unmute operations, and track clicks. Uses `requestAnimationFrame` with cubic ease-out easing:

```
eased = 1 - (1 - t)³
```

Each animation is associated with an `AnimationSignal` object providing cancellation semantics. When a new animation starts for the same session, the previous signal's `cancelled` flag is set, its `frameId` is passed to `cancelAnimationFrame`, and its associated Promise resolves with `false`. This prevents animation conflicts when multiple volume changes arrive in rapid succession.

**2. Hardware Volume Interpolation (`startHardwareVolumeInterpolation`)**

Smooths the visual representation of hardware-driven volume changes using exponential smoothing via `requestAnimationFrame`:

```
currentVolume += (targetVolume - currentVolume) × 0.3
```

The interpolation converges when the absolute difference falls below 0.001. Target values are stored in `hardwareVolumeTargets` and updated independently of the animation frame, allowing the physical input to change the target while the visual representation catches up smoothly.

**3. Window Resize Animation (Rust: `animate_window_resize`)**

Animates window width changes when the number of bound sessions changes. Runs on a singleton `window-anim` thread (lazily spawned on first resize) to avoid blocking the Tauri event loop. New resize requests cancel in-progress animations via channel drain and `try_recv()` mid-animation checks:

| Parameter | Value |
|-----------|-------|
| Duration | 500ms |
| Frame interval | ~4.2ms (~240fps) |
| Easing | Cubic ease-out: `1 - (1-t)³` |
| Anchor | Bottom-right (repositioned every frame) |

**4. Live Volume Update Throttle**

Slider drag interactions generate continuous `input` events. The `scheduleLiveVolumeUpdate` function throttles backend calls to a minimum interval of 40ms (maximum 25 calls/second), queuing the latest value and dispatching it when the interval elapses. This prevents IPC saturation while maintaining responsive visual feedback.

---

## 5. Performance and Optimisation

### 5.1 Release Profile

The Rust release build is configured for maximum optimisation:

```toml
[profile.release]
lto = "fat"           # Full link-time optimisation across all crates
codegen-units = 1     # Single codegen unit for maximum optimisation opportunity
panic = "abort"       # No stack unwinding overhead
opt-level = "z"       # Optimise for binary size
```

This configuration produces smaller binaries at the cost of longer compile times, appropriate for a desktop application where distribution size and runtime performance take priority over build iteration speed.

### 5.2 Memory Management

**Rust Backend:**

- Pre-allocated collections with `Vec::with_capacity()` at known initial sizes (`INITIAL_SESSION_CAPACITY: 64`, `INITIAL_DEVICE_CAPACITY: 16`, `INITIAL_HID_DEVICE_CAPACITY: 32`)
- `HashMap::shrink_to_fit()` called in cleanup methods to release excess capacity
- Session cache bounded at `MAX_SESSION_CACHE_SIZE: 1,000` with pruning to 500 when exceeded
- RAII `Drop` implementations for `AudioManager`, `HidInputManager`, and `ProcessHandle`

**Frontend:**

- 30-second memory monitor checks all Map/Set cache sizes against `MAX_CACHE_SIZE` (1,000)
- 5-minute periodic cleanup removes entries for sessions that no longer exist
- Full cleanup on component destroy: `stopPolling()`, `cleanupAllAnimations()`, `cleanupAllLiveVolumeStates()`, `cleanupAllCaches()`
- Dev-mode memory profiler samples `performance.memory` every 60 seconds (up to 120 snapshots), warning on >50% heap growth

### 5.3 Threading Model

| Thread | Responsibility |
|--------|---------------|
| Main (Tauri) | Event loop, window management, IPC dispatch |
| `audio-com` | Dedicated COM apartment thread; processes all audio commands via mpsc channel |
| `input-poll` | Dedicated joystick/HID polling thread; emits `input-axis-data` events at 50ms intervals |
| `window-anim` | Singleton animation thread (lazily spawned); 500ms cubic ease-out at ~240fps frames |
| `theme-monitor` | Background: polls Windows registry every 2 seconds; exits via `AtomicBool` shutdown flag |
| Frontend (JS) | Single-threaded event loop with `listen()` for hardware input and `setInterval` for audio polling |

The `audio-com` and `input-poll` threads are spawned during initialisation and run for the lifetime of the application. Tauri commands for audio operations are thin wrappers that forward to the audio thread via channel and block on the reply. Hardware input data is pushed to the frontend via Tauri events rather than pulled via `invoke()`.

### 5.4 Latency Budget

| Operation | Interval / Latency | Notes |
|-----------|-------------------|-------|
| Hardware axis polling | 50ms (20 Hz) | Dedicated `input-poll` thread with sleep loop |
| Audio session refresh | 200ms (5 Hz) | Session enumeration + device change detection |
| Live volume update throttle | 40ms minimum | Prevents IPC saturation during slider drag |
| Volume animation frame | ~16ms | `requestAnimationFrame` (monitor refresh rate) |
| Window resize frame | ~4.2ms (~240fps) | Singleton Rust thread, oversamples for smooth animation |
| Periodic cache cleanup | 300,000ms (5 min) | Removes stale session entries from all caches |
| Memory monitor | 30,000ms (30s) | Checks cache bounds, enforces MAX_CACHE_SIZE |
| Theme detection | 2,000ms | Windows registry poll for `AppsUseLightTheme` |

---

## 6. Security and Reliability

**Capability Model:** The application uses Tauri's capability-based permission system. The default capability grants `core:default` and `opener:default`. All 26 backend commands are exposed through the `invoke_handler` and accessed via Tauri's IPC channel rather than through web-accessible endpoints.

**Window Configuration:** The main window is configured with `decorations: false`, `transparent: true`, `shadow: false`, and `skipTaskbar: true`, operating as a utility overlay rather than a standard application window. Close requests are intercepted (`api.prevent_close()`) — the window hides rather than terminates, ensuring persistent background operation.

**Type Safety:** Every Tauri command returns `Result<T, String>`, propagating errors as human-readable messages to the frontend. The frontend wraps all `invoke()` calls in try/catch blocks. TypeScript strict mode and exhaustive interface definitions provide compile-time guarantees that IPC data structures match between frontend and backend.

**Error Propagation Pattern:**

```rust
#[tauri::command]
pub fn get_audio_sessions() -> Result<Vec<AudioSession>, String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::EnumerateSessions { reply }))
}
```

Tauri commands are thin wrappers that forward to the dedicated audio thread via `with_handle()`. The helper acquires the `AUDIO_THREAD_HANDLE` mutex, verifies the handle exists (returning a clear error if not initialised), then calls `send_and_recv()` which sends an `AudioCommand` variant with a oneshot reply channel and blocks until the audio thread responds. Every fallible operation uses the `?` operator with `map_err` to convert system errors into descriptive strings.

**Resource Cleanup Guarantees:**

| Layer | Mechanism | Scope |
|-------|-----------|-------|
| Rust COM | `audio-com` thread calls `CoUninitialize()` on exit | Process lifetime |
| Rust Handles | `ProcessHandle::Drop` calls `CloseHandle()` | Per-operation |
| Rust Caches | `cleanup()` with `shrink_to_fit()` | On demand / on drop |
| Frontend Events | `unlisten()` for Tauri event listeners in `stopPolling()` | `onDestroy` lifecycle |
| Frontend Animations | `cancelAnimationFrame()` in `cleanupAllAnimations()` | `onDestroy` lifecycle |
| Frontend Timers | Timer ID clearing in `cleanupAllLiveVolumeStates()` | `onDestroy` lifecycle |
| Frontend Caches | `.clear()` on all Maps/Sets in `cleanupAllCaches()` | `onDestroy` lifecycle |
| Tauri Listeners | Promise-based unlisten: `promise.then(fn => fn())` | `onMount` cleanup return |

---

## 7. Code Quality

**Static Analysis:**

| Tool | Scope | Target |
|------|-------|--------|
| `cargo clippy` | Rust linting | Zero warnings |
| `svelte-check` | Svelte + TypeScript checking | Zero errors, zero warnings |
| TypeScript strict mode | Frontend type safety | Enforced via `tsconfig.json` |

The codebase maintains a zero-warnings policy across both toolchains. The Rust backend compiles cleanly under Clippy's default lint set, and the Svelte frontend passes `svelte-check` without type errors or warnings.

**Coding Conventions:**

| Domain | Convention |
|--------|-----------|
| Rust identifiers | `snake_case` functions/variables, `PascalCase` structs/enums |
| TypeScript identifiers | `camelCase` functions/variables, `PascalCase` interfaces/components |
| Tauri command parameters | `snake_case` (Rust convention; frontend must match) |
| Comments | British English, explaining rationale ("why") over mechanics ("what") |
| Module boundaries | One public API per module directory (`mod.rs` with `pub fn`) |
| Error messages | Human-readable format strings: `"Failed to {action}: {cause}"` |

**Suppressed Lints:** A single `#[allow(dead_code)]` annotation exists on `DeviceInfo::num_axes` — the field is populated for completeness but not currently read by any consumer. The release build suppresses the Windows console window via `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.

---

## 8. Build and Deployment

**Development Workflow:**

```bash
npm run tauri dev     # Starts Vite dev server (port 1420) + cargo build
                      # Frontend: hot module replacement
                      # Backend: recompiles on save (~3-5s)
```

**Production Build:**

```bash
npm run tauri build   # Vite build → ../build/ (static assets)
                      # Cargo build with release profile (LTO fat)
                      # Generates installer and standalone binary
```

**Quality Checks:**

```bash
npm run check         # svelte-kit sync + svelte-check (TypeScript/Svelte)
cd src-tauri && cargo clippy   # Rust linting
```

**Tauri Window Configuration:**

```json
{
  "label": "main",
  "width": 600,
  "height": 600,
  "resizable": false,
  "decorations": false,
  "transparent": true,
  "shadow": false,
  "alwaysOnTop": false,
  "visible": false,
  "skipTaskbar": true,
  "center": false
}
```

The window starts hidden (`visible: false`) and is shown by clicking the system tray icon. `skipTaskbar: true` prevents the application from appearing in the Windows taskbar. `decorations: false` and `transparent: true` enable the custom acrylic glass visual treatment.

**Bundle Configuration:**

| Setting | Value |
|---------|-------|
| Identifier | `com.cameroncarlyon.clearcomms` |
| Targets | All (MSI, NSIS, standalone) |
| Icons | Dual-theme: `icons/white/` (dark mode) and `icons/black/` (light mode) |
| Frontend dist | `../build` (Vite output) |

---

## 9. Feature Specification

### 9.1 Hardware Axis Binding

**Binding Flow:**

1. Enter edit mode via the dock menu
2. Click "Bind Axis" on a session channel
3. The system snapshots all current axis values across all connected devices
4. Move a physical axis — the first axis to change by more than 5% is detected
5. A mapping is created linking `{deviceHandle, axisName}` → `{processName, sessionId}`
6. The mapping is persisted to `localStorage` and the application is pinned to the mixer

**Activation Guard:** After binding, the axis does not control volume until it has moved >5% from its position at application startup. This prevents the volume from jumping to an arbitrary position when the application launches with the physical axis at rest.

**Axis Inversion:** Each axis mapping supports an `inverted` boolean flag, allowing the volume direction to be reversed (useful when a physical slider's orientation is opposite to the expected direction).

### 9.2 Hardware Button Binding

**Binding Flow:** Identical to axis binding, but detects a rising edge (button press transition from `false` to `true`) rather than axis movement. Each button press toggles the mute state of the bound application.

**Mute Animation:** When muted, the volume slider animates to 0 over 200ms using cubic ease-out. The pre-mute volume is stored in `preMuteVolumes` for restoration on unmute. Backend `set_session_volume(0)` and `set_session_mute(true)` are dispatched as fire-and-forget calls before the animation starts, ensuring the actual audio change is instantaneous while the visual representation catches up.

### 9.3 System Volume Control

A virtual session with the identifier `__SYSTEM__` routes volume and mute operations to the Windows master endpoint volume (`IAudioEndpointVolume`) rather than per-application session volume (`ISimpleAudioVolume`). This is handled transparently — the frontend treats the system volume channel identically to application channels, with routing logic in the `invokeSetVolume` and `invokeSetMute` functions selecting the appropriate backend command based on the session ID.

### 9.4 Window Management

**System Tray:** The application lives in the Windows notification area with a theme-adaptive icon (white for dark mode, black for light mode). Left-click toggles visibility; right-click shows a native Win32 context menu with Show, Hide, Pin on Top, and Quit options.

**Focus Behaviour:** When unpinned, the window automatically hides on focus loss (`WindowEvent::Focused(false)`). A 200ms debounce prevents the tray click handler from immediately reopening a window that was just hidden by the focus loss event triggered by the tray click itself.

**Pin on Top:** The `always_on_top` window property is toggled via both the tray context menu and the frontend settings menu. State is synchronised through the `window-pin-changed` event emitted by the backend.

**Dynamic Resizing:** The window width adjusts to accommodate the number of bound sessions. The frontend measures its own rendered component dimensions and sends them to the backend via `update_layout_measurements`. The resize formula is:

```
width = base_width + (channel_width + channel_gap) × (session_count - 1)
```

Width values are in logical pixels, converted to physical pixels using the display's DPI scale factor.

### 9.5 Visual Design

**Acrylic Effect:** The `window-vibrancy` crate applies the Windows Acrylic material to the window background, providing a translucent blur effect consistent with modern Windows design language.

**Rounded Corners:** DWM window attributes are set to `DWMWCP_ROUND` via `DwmSetWindowAttribute`, giving the frameless window rounded corners matching the Windows 11 visual style.

**Theme-Adaptive Tray Icon:** A background thread polls the Windows registry key `Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme` every 2 seconds. When the theme changes, the tray icon is updated on the main thread via `app_handle.run_on_main_thread()`.

**Instant Show/Hide:** Window transition animations are disabled via `DWMWA_TRANSITIONS_FORCEDISABLED`, ensuring the window appears and disappears instantly when toggled via the system tray.

---

## 10. API Reference

### 10.1 Audio Management Commands

#### `init_audio_manager`

```rust
fn init_audio_manager() -> Result<String, String>
```

Spawns a dedicated `audio-com` thread that initialises the COM library with `COINIT_APARTMENTTHREADED`, creates an `AudioManager` instance with cached COM objects, and processes all subsequent audio commands via an `mpsc` channel. Stores the `AudioThreadHandle` (channel sender) in a global mutex. Returns a status message. Errors if the thread fails to start or COM initialisation fails.

#### `get_audio_sessions`

```rust
fn get_audio_sessions() -> Result<Vec<AudioSession>, String>
```

Enumerates all active audio sessions across all audio rendering devices. Returns a vector of `AudioSession` structs containing session ID, display name, process ID, process name, current volume (0.0–1.0), and mute state. Sessions with `process_id == 0` (system sounds) are excluded.

#### `set_session_volume`

```rust
fn set_session_volume(session_id: String, volume: f32) -> Result<(), String>
```

Sets the volume for all audio sessions matching the process ID of the specified session. The `volume` parameter is clamped to 0.0–1.0. Operates across all audio devices to handle multi-device applications.

**Error cases:** Manager not initialised, session not found in cache, COM interface acquisition failure.

#### `set_session_mute`

```rust
fn set_session_mute(session_id: String, muted: bool) -> Result<(), String>
```

Sets the mute state for all audio sessions matching the process ID of the specified session. Operates across all audio devices.

#### `check_default_device_changed`

```rust
fn check_default_device_changed() -> Result<bool, String>
```

Compares the current default audio endpoint device ID against the stored ID. Returns `true` if the device has changed (e.g., headphones plugged in), signalling the frontend to re-enumerate sessions.

#### `cleanup_audio_manager`

```rust
fn cleanup_audio_manager() -> Result<String, String>
```

Sends a cleanup command to the audio thread, which clears the session cache, releases excess memory via `shrink_to_fit()`, and returns a status message. Does not destroy the thread or uninitialise COM — those operations occur during application shutdown.

#### `get_system_volume`

```rust
fn get_system_volume() -> Result<f32, String>
```

Returns the system master volume level (0.0–1.0) via `IAudioEndpointVolume::GetMasterVolumeLevelScalar`.

#### `get_system_mute`

```rust
fn get_system_mute() -> Result<bool, String>
```

Returns the system master mute state via `IAudioEndpointVolume::GetMute`.

#### `set_system_volume`

```rust
fn set_system_volume(volume: f32) -> Result<(), String>
```

Sets the system master volume level (0.0–1.0) via `IAudioEndpointVolume::SetMasterVolumeLevelScalar`.

#### `set_system_mute`

```rust
fn set_system_mute(muted: bool) -> Result<(), String>
```

Sets the system master mute state via `IAudioEndpointVolume::SetMute`.

### 10.2 Hardware Input Commands

#### `init_input`

```rust
fn init_input(app: tauri::AppHandle) -> Result<String, String>
```

Initialises the HID API, enumerates all connected joystick devices (up to 16), correlates Joystick API devices with HID device names, and spawns a dedicated `input-poll` thread that reads all axes and buttons at 50ms intervals. The thread emits `input-axis-data` Tauri events via the supplied `AppHandle`. An `AtomicBool` shutdown flag is stored globally for clean exit. Returns a status message including detected device count.

#### `get_input_status`

```rust
fn get_input_status() -> Result<String, String>
```

Returns a human-readable status string indicating whether the input polling thread is running, shut down, or not yet initialised.

#### `enumerate_input_devices`

```rust
fn enumerate_input_devices() -> Result<Vec<String>, String>
```

Returns an empty vector. Retained for frontend compatibility during initialisation. The polling thread handles device discovery and hot-plug detection autonomously; device data (including names) is delivered via the `input-axis-data` event.

#### Tauri Event: `input-axis-data`

The dedicated `input-poll` thread emits this event at ~50ms intervals via `app.emit("input-axis-data", &data)`. The payload is `Vec<AxisData>` — normalised axis values (0.0–1.0) and boolean button states for all connected devices. The frontend listens via `listen<AxisData[]>('input-axis-data', callback)` rather than invoking a command.

#### `cleanup_input_manager`

```rust
fn cleanup_input_manager() -> Result<String, String>
```

Signals the input polling thread to shut down via its `AtomicBool` flag. The thread performs its own cleanup (clearing caches, releasing memory) before exiting. Returns a status message.

### 10.3 Window Management Commands

#### `update_layout_measurements`

```rust
fn update_layout_measurements(
    channel_width: u32,
    channel_gap: u32,
    base_width: u32,
) -> Result<String, String>
```

Stores frontend-measured layout dimensions (in logical pixels) for use in window width calculations. This allows the backend to compute accurate window sizes across different DPI scales without hardcoding pixel values.

#### `resize_window_to_content`

```rust
fn resize_window_to_content(app: AppHandle, session_count: usize) -> Result<String, String>
```

Calculates the target window width for the given session count, converts from logical to physical pixels using the display's scale factor, and spawns an animated resize thread if the current width differs from the target. Skips animation if already at the target size (within 1px tolerance).

#### `show_main_window`

```rust
fn show_main_window(app: AppHandle) -> Result<(), String>
```

Positions the window in the bottom-right corner, shows it, and sets focus.

#### `hide_main_window`

```rust
fn hide_main_window(app: AppHandle) -> Result<(), String>
```

Hides the main window.

#### `toggle_pin_window`

```rust
fn toggle_pin_window(app: AppHandle) -> Result<bool, String>
```

Toggles the `always_on_top` window property and returns the new state. Also positions, shows, and focuses the window.

#### `is_window_pinned`

```rust
fn is_window_pinned(app: AppHandle) -> Result<bool, String>
```

Returns the current `always_on_top` state.

### 10.4 Utility Commands

#### `restart_application`

```rust
async fn restart_application(app: AppHandle) -> Result<(), String>
```

Signals all background threads to shut down (theme monitor, audio thread, input thread), exits the current process, and spawns a new instance of the executable. Windows-only implementation using `std::process::Command`.

#### `quit_application`

```rust
fn quit_application(app: tauri::AppHandle)
```

Signals all background threads to shut down (theme monitor, audio thread, input thread) via their respective `AtomicBool` flags and channel shutdown commands, then terminates via `app.exit(0)`.

#### `open_url`

```rust
async fn open_url(url: String) -> Result<(), String>
```

Opens the specified URL in the default browser using `ShellExecuteW` with `SW_SHOWNORMAL`. Returns an error if `ShellExecuteW` returns a value ≤ 32.

---

## 11. Data Structures

### 11.1 TypeScript Interfaces

```typescript
/** Audio session from the Windows Core Audio API */
interface AudioSession {
  session_id: string;       // Unique Windows session identifier
  display_name: string;     // Friendly name from Windows
  process_id: number;       // Windows process ID
  process_name: string;     // e.g., "Discord.exe"
  volume: number;           // 0.0 to 1.0
  is_muted: boolean;
}

/** Axis-to-volume binding configuration */
interface AxisMapping {
  deviceHandle: string;     // Joystick device ID (0-15)
  deviceName: string;       // Human-readable device name
  axisName: string;         // X, Y, Z, R, U, V, or POV
  sessionId: string;        // Target audio session ID
  sessionName: string;      // Display name at time of binding
  processId: number;        // Target process ID
  processName: string;      // Target process name
  inverted: boolean;        // Reverse axis direction
}

/** Button-to-mute binding configuration */
interface ButtonMapping {
  deviceHandle: string;
  deviceName: string;
  buttonName: string;       // Button1-32 or POV direction
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
}

/** Hardware device axis and button data */
interface AxisData {
  device_handle: string;
  device_name: string;
  manufacturer: string;
  product_id: number;
  vendor_id: number;
  axes: Record<string, number>;      // Axis name → normalised value (0.0-1.0)
  buttons: Record<string, boolean>;  // Button name → pressed state
}

/** Pending axis binding state */
interface PendingBinding {
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
}

/** Pending button binding state */
interface PendingButtonBinding {
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
}

/** Throttle state for live volume IPC calls */
interface LiveVolumeState {
  inFlight: boolean;        // Whether an invoke() call is pending
  lastSent: number;         // Timestamp of last sent update
  queuedVolume?: number;    // Latest queued value (sent when interval elapses)
  timerId?: number;         // setTimeout ID for delayed send
}

/** Cancellation signal for volume animations */
interface AnimationSignal {
  cancelled: boolean;               // Set to true to cancel
  resolve?: (completed: boolean) => void;  // Promise resolver
  frameId?: number;                 // requestAnimationFrame ID
}

/** Chromium memory API (dev-mode profiling) */
interface MemoryInfo {
  jsHeapSizeLimit?: number;
  totalJSHeapSize?: number;
  usedJSHeapSize?: number;
}
```

### 11.2 Rust Structures

```rust
/// Audio session data (serialised to frontend via JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSession {
    pub session_id: String,
    pub display_name: String,
    pub process_id: u32,
    pub process_name: String,
    pub volume: f32,
    pub is_muted: bool,
}

/// Audio subsystem manager — lives exclusively on the dedicated audio-com thread
struct AudioManager {
    sessions: HashMap<String, AudioSession>,
    current_device_id: String,
    enumerate_calls: usize,
    last_logged_counts: Option<(usize, usize)>,
    /// Cached COM objects — only recreated on device change
    cached_enumerator: Option<IMMDeviceEnumerator>,
    cached_device: Option<IMMDevice>,
    cached_endpoint_volume: Option<IAudioEndpointVolume>,
}

/// RAII wrapper for Windows HANDLE
struct ProcessHandle(HANDLE);

/// Frontend layout dimensions for window sizing
#[derive(Debug, Clone)]
struct LayoutMeasurements {
    channel_width: u32,     // Default: 48px
    channel_gap: u32,       // Default: 48px
    base_width: u32,        // Default: 250px
}

/// Hardware device axis and button data (serialised to frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisData {
    pub device_handle: String,
    pub device_name: String,
    pub manufacturer: String,
    pub product_id: u16,
    pub vendor_id: u16,
    pub axes: HashMap<String, f32>,
    pub buttons: HashMap<String, bool>,
}

/// Hardware device metadata
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: u32,
    pub name: String,
    pub manufacturer: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub num_axes: u32,
    pub num_buttons: u32,
}

/// Hardware input subsystem manager — lives exclusively on the dedicated input-poll thread
pub struct HidInputManager {
    devices: Vec<DeviceInfo>,
    axis_cache: HashMap<u32, HashMap<String, f32>>,
    button_cache: HashMap<u32, HashMap<String, bool>>,
    hid_api: HidApi,
    known_joy_ids: Vec<u32>,
    last_hotplug_check: Option<Instant>,
}
```

---

## 12. Architecture Decisions

| Decision | Selection | Rationale |
|----------|-----------|-----------|
| Desktop framework | Tauri 2.x | ~5MB binary vs ~150MB (Electron); native Rust backend; system WebView2 |
| Frontend framework | Svelte 5 | Compiler-optimised reactivity; no virtual DOM; minimal bundle size |
| Backend language | Rust | Memory safety for COM interop; zero-cost abstractions; deterministic Drop |
| State management | Local `$state` runes | Single-page app; no prop drilling; co-located with IPC calls |
| Persistence | `localStorage` | Three small JSON keys; instant reads; no database dependency |
| Hardware poll rate | 50ms (20 Hz) via dedicated thread | Responsive for volume knobs; push-based via Tauri events |
| Audio monitor rate | 200ms (5 Hz) | Detects external changes without excessive COM calls |
| Volume update throttle | 40ms | Prevents flooding Windows audio API during slider drag |
| Window behaviour | Hide on focus loss | Widget-like UX; system tray as primary access point |
| Close behaviour | Hide instead of exit | Persistent background operation; tray icon always available |
| COM threading | Dedicated `audio-com` thread with `COINIT_APARTMENTTHREADED` | All COM calls confined to one thread; eliminates cross-thread access violations |
| Tray menu | Native Win32 (`TrackPopupMenu`) | Consistent with Windows shell UX; avoids web-rendered menus |
| Device identification | Dual API (Joystick + HID) | Joystick API provides data; HID API provides human-readable names |
| Binary optimisation | LTO fat + `opt-level = "z"` | Minimised binary size for distribution |
| Input data delivery | Push via Tauri events | Eliminates cross-thread `invoke()` for axis data; frontend uses `listen()` |

---

## 13. Future Considerations

**SimConnect / LVar Integration:** The `lvar_input` module is scaffolded with documentation comments defining the integration surface for reading and writing Microsoft Flight Simulator local variables via a WASM bridge (currently targeting the MobiFlight Event Module). This integration would enable mapping cockpit audio panel controls (VHF1/INT/CAB volume knobs) directly to application volumes, eliminating the need for external hardware for aircraft-equipped audio panels.

**Multi-Profile Support:** The persistence layer could be extended to support named configuration profiles, allowing different axis/button mappings per aircraft type. The current `localStorage` keys would be namespaced by profile identifier.

**Network Remote Control:** A WebSocket or TCP server could expose the volume control API to external clients, enabling tablet-based or touchscreen-based remote mixing from a secondary device. The existing command architecture would require minimal adaptation.

**Plugin Architecture:** The modular Rust backend (separate modules for audio, hardware, LVar) is structured to support dynamic loading of additional input/output adapters. Future input sources (MIDI controllers, network streams) could be implemented as independent modules conforming to a standardised trait interface.

**Cross-Platform Support:** The Rust backend uses `#[cfg(target_os = "windows")]` guards on all platform-specific code, with stub implementations for non-Windows targets. macOS support would require CoreAudio integration in `audio_management` and IOKit/HID integration in `hardware_input`, while the frontend and IPC layer remain platform-independent.

---

*ClearComms — Technical Documentation v0.1.0*
