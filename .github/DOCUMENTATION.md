# ClearComms — Technical Documentation

**Version:** 1.0.0
**Platform:** Windows 10/11
**Architecture:** Tauri 2.x | Rust | Svelte 5 | TypeScript
**Last Updated:** August 2026

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
   - 4.3 [Simulator Detection](#43-simulator-detection)
   - 4.4 [SimConnect and MobiFlight Integration](#44-simconnect-and-mobiflight-integration)
   - 4.5 [Sim Channel Synchronisation](#45-sim-channel-synchronisation)
   - 4.6 [State Management](#46-state-management)
   - 4.7 [Animation Systems](#47-animation-systems)
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
   - 9.3 [Sim Channel Assignment](#93-sim-channel-assignment)
   - 9.4 [System Volume Control](#94-system-volume-control)
   - 9.5 [Window Management](#95-window-management)
   - 9.6 [Visual Design](#96-visual-design)
10. [API Reference](#10-api-reference)
    - 10.1 [Audio Management Commands](#101-audio-management-commands)
    - 10.2 [Hardware Input Commands](#102-hardware-input-commands)
    - 10.3 [Simulator Commands](#103-simulator-commands)
    - 10.4 [Window Management Commands](#104-window-management-commands)
    - 10.5 [Utility Commands](#105-utility-commands)
    - 10.6 [Tauri Events](#106-tauri-events)
11. [Data Structures](#11-data-structures)
    - 11.1 [TypeScript Interfaces](#111-typescript-interfaces)
    - 11.2 [Rust Structures](#112-rust-structures)
12. [Architecture Decisions](#12-architecture-decisions)
13. [Future Considerations](#13-future-considerations)

---

## 1. Project Overview

ClearComms is a high-performance desktop application providing real-time per-application audio mixing control for Microsoft Flight Simulator environments. The application enables two independent forms of control:

- **Hardware binding** — mapping physical axes (knobs, sliders, throttle levers) and buttons on flight simulation peripherals directly to individual Windows audio sessions.
- **Sim channel assignment** — linking a Windows audio session to a flightdeck radio channel (COM1–3, HF1–2, CAB, PA, INT) so that the aircraft's own audio control panel and the application volume remain synchronised in both directions.

The application addresses a specific workflow gap in the flight simulation ecosystem: pilots running multiple audio applications simultaneously (vPilot for online ATC, Discord for group communication, GSX for ground services, MSFS for game audio) must typically Alt-Tab to the Windows Volume Mixer to adjust levels. ClearComms eliminates this context switch by providing a persistent, hardware-driven audio control surface that operates transparently alongside the simulator.

The system is built on a three-tier architecture: a Rust native backend providing direct Windows API integration (Core Audio, Joystick, HID, SimConnect), a Tauri 2.x IPC bridge for type-safe command invocation with JSON serialisation, and a Svelte 5 reactive frontend with runes-based state management. This architecture delivers sub-10ms input-to-audio latency, a steady-state memory footprint well under 100MB, and a release binary optimised for minimal size through fat link-time optimisation.

A defining constraint runs through the entire design: ClearComms shares a machine with one of the most CPU-hungry consumer applications in existence. Every subsystem that can be event-driven is event-driven. Audio state reaches the frontend by push notification rather than polling; the SimConnect dispatch loop and the simulator process watcher block on Win32 event handles rather than spinning; simulator variables are subscribed with change detection performed inside the simulator process. While connected and idle, the simulator integration performs no work at all.

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
│  │  ├── $state runes for reactive UI state                        │  │
│  │  ├── $derived maps for sim function routing                    │  │
│  │  ├── Map/Set caches for non-reactive performance state         │  │
│  │  ├── Tauri event listeners: input-axis-data,                   │  │
│  │  │    audio-state-updated, lvar-value-changed (no polling)     │  │
│  │  ├── requestAnimationFrame volume animations                   │  │
│  │  └── localStorage (5 keys) + ui-state.json via IPC             │  │
│  │                                                                │  │
│  │  Reusable Components (props-down, events-up)                   │  │
│  │  └── aircraftProfiles.ts — aircraft → LVar registry            │  │
│  └────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────┤
│                       COMMUNICATION LAYER                            │
│                                                                      │
│  Tauri 2.x IPC                                                       │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  31 registered commands via invoke_handler                     │  │
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
│  │                      + IMMNotificationClient (push events)     │  │
│  │  hardware_input      joyGetPosEx (axes) + hidapi (device names)│  │
│  │  sim_detection       Toolhelp32 process watcher (MSFS 2020/24) │  │
│  │  simconnect          SimConnect dispatch loop + MobiFlight     │  │
│  │                      ClientDataArea LVar transport             │  │
│  │  native_menu         Win32 popup menu (TrackPopupMenu)         │  │
│  │  theme               Registry-driven light/dark resolution     │  │
│  │  notification        Native Windows toast (WinRT ToastNotif.)  │  │
│  │  window_utils        DPI-aware positioning                     │  │
│  │  main                Tray, layout, resize animation, config    │  │
│  │                                                                │  │
│  │  Concurrency: Dedicated threads, mpsc channels, Win32 events   │  │
│  │  Resource Management: RAII via Drop (COM, HANDLE, caches)      │  │
│  └────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

### 2.2 Design Patterns

| Pattern | Implementation | Purpose |
|---------|---------------|---------|
| **RAII / Drop** | `ProcessHandle`, `AudioManager`, `HidInputManager`, `ComApartment` | Deterministic resource cleanup for COM objects, Windows handles, and caches |
| **Dedicated Thread + Channel** | `AudioThreadHandle` (mpsc), `input-poll`, `simconnect` | Confines COM, joystick and SimConnect calls to their owning threads; Tauri commands forward via channels |
| **Blocking Wait Loop** | `WaitForMultipleObjects` in the SimConnect dispatch loop | Zero CPU while idle; every reason to wake is a kernel event |
| **Observer / Event Bus** | `tauri::Emitter` for `audio-state-updated`, `input-axis-data`, `lvar-value-changed`, `sim-status-changed`, `window-pin-changed` | Push-based backend-to-frontend synchronisation, replacing frontend polling |
| **Lifecycle Controller** | `simconnect-ctrl` thread consuming `SimDetectionEvent` | Spawns and tears down the SimConnect connection in response to simulator process transitions |
| **Command** | 31 `#[tauri::command]` functions | Decoupled IPC interface with typed parameters and returns |
| **Throttle** | `scheduleLiveVolumeUpdate` (40ms), `writeSimVolume` (120ms trailing) | Rate-limited backend and simulator writes during slider interaction |
| **Bounded Cache** | `MAX_SESSION_CACHE_SIZE: 1000`, `MAX_CACHE_SIZE: 1000`, `MAX_LVAR_SUBSCRIPTIONS: 64` | Memory leak prevention with automatic eviction and bounded ID ranges |
| **Adapter** | `HidInputManager` combining Joystick API + HID API | Unified device abstraction merging data from two distinct APIs |
| **Interpolation / Easing** | `animateVolumeTo`, `animate_window_resize` | Cubic ease-out transitions for volume and window animations |
| **Guard / Activation** | `axisActivated` Map (5% threshold), `lvarsSeenNonZero` | Prevents a mapping or LVar from applying until it has proven itself deliberate |
| **Echo Suppression** | `simVolumeWrites` window, `isSimFunctionLocallyDriven` | Distinguishes an inbound value that is our own write returning from a genuine cockpit movement |
| **Exponential Backoff** | `next_retry_delay_ms` (5s → 60s), LVar subscription retry | Prevents repeated failure from churning inside the simulator process |
| **Event-Driven Cleanup** | `notification::show`'s `on_dismissed`/`on_activated` handlers | Removes the toast from the Action Centre when its popup ends, with no polling timer |

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

**Flightdeck Knob → Audio Change (sim → app):**

```
Pilot turns the VHF1 volume knob in the flightdeck
       │
       ▼
[1] MobiFlight WASM module writes the new LVar value into the
    ClearComms.LVars ClientDataArea inside the simulator process
       │
       ▼
[2] SimConnect delivers it only if the value changed
    (PERIOD_ON_SET + REQUEST_FLAG_CHANGED — diffing happens in-sim)
       │
       ▼
[3] Win32 event handle signalled → simconnect thread wakes from
    WaitForMultipleObjects and drains GetNextDispatch until E_FAIL
       │
       ▼
[4] Rust: slot index → LVar name, compared against last emitted value
    → app.emit("lvar-value-changed", { name, value })
       │
       ▼
[5] Frontend: lvarRouteByName maps the LVar to every bound channel
    ├── Session manually held by pointer  →  Skip (user's grip wins)
    ├── Sim function locally driven (<500ms)  →  Skip (our own echo)
    ├── Value is 0 and LVar never seen non-zero  →  Skip (sim still loading)
    └── Otherwise → normaliseVolume() → applyLvarVolume()
            │
            ▼
[6] animateVolumeTo(…, 'lvar') then invoke("set_session_volume")
    → audio-com thread → ISimpleAudioVolume::SetMasterVolume
```

**Slider Drag → Flightdeck Knob (app → sim):**

```
User drags the ClearComms volume slider
       │
       ▼
[1] markSimFunctionLocalInput() claims the sim function for local input,
    suppressing inbound LVar events for the duration of the gesture
       │
       ▼
[2] writeSimVolume() — 120ms trailing throttle per LVar
    (writeSimVolumeFinal bypasses the throttle at gesture end)
       │
       ▼
[3] denormaliseVolume() into the aircraft's native range
    (0–1 analogue, or rounded for 0–100 integer knobs)
       │
       ▼
[4] invoke("set_sim_lvar") → validate_lvar_name() → mpsc queue
    → SetEvent wakes the simconnect dispatch loop
       │
       ▼
[5] Connection thread writes "MF.SimVars.Set.<value> (>L:<name>)"
    into the ClearComms.Command ClientDataArea
       │
       ▼
[6] MobiFlight WASM module executes the calculator code; the knob moves
       │
       ▼
[7] The resulting inbound echo is recognised by value and window
    (LVAR_ECHO_EPSILON_RATIO / LVAR_ECHO_WINDOW_MS) and dropped
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
| `windows` | 0.58 | Win32 API bindings (COM, Audio, DWM, Registry, Shell, ToolHelp) |
| `simconnect-sys` | 0.24.3 | Raw SimConnect FFI bindings (statically linked) |
| `hidapi` | 2.6 | HID device enumeration and identification |
| `window-vibrancy` | 0.5 | Windows Acrylic/Mica visual effects |
| `image` | 0.25 | PNG decoding for tray icons |
| `serde` / `serde_json` | 1.x | JSON serialisation for IPC and persisted UI config |
| `tracing` / `tracing-subscriber` / `tracing-appender` | 0.1 / 0.3 / 0.2 | Structured, level-filtered logging with file rotation |
| `tauri-plugin-single-instance` | 2 | Prevents a second instance; forwards the launch to the running one |
| `base64` | 0.22 | Encoding for embedded assets |
| `@tauri-apps/api` | ^2.10.1 | Frontend Tauri bindings (invoke, listen) |

The `simconnect-sys` crate is used with the `static` feature, so `SimConnect.dll` is linked into the binary rather than shipped alongside it or resolved from the simulator installation at runtime.

### 3.2 Stack Rationale

**Tauri 2.x** was selected over Electron for its significantly smaller binary footprint (leveraging the system's native WebView2 rather than bundling Chromium), first-class Rust integration enabling direct Windows API access without Node.js FFI overhead, and a refined security model with capability-based permissions.

**Rust** provides the memory safety guarantees and zero-cost abstractions required for an application interfacing with low-level COM objects, Windows handles, and HID devices. The ownership model ensures deterministic resource cleanup through RAII patterns — critical for COM objects that hold system-level audio references. Rust's `unsafe` blocks confine Win32 FFI calls to well-defined boundaries while the surrounding safe code prevents resource leaks.

**Svelte 5** with its runes-based reactivity system (`$state`, `$derived`, `$effect`) was selected for its compilation-time optimisation model. Unlike virtual-DOM frameworks that perform runtime diffing, Svelte compiles reactive declarations directly into targeted DOM update instructions. This architecture is particularly suited to the application's push-driven update pattern, where hardware, audio and simulator events arrive asynchronously and must propagate to the UI with minimal overhead.

**SimConnect** is the simulator's official client interface and the only supported route to aircraft state that does not depend on reverse-engineering an add-on's internals. The raw `simconnect-sys` FFI bindings are used rather than a higher-level wrapper because ClearComms needs direct control over two things that wrappers typically abstract away: the event handle passed to `SimConnect_Open`, which is what makes the dispatch loop blocking rather than polling, and the ClientDataArea calls that carry MobiFlight traffic.

**The MobiFlight WASM Event Module** provides the bridge to aircraft-specific LVars. Reading them requires code running inside the simulator process, and MobiFlight's module is the established, widely-installed solution — the alternative being a bespoke WASM module that every user would have to install separately. ClearComms registers its own named client (`ClearComms`) rather than sharing the default one, so its subscriptions are isolated from the MobiFlight Connector application: in particular `MF.SimVars.Clear` only clears the issuing client's variables, so the two can run side by side without disturbing each other.

**TypeScript** enforces structural type contracts across the IPC boundary: the TypeScript interfaces mirror the Rust `Serialize`/`Deserialize` structs, ensuring that data flowing through Tauri's JSON serialisation layer is structurally correct at compile time on both sides.

---

## 4. Core Systems

### 4.1 Audio Management

**Module:** `src-tauri/src/audio_management/mod.rs`

The audio management system provides per-application and system-level volume control through the Windows Core Audio API. All COM operations run on a single dedicated thread (`audio-com`) that owns the COM apartment, eliminating cross-thread COM access violations. Tauri commands forward requests to this thread via an `mpsc` channel and block on the reply.

Session state reaches the frontend by push rather than by poll. The `audio-com` thread calls `emit_if_changed()`, which enumerates sessions and emits an `audio-state-updated` event **only when the resulting list differs from the last one emitted**. The frontend subscribes with `listen()` and holds no polling timer for audio at all.

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

**Device Change Detection:** The `check_device_changed` method compares the current default audio endpoint's device ID against the stored ID. When a device change is detected, the cached COM objects are invalidated and rebuilt via `rebuild_com_cache()`.

**Notification Thread (`audio-notify`):** A second dedicated thread initialises COM as **MTA** and registers an `IMMNotificationClient` to receive audio topology changes (device added, removed, default endpoint changed). MTA is used deliberately: MTA callbacks are delivered directly on the MMDevice notification thread, whereas an STA registration would require a Win32 message pump on the `audio-com` thread to receive anything. The callback sets a shared `AtomicBool`; the `audio-com` thread observes the flag, rebuilds its COM cache if the default endpoint moved, and re-emits.

The registration is owned by this thread and unregistered there on shutdown. This matters: a notification client that outlives the interfaces it references is a use-after-free, and an earlier revision of this code produced intermittent `0xC0000005` access violations for exactly that reason. `AudioManager::rebuild_com_cache` deliberately registers no callbacks of its own.

**Wake Cadence:** The `audio-com` thread blocks in `recv_timeout(FLAG_CHECK_INTERVAL)` — 500ms — so a topology notification is observed promptly without a dedicated wait handle. A `SAFETY_NET_INTERVAL` of 10 seconds triggers a proactive enumeration to catch changes COM notifications do not cover, most notably another application adjusting a per-session volume through the Windows Volume Mixer. Both are cheap: neither emits anything unless the session list has actually changed.

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

### 4.3 Simulator Detection

**Module:** `src-tauri/src/sim_detection/mod.rs`

Before any SimConnect work can happen, the application needs to know whether a simulator is running. This is done with periodic Toolhelp32 process snapshots — a deliberately unglamorous approach chosen because it requires no COM, no WMI, and no administrator privileges, and cannot fail in ways that leave the application in an unrecoverable state.

The `sim-detection` thread takes a snapshot every two seconds, sleeping in `WaitForSingleObject` on its shutdown event in between, so it consumes no CPU whilst waiting and exits instantly when signalled. A one-shot scan runs at startup so a simulator that was already running is detected immediately.

| Executable | Version reported |
|-----------|------------------|
| `FlightSimulator.exe` | MSFS 2020 |
| `FlightSimulator2024.exe` | MSFS 2024 |

Transitions are published as `SimDetectionEvent::Started(version)` / `Stopped` over an `mpsc` channel. Two helpers are exposed for use elsewhere: `scan_for_running_sim()` returns the running version, and `scan_for_running_sim_with_pid()` additionally returns the process ID, which the connection thread uses to obtain a `PROCESS_SYNCHRONIZE` handle.

### 4.4 SimConnect and MobiFlight Integration

**Module:** `src-tauri/src/simconnect/` (`mod.rs` lifecycle and commands, `connection.rs` dispatch loop and FFI, `state.rs` shared state types)

#### Design Constraint

MSFS is among the most CPU-hungry applications a consumer machine will ever run. Every cycle ClearComms spends is a cycle potentially taken from the simulator, so the entire integration is built on a single principle: **sleep until the simulator says something changed**. There is no polling loop anywhere in this subsystem. Three layers enforce this:

1. **SimConnect data requests** use `SIMCONNECT_DATA_REQUEST_FLAG_CHANGED`, so the diffing happens inside the simulator process and no message is sent when a value has not moved.
2. **The dispatch loop** blocks in `WaitForMultipleObjects` on kernel event handles. The thread is descheduled at the OS level and consumes nothing until Windows wakes it.
3. **MobiFlight subscriptions** use `SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET`, so the WASM module pushes data only when it writes — not on a timer, not per frame.

#### Lifecycle

The SimConnect thread is not started at application startup. A lifecycle controller (`simconnect-ctrl`) blocks on the detection channel and spawns or tears down the connection in response to simulator process transitions:

```
sim-detection thread                simconnect-ctrl                  simconnect thread
       │                                   │                                │
       │  SimDetectionEvent::Started ─────►│                                │
       │                                   │──── spawn ────────────────────►│
       │                                   │                                │ SimConnect_Open
       │                                   │                                │ dispatch loop
       │  SimDetectionEvent::Stopped ─────►│                                │
       │                                   │──── SetEvent(shutdown) ───────►│
       │                                   │◄─── join ──────────────────────┘
```

**Connection retry.** When MSFS launches, its SimConnect IPC server takes several seconds to become ready after the process appears, and `SimConnect_Open` fails with `E_FAIL` during that window. Rather than adding a retry loop inside the connection thread, a failed attempt re-injects a `Started` event into the detection channel after a delay, so retry flows through the same event-driven path as a genuine start. The delay is implemented with `WaitForSingleObject` on the shutdown event, making it instantly interruptible.

The delay doubles per consecutive failed attempt, from `RETRY_DELAY_MS` (5s) up to `RETRY_DELAY_MAX_MS` (60s), and resets via `note_connection_established()` as soon as a connection actually opens. Without the backoff, a condition that ends the dispatch loop immediately and repeatably would re-open SimConnect, re-register the MobiFlight client and rebuild every subscription every five seconds indefinitely — all of it work performed inside the simulator process.

#### Dispatch Loop

The loop waits on up to four handles simultaneously:

| Handle | Signalled when |
|--------|----------------|
| SimConnect event handle | Messages are waiting (passed as `hEventHandle` to `SimConnect_Open`) |
| Shutdown event | The application is quitting or the simulator has stopped |
| LVar wake event | The frontend has queued a subscribe or set command |
| Simulator process handle | The simulator process exits (`PROCESS_SYNCHRONIZE`) |

Waiting on the simulator's own process handle turns its exit into an event rather than something to be discovered by a later process scan. When that handle is available the loop needs no periodic work at all, so the timeout is a 60-second backstop (`WAIT_TIMEOUT_WATCHED_MS`) against an orphaned event handle. If `OpenProcess` fails, the loop falls back to a 5-second timeout (`WAIT_TIMEOUT_POLLED_MS`) and process scans, requiring two consecutive negative scans before tearing down — a single failed enumeration would otherwise cost a full reconnect.

On a SimConnect wake, the loop drains `GetNextDispatch` in a tight inner loop until the queue reports empty, then returns to waiting. `E_FAIL` (`0x80004005`) is the **normal** termination of that drain, signalling an empty queue rather than an error. Calling `GetNextDispatch` only once per wake would leave messages queued and add latency.

#### MobiFlight Handshake

```
[1] SIMCONNECT_RECV_ID_OPEN
       ├── version sanity check (major 11 = 2020, 12 = 2024)
       ├── register TITLE SimVar (PERIOD_SECOND + FLAG_CHANGED)
       └── map MobiFlight.Command / MobiFlight.Response ClientDataAreas
              subscribe Response with PERIOD_ON_SET
       │
       ▼
[2] Send "MF.Ping" on MobiFlight.Command
    + a one-shot PERIOD_ONCE read of the Response area
       │
       ▼
[3] "MF.Pong" received  →  WasmState::Present
       └── send "MF.Clients.Add.ClearComms"
       │
       ▼
[4] "MF.Clients.Add.ClearComms.Finished" received
       └── map ClearComms.Command / .LVars / .Response areas
           client_ready = true; flush any pending subscription
```

The one-shot `PERIOD_ONCE` read in step 2 exists for a specific failure case: when MSFS was already running before ClearComms started, the `MobiFlight.Response` area may still hold `MF.Pong` from a previous session. Sending a new ping causes the module to write the identical value, and SimConnect suppresses the `ON_SET` notification because nothing changed — leaving the handshake stalled. The `ONCE` read bypasses change detection and returns the buffer's current contents unconditionally. Because that buffer may equally hold something stale and unrelated, only `MF.Pong` is trusted from that path.

There is deliberately **no periodic ping**. Health is confirmed once at connection, after which the `ON_SET` subscription is the ongoing signal. A timer-based health check would wake a thread on a fixed interval regardless of simulator state, which is precisely what this design exists to avoid.

#### LVar Subscription

Subscriptions are replaced wholesale rather than incrementally. `apply_lvar_subscriptions` clears the client's registrations with `MF.SimVars.Clear`, tears down each previously used slot, then re-registers each name as `MF.SimVars.Add.(L:<name>)` and subscribes to its float slot in the `ClearComms.LVars` area. Each LVar occupies a 4-byte float at offset `index × 4`, and the subscription set is bounded at `MAX_LVAR_SUBSCRIPTIONS` (64) to keep definition and request ID ranges bounded.

Tearing down the old slot before redefining it is essential: both the request and the definition persist for the life of the connection, and re-adding a datum to an existing definition raises `SIMCONNECT_EXCEPTION_DUPLICATE_ID`, leaving that slot unusable for the remainder of the session. Requests are cancelled (`PERIOD_NEVER`) before their definitions are cleared, so no live request ever references a freed definition.

The standing subscription uses `PERIOD_ON_SET` **with** `REQUEST_FLAG_CHANGED`. The flag matters more than it might appear: the MobiFlight module rewrites every registered variable on each tick whether or not it moved, so without `CHANGED` the connection thread would wake tens of times per second purely to discard identical data. Because `CHANGED` reports only movement, each slot is additionally primed with a one-shot `PERIOD_ONCE` read on a separate request ID so the current level is known immediately rather than whenever the knob is next touched. That prime is skipped when the slot previously held a different variable, since the module may not yet have written the new variable to that offset — applying the previous variable's value to a new channel would be worse than waiting one tick.

#### Input Validation

LVar names are embedded into MobiFlight calculator-code strings (`MF.SimVars.Add.(L:<name>)`, `MF.SimVars.Set.<value> (>L:<name>)`) that are executed inside the simulator. `validate_lvar_name` is therefore the injection boundary: names must be 1–128 characters of ASCII alphanumerics and underscores only. `set_sim_lvar` additionally rejects non-finite values.

### 4.5 Sim Channel Synchronisation

**Frontend:** `src/lib/data/aircraftProfiles.ts` and the sim function logic in `+page.svelte`

#### Division of Responsibility

All aircraft knowledge lives in the frontend. The Rust backend holds no aircraft profiles whatsoever and acts purely as transport: the frontend matches the `TITLE` SimVar to a profile, sends a flat list of LVar names via `subscribe_lvars`, and receives `lvar-value-changed` events with a name and a raw value. Adding support for a new aircraft is therefore a data-only change to a single TypeScript file.

Profiles map eight generic categories — COM1, COM2, COM3, HF1, HF2, CAB, PA, INT — to per-aircraft LVars, each with a volume endpoint (name plus native range) and an optional mute endpoint (name plus explicit muted and unmuted values, so either switch polarity can be expressed). The schema carries both `captain` and `firstOfficer` seats; the UI currently binds the captain's panel only (`SIM_SEAT`).

Only **verified** LVars may be added to the registry. A non-existent LVar reads back as a constant `0`, which the mute semantics would misinterpret as a deliberately silenced channel — a silent, confusing failure rather than an obvious one.

| Profile | Title patterns | Notes |
|---------|---------------|-------|
| Fenix A320 | `fenix` | Volume range 0–1; receive latches on all eight categories |
| FlyByWire A380X | `a380x` | Volume range 0–100; receive switches on COM1–3 only |
| iniBuilds A350 | `a350` | Volume range 0–100; channel-indexed LVars, mute on all categories |

Values are converted by `normaliseVolume` / `denormaliseVolume`. Wide ranges (span > 10) are treated as integer knobs and rounded; narrow ranges (0–1) are analogue and keep full precision.

#### Loop Prevention

Two-way synchronisation invites feedback: a write produces an inbound event that looks like a fresh change and triggers another write. Four independent mechanisms prevent this, each covering a case the others do not:

| Mechanism | Guards against |
|-----------|----------------|
| `manuallyControlledSessions` | A pointer gesture being overridden mid-drag. The user's grip always wins. |
| `isSimFunctionLocallyDriven` (500ms) | Echoes arriving during wheel or hardware-axis movement, which have no gesture end. One source owns a sim function at a time. |
| Value + time window (`LVAR_ECHO_EPSILON_RATIO`, `LVAR_ECHO_WINDOW_MS`) | An inbound value equal to what we just wrote. Bounded to 500ms so a genuine knob movement that happens to land on our last value is not suppressed forever. |
| `lvarsSeenNonZero` | The simulator publishing every registered LVar as exactly `0` whilst the aircraft is still loading, which would otherwise drag every bound application to silence. Once an LVar has shown a real value, `0` becomes meaningful. |

Comparisons use an epsilon rather than equality throughout, because values make a round trip through `f32`. Mute state is resolved by proximity — whichever of `mutedValue` / `unmutedValue` the reading is nearer — rather than exact equality, so round-trip noise or a switch that animates through intermediate values is still interpreted correctly instead of being silently discarded.

#### Shared Channels

Several applications may be assigned to the same category. A profile hands out one definition object per category, so two channels share a function exactly when they resolve to the same object — identity is the whole test. When one channel moves, `syncSimFunctionSiblings` mirrors the change onto the others immediately rather than waiting a round trip, whilst the LVar itself is written only once, by the originating channel.

Siblings deliberately do **not** claim manual control, because that flag also suppresses inbound cockpit state and would leave a shared channel deaf to its own LVar. They are routed through the same live-volume throttle instead, which records the write and allows the resulting audio push to be recognised as locally originated.

#### Subscription Orchestration

A `$effect` keeps the backend's subscription set in step with the active profile and assignments, keyed on profile ID plus the sorted LVar list so an unchanged set is never re-sent. The key is recorded only once the backend **accepts** the set: caching it optimistically meant that a single transient failure — the command channel is published a moment after the connection reports ready — left the backend subscribed to nothing with no path back. Failures retry with exponential backoff from 1s to 30s. When the WASM module drops, the key is cleared so a reconnect always re-sends the full set.

### 4.6 State Management

Most reactive application state resides within the `+page.svelte` component as local `$state` rune declarations. The `lib/stores/` directory holds a deliberately small amount of shared state:

| File | Contents |
|------|----------|
| `audioStore.ts` | Utility functions (`formatProcessName`, `isSystemVolume`) and constants (`SYSTEM_VOLUME_ID`, `SYSTEM_VOLUME_PROCESS_NAME`, `SYSTEM_VOLUME_DISPLAY_NAME`). No reactive state. |
| `simStore.svelte.ts` | Reactive `simStatus` (connection, WASM presence, aircraft title, version), fed by the `sim-status-changed` event |
| `mixerMenuStore.svelte.ts` | Reactive single-open-menu coordination, so opening one mixer dropdown collapses any other |
| `themeStore.ts` | Theme state and resolution |

`startSimStatusListener` registers its event listener **before** fetching initial state via `get_sim_status`, so no status change can slip through the gap between the two. The fetch is what surfaces a simulator that was already running when ClearComms launched.

**Reactive State:**

State is categorised into four groups:

| Category | Examples | Persistence |
|----------|----------|-------------|
| **Backend Data** | `audioSessions`, `axisData`, `simStatus` | Ephemeral (pushed via events) |
| **User Configuration** | `axisMappings`, `buttonMappings`, `pinnedApps`, `simAssignments` | localStorage |
| **UI State** | `isEditMode`, `isBindingMode`, `windowPinned`, `dockOpen` | Ephemeral / `ui-state.json` |
| **Internal** | `initStatus`, `audioInitialised`, `previousDisplayCount` | Ephemeral |

**Derived Sim Routing:**

Sim channel synchronisation runs on every inbound LVar event and every animation frame of a gesture, so its lookup structures are built once per change as `$derived` maps rather than being filtered per call — a filter in that position allocates a fresh array on each invocation:

| Derived map | Purpose |
|-------------|---------|
| `activeSimProfile` | Aircraft profile matched from the `TITLE` SimVar (null when unsupported) |
| `simFunctionByProcess` | Process name → function definition for the active profile |
| `sessionsBySimFunction` | Function definition → every running application bound to it |
| `sessionById` | Session ID → session, for O(1) lookup during sync |
| `lvarRouteByName` | LVar name → every channel route it drives |

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

Configuration is persisted through two independent mechanisms.

User bindings are stored in the WebView's `localStorage` under five keys:

```
clearcomms_axis_mappings       → JSON array of AxisMapping objects
clearcomms_button_mappings     → JSON array of ButtonMapping objects
clearcomms_pinned_apps         → JSON array of process name strings (from Set)
clearcomms_app_friendly_names  → JSON map of process name → user-set display name
clearcomms_sim_assignments     → JSON array of SimFunctionAssignment objects
```

Values are loaded on mount and saved after every configuration change. The Tauri WebView maintains a persistent storage profile across application restarts.

Separately, UI state that must survive independently of the WebView's storage profile is written to `ui-state.json` in the Tauri application config directory, via the `save_config_value` / `load_config_value` commands. Reads and writes are serialised behind a mutex on the Rust side, and each call rewrites the whole JSON object, so this is intended for a small number of infrequently changing keys rather than hot state.

**Derived Behaviour ($effect):**

`$effect` blocks handle derived state changes, including:

1. **Theme application** — Applies the resolved light/dark theme when it changes
2. **Onboarding enforcement** — Activates edit mode when no applications are pinned
3. **Pin state synchronisation** — Fetches window pin state when the settings menu or dock opens
4. **Layout measurement** — Triggers frontend dimension measurement when channels render
5. **LVar subscription orchestration** — Keeps the backend subscription set in step with the active aircraft profile and assignments (see §4.5)

### 4.7 Animation Systems

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
- LVar subscriptions bounded at `MAX_LVAR_SUBSCRIPTIONS: 64`, which also bounds the client data definition and request ID ranges
- Reduced stack reservations on the two SimConnect threads (256KB / 512KB against the default reserve)
- `LvarValueEvent` borrows the LVar name from the subscription list rather than cloning it, avoiding an allocation per delivered value
- RAII `Drop` implementations for `AudioManager`, `HidInputManager`, `ProcessHandle`, and `ComApartment`

**Frontend:**

- 30-second memory monitor checks all Map/Set cache sizes against `MAX_CACHE_SIZE` (1,000)
- 5-minute periodic cleanup removes entries for sessions that no longer exist
- Full cleanup on component destroy: `stopPolling()`, `cleanupAllAnimations()`, `cleanupAllLiveVolumeStates()`, `cleanupAllCaches()`
- Dev-mode memory profiler samples `performance.memory` every 60 seconds (up to 120 snapshots), warning on >50% heap growth

### 5.3 Threading Model

| Thread | Responsibility | Wake mechanism | Lifetime |
|--------|---------------|----------------|----------|
| Main (Tauri) | Event loop, window management, IPC dispatch | — | Application |
| `audio-com` | STA COM apartment; processes all audio commands and emits `audio-state-updated` | `recv_timeout` (500ms flag check) | Application |
| `audio-notify` | MTA COM apartment; holds the `IMMNotificationClient` registration | COM callback | Application |
| `input-poll` | Joystick/HID polling; emits `input-axis-data` | 50ms sleep loop | Application |
| `sim-detection` | Toolhelp32 process snapshots for MSFS 2020/2024 | `WaitForSingleObject` (2s) | Application |
| `simconnect-ctrl` | Lifecycle controller; spawns/tears down the connection thread | Blocking `mpsc::recv` | Application |
| `simconnect` | SimConnect dispatch loop, MobiFlight handshake, LVar I/O | `WaitForMultipleObjects` (4 handles) | Per connection |
| `window-anim` | Window resize easing; 500ms cubic ease-out at ~240fps | Channel receive | Lazily spawned, then persistent |
| `theme-monitor` | Windows theme registry polling | 2s interval; `AtomicBool` shutdown | Application |
| `menu-defer` | Native tray/context menu display | One-shot | Per menu |
| Frontend (JS) | Single-threaded event loop; `listen()` for all backend data | Event-driven | Application |

The long-lived threads are spawned during initialisation. Tauri commands are thin wrappers that forward to the owning thread via a channel and block on the reply — no command touches COM, joystick or SimConnect APIs directly, since all three are thread-affine.

Two stack sizes are reduced from the default reserve: `simconnect-ctrl` to 256KB (it only blocks on a receive and spawns) and `simconnect` to 512KB (generous for the dispatch loop and FFI). These are reservations rather than commitments, so the saving is in virtual address space rather than working set.

**Shutdown ordering.** Every thread exits by explicit signal rather than process termination: Win32 events for `sim-detection`, `simconnect` and `audio-notify`; an `AtomicBool` for `theme-monitor`; a `Shutdown` command variant for `audio-com`. Threads that own COM apartments call `CoUninitialize` on the same thread that initialised them, and the audio thread announces completion only after that teardown has genuinely finished. Handles are closed inside the same mutex that guards their use, so a signal can never target a handle that has just been closed and potentially recycled by Windows for an unrelated kernel object.

### 5.4 Latency Budget

| Operation | Interval / Latency | Notes |
|-----------|-------------------|-------|
| Hardware axis polling | 50ms (20 Hz) | Dedicated `input-poll` thread with sleep loop |
| Audio topology flag check | 500ms | `recv_timeout` wake; emits only if the session list changed |
| Audio safety-net enumeration | 10s | Catches external mixer changes COM notifications do not cover |
| Live volume update throttle | 40ms minimum | Prevents IPC saturation during slider drag |
| Sim LVar write throttle | 120ms trailing | Final value of a gesture bypasses the throttle |
| Sim function local-input hold | 500ms | Window during which inbound LVars are ignored after local input |
| LVar echo window | 500ms | Beyond this, a matching value is treated as a genuine cockpit movement |
| Volume animation frame | ~16ms | `requestAnimationFrame` (monitor refresh rate) |
| Window resize frame | ~4.2ms (~240fps) | Singleton Rust thread, oversamples for smooth animation |
| Periodic cache cleanup | 300,000ms (5 min) | Removes stale session entries from all caches |
| Memory monitor | 30,000ms (30s) | Checks cache bounds, enforces MAX_CACHE_SIZE |
| Simulator process detection | 2,000ms | Toolhelp32 snapshot; sleeps in `WaitForSingleObject` |
| Theme detection | 2,000ms | Windows registry poll for `AppsUseLightTheme` |
| SimConnect dispatch wake | Event-driven | 60s backstop timeout whilst the process handle is watched |
| SimConnect reconnect delay | 5s, doubling to 60s | Reset once a connection is established |

Note that the simulator rows describe **ceilings and guard windows, not work performed on a timer**. The SimConnect dispatch loop has no periodic work at all whilst the simulator process handle is available: its 60-second timeout is a backstop against an orphaned event handle, not a poll. Likewise the throttle and hold intervals only apply whilst a value is actually moving.

---

## 6. Security and Reliability

**Capability Model:** The application uses Tauri's capability-based permission system. The `default` capability applies to the `main` window and grants `core:default`, `core:event:default`, and a narrow set of explicit `core:window:*` permissions (show, hide, close, focus, position, size, always-on-top, scale factor) rather than a blanket window permission. All 31 backend commands are exposed through the `invoke_handler` and accessed via Tauri's IPC channel rather than through web-accessible endpoints.

**Input Validation at FFI Boundaries:** Two boundaries accept frontend-supplied data that reaches native APIs, and both validate before crossing:

- `validate_lvar_name` constrains LVar names to 1–128 ASCII alphanumerics and underscores. These names are interpolated into MobiFlight calculator code executed inside the simulator, so anything else is rejected before it can become malformed or injected RPN. `set_sim_lvar` additionally requires a finite value, and `subscribe_lvars` caps the set at `MAX_LVAR_SUBSCRIPTIONS`.
- `validate_message` guards every inbound SimConnect message, checking the pointer is non-null, that the reported `dwSize` agrees with the buffer size supplied by `GetNextDispatch`, and that the buffer is large enough for the struct being cast to. This defends against both struct-layout mismatches in the FFI bindings and a corrupted message stream; payload reads are additionally clamped to the defined buffer size so a message reporting more bytes than were allocated cannot cause an over-read.

**Single Instance:** `tauri-plugin-single-instance` prevents a second copy from starting. A subsequent launch surfaces the existing instance rather than creating a competing one — important given that the application holds exclusive COM registrations and a SimConnect client identity.

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
| Rust COM (audio) | `audio-com` thread calls `CoUninitialize()` on exit | Process lifetime |
| Rust COM (SimConnect) | `ComApartment` Drop guard calls `CoUninitialize()` on the connection thread | Per connection |
| Rust COM (notifications) | `audio-notify` thread unregisters the `IMMNotificationClient` before exit | Process lifetime |
| SimConnect | `SimConnect_Close` plus `CloseHandle` for the event and process handles | Per connection |
| Win32 Events | Closed inside the mutex guarding their use, so no signal can race a close | Per connection |
| Rust Handles | `ProcessHandle::Drop` calls `CloseHandle()` | Per-operation |
| Rust Caches | `cleanup()` with `shrink_to_fit()` | On demand / on drop |
| Frontend Events | `unlisten()` for `input-axis-data`, `audio-state-updated`, `lvar-value-changed`, `sim-status-changed` in `stopPolling()` | `onDestroy` lifecycle |
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

**Logging:**

Logging uses `tracing`, and the subscriber is initialised **only in debug builds** — release builds install none, so instrumentation costs effectively nothing in a shipped binary. Debug output goes to stdout through a `tracing_appender` non-blocking writer: a blocking writer serialises every thread behind a single console write, and a chatty background thread could stall the UI thread for as long as the terminal took to render. The returned guard must outlive the application, since dropping it flushes the buffer and stops the writer thread.

Verbosity is controlled by `RUST_LOG`, defaulting to `warn,ClearComms=debug`. The global `warn` level is deliberate rather than incidental: it keeps dependency diagnostics that matter — `tao`'s event-loop starvation warnings in particular — whilst leaving unmatched targets quiet.

Level discipline is enforced by convention on hot paths:

| Level | Permitted use |
|-------|--------------|
| `INFO` | Genuine state transitions only: connection opened or closed, aircraft changed, WASM presence changed, thread started or exited |
| `DEBUG` | Setup steps, individual message handling, ignored or stale responses |
| `WARN` / `ERROR` | Recoverable failures and poisoned mutexes |

No `INFO`-level statement may fire from inside the dispatch loop or any per-value data path. A log line on a path that runs whenever a knob moves is not merely noise — formatting and writing it is work performed on behalf of the simulator's frame budget.

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
  "width": 700,
  "height": 700,
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
| Identifier | `ClearComms` |
| Publisher | Cameron Carlyon |
| Targets | NSIS installer |
| WebView2 | `embedBootstrapper` — the installer bootstraps the runtime if absent |
| Bundle icons | `icons/white/` |
| Tray icons | Dual-theme: `icons/white/` (dark mode) and `icons/black/` (light mode), selected at runtime |
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

### 9.3 Sim Channel Assignment

**Assignment Flow:**

1. Enter edit mode via the dock menu
2. Open the simulator function button on an application channel
3. Select a category (COM1, COM2, COM3, HF1, HF2, CAB, PA, INT). Selecting the category already assigned clears it
4. The assignment is stored as `{processName, category}` and persisted to `localStorage`

Assignments are keyed by **process name**, not session ID, so they survive the application being closed and reopened, and apply to every audio session that process owns.

The picker always offers all eight categories regardless of the loaded aircraft. Assignment and aircraft support are deliberately independent concerns: a user may configure their mixer without the simulator running, and an assignment to a category the current aircraft lacks simply stays dormant until an aircraft that has it is loaded.

**Availability:** Synchronisation becomes active only when the simulator is connected, the MobiFlight WASM module has answered, and the loaded aircraft matches a profile. When any of those is untrue the assignment remains stored but inert — the application channel behaves as a normal mixer channel.

**Two-Way Behaviour:**

| Action | Result |
|--------|--------|
| Turn the flightdeck volume knob | Application volume follows, animated over 150ms |
| Move the ClearComms slider | Flightdeck knob follows, throttled to 120ms with an immediate final value |
| Move a bound hardware axis | Both the application and the flightdeck knob follow |
| Operate the audio panel receive switch | Application mutes or unmutes |
| Mute the application in ClearComms | Receive switch follows, where the aircraft exposes one |
| Drag the slider to zero | Auto-mute engages; the channel unmutes as soon as it leaves zero |

**Contention:** A pointer gesture always outranks every other source. Whilst a slider is held — including held perfectly still, which produces no events at all — no inbound simulator value is applied to it or to any channel sharing its function. Wheel and hardware-axis input, which have no gesture end, instead own the function for 500ms after each movement.

### 9.4 System Volume Control

A virtual session with the identifier `__SYSTEM__` routes volume and mute operations to the Windows master endpoint volume (`IAudioEndpointVolume`) rather than per-application session volume (`ISimpleAudioVolume`). This is handled transparently — the frontend treats the system volume channel identically to application channels, with routing logic in the `invokeSetVolume` and `invokeSetMute` functions selecting the appropriate backend command based on the session ID.

### 9.5 Window Management

**System Tray:** The application lives in the Windows notification area with a theme-adaptive icon (white for dark mode, black for light mode). Left-click toggles visibility; right-click shows a native Win32 context menu with Show, Hide, Pin on Top, and Quit options.

**Focus Behaviour:** When unpinned, the window automatically hides on focus loss (`WindowEvent::Focused(false)`). A 200ms debounce prevents the tray click handler from immediately reopening a window that was just hidden by the focus loss event triggered by the tray click itself.

**Pin on Top:** The `always_on_top` window property is toggled via both the tray context menu and the frontend settings menu. State is synchronised through the `window-pin-changed` event emitted by the backend.

**Dynamic Resizing:** The window width adjusts to accommodate the number of bound sessions. The frontend measures its own rendered component dimensions and sends them to the backend via `update_layout_measurements`. The resize formula is:

```
width = base_width + (channel_width + channel_gap) × (session_count - 1)
```

Width values are in logical pixels, converted to physical pixels using the display's DPI scale factor.

### 9.6 Visual Design

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

#### `cleanup_input_manager`

```rust
fn cleanup_input_manager() -> Result<String, String>
```

Signals the input polling thread to shut down via its `AtomicBool` flag. The thread performs its own cleanup (clearing caches, releasing memory) before exiting. Returns a status message.

### 10.3 Simulator Commands

#### `get_sim_status`

```rust
fn get_sim_status(state: State<Arc<SimStateHandle>>) -> Result<SimStatusResponse, String>
```

Returns a snapshot of the simulator connection: whether SimConnect is open and dispatching, whether the MobiFlight WASM module has responded, the current aircraft title if known, and the simulator version (`"2020"`, `"2024"`, or `"unknown"`).

Primarily used once at startup to catch state that was established before the frontend registered its listener; thereafter `sim-status-changed` is the live source.

> **Note on state extraction:** `main.rs` manages `Arc<SimStateHandle>`. Tauri stores managed state by `TypeId`, so the extractor must be `State<Arc<SimStateHandle>>` rather than `State<SimStateHandle>` to match what was registered. Rust's auto-deref chain means `state.lock()` still resolves correctly.

#### `subscribe_lvars`

```rust
fn subscribe_lvars(
    lvar_handle: State<Arc<LvarCommandHandle>>,
    names: Vec<String>,
) -> Result<(), String>
```

Replaces the entire LVar subscription set. An empty list unsubscribes everything. Each name is validated (`validate_lvar_name`) and the set is capped at `MAX_LVAR_SUBSCRIPTIONS` (64) before the command is queued to the connection thread.

The backend subscribes each name as `(L:<name>)` and streams changes back as `lvar-value-changed` events.

**Error cases:** more than 64 names, an invalid name, or no active SimConnect connection (`"SimConnect is not connected"`).

#### `set_sim_lvar`

```rust
fn set_sim_lvar(
    lvar_handle: State<Arc<LvarCommandHandle>>,
    name: String,
    value: f64,
) -> Result<(), String>
```

Writes a value to an LVar in the simulator via `MF.SimVars.Set`. The name is validated and the value must be finite.

Both commands enqueue onto an `mpsc` channel and signal a Win32 event to wake the dispatch loop, because every SimConnect API call must happen on the connection thread. The mutex is deliberately held across the `SetEvent`: the connection thread closes the wake event handle only whilst holding the same lock, which makes it impossible to signal a handle that has just been closed — and possibly already recycled by Windows for an unrelated kernel object.

### 10.4 Window Management Commands

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

#### `get_display_info`

```rust
fn get_display_info(app: AppHandle) -> Result<window_utils::DisplayInfo, String>
```

Returns monitor and work-area geometry in physical pixels, along with the DPI scale factor and the maximum permissible window dimensions. Used by the frontend to constrain layout across mixed-DPI setups.

### 10.5 Utility Commands

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

#### `save_config_value` / `load_config_value`

```rust
fn save_config_value(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String>
fn load_config_value(app: AppHandle, key: String) -> Result<Option<serde_json::Value>, String>
```

Read and write individual keys in `ui-state.json` within the Tauri application config directory. Both take a process-wide mutex and rewrite the entire JSON object, so they are intended for infrequently changing UI state rather than hot values.

#### Theme Commands

```rust
fn get_theme_mode_command() -> Result<String, String>
fn set_theme_mode_command(mode: String) -> Result<String, String>
fn get_resolved_theme_name_command() -> Result<String, String>
fn get_theme_state_command() -> Result<(String, String), String>
```

Get and set the theme mode (the user's preference, which may be "system"), and read the resolved concrete theme. `get_theme_state_command` returns both in one call as `(mode, resolved)`, avoiding a second IPC round trip when the frontend needs the pair.

### 10.6 Tauri Events

Backend-to-frontend data is pushed over the event bus rather than pulled by polling. The frontend subscribes with `listen()` and releases every handle in `stopPolling()`.

| Event | Payload | Emitted by | Emitted when |
|-------|---------|-----------|--------------|
| `input-axis-data` | `Vec<AxisData>` | `input-poll` thread | Axis or button values change (~50ms cadence whilst hardware moves) |
| `audio-state-updated` | `Vec<AudioSession>` | `audio-com` thread | The enumerated session list differs from the last emission |
| `lvar-value-changed` | `{ name, value }` | `simconnect` thread | A subscribed LVar's value changes in the simulator |
| `sim-status-changed` | `SimStatusResponse` | `simconnect` thread / lifecycle controller | Connection, WASM presence, aircraft title or version changes |
| `window-pin-changed` | `bool` | Main thread | The always-on-top state is toggled from the tray menu |

`lvar-value-changed` borrows the LVar name from the subscription list rather than owning it. The event is constructed per delivered value, so cloning the string would mean an allocation on the hot path for a name that already exists and outlives the emit.

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
  /** Temporary override for the displayed volume during mute/unmute animations */
  displayVolumeOverride?: number;
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

/** Simulator connection status from the Rust backend */
interface SimStatus {
  connected: boolean;       // SimConnect open and dispatching
  wasmPresent: boolean;     // MobiFlight WASM module answered the ping
  aircraftTitle: string | null;
  simVersion: string;       // "2020" | "2024" | "unknown"
}

/** Generic flightdeck radio channels an application may be assigned to */
type SimFunctionCategory =
  'COM1' | 'COM2' | 'COM3' | 'HF1' | 'HF2' | 'CAB' | 'PA' | 'INT';

/** Which cockpit seat's audio panel an assignment follows */
type SimSeat = 'captain' | 'firstOfficer';

/** Assignment of an application (keyed by process name) to a channel */
interface SimFunctionAssignment {
  processName: string;
  category: SimFunctionCategory;
}

/** Payload of the `lvar-value-changed` event */
interface LvarValueEvent {
  name: string;             // LVar name, without the "L:" prefix
  value: number;            // Raw value in the profile's native range
}
```

**Aircraft Profile Registry (`aircraftProfiles.ts`):**

```typescript
/** A readable/writable volume knob LVar with its native value range */
interface VolumeEndpoint {
  lvar: string;
  min: number;
  max: number;
}

/**
 * A receive/mute switch LVar (the push/pull knob on the audio panel).
 * Values are explicit so either polarity can be expressed.
 */
interface MuteEndpoint {
  lvar: string;
  mutedValue: number;       // Raw value when the receive switch is off
  unmutedValue: number;     // Raw value when the receive switch is on
}

/** The volume and (optional) mute endpoints for one category */
interface SimFunctionDef {
  volume: VolumeEndpoint;
  mute?: MuteEndpoint;
}

/** Categories may be absent if the seat does not support them */
type SeatFunctions = Partial<Record<SimFunctionCategory, SimFunctionDef>>;

interface AircraftProfile {
  id: string;
  name: string;
  /** Case-insensitive regex patterns tested against the TITLE SimVar */
  titlePatterns: string[];
  seats: Record<SimSeat, SeatFunctions>;
}
```

The registry exposes `matchAircraftProfile`, `getFunctionDef`, `getSupportedCategories`, and the `normaliseVolume` / `denormaliseVolume` conversion pair.

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

/// Detected simulator version, from SIMCONNECT_RECV_OPEN or process detection
pub enum SimVersion { Msfs2020, Msfs2024, Unknown }

/// High-level connection state exposed to the frontend
pub enum ConnectionState { Disconnected, Connecting, Connected }

/// MobiFlight WASM module presence
pub enum WasmState { Absent, Present, Checking }

/// Complete simulator state snapshot held in Tauri managed state
pub struct SimState {
    pub connection: ConnectionState,
    pub wasm: WasmState,
    pub aircraft_title: Option<String>,
    pub sim_version: SimVersion,
    pub last_error: Option<String>,
}

/// Thread-safe wrapper for Tauri managed state
pub type SimStateHandle = std::sync::Mutex<SimState>;

/// Commands sent from Tauri commands to the SimConnect connection thread
pub enum LvarCommand {
    /// Replace the entire subscription set; empty unsubscribes all
    Subscribe(Vec<String>),
    /// Write a value via MF.SimVars.Set
    Set { name: String, value: f64 },
}

/// Sender plus the Win32 wake event handle (as isize, for Send + Sync)
pub struct LvarCommandChannel {
    pub sender: std::sync::mpsc::Sender<LvarCommand>,
    pub wake_event: isize,
}

/// Connection-thread state for the ClearComms MobiFlight client
struct LvarClientState {
    register_sent: bool,      // MF.Clients.Add.ClearComms sent, awaiting confirmation
    client_ready: bool,       // ClearComms data areas mapped and usable
    subscribed: Vec<String>,  // Index is the module's order, and so the float offset
    last_values: Vec<f32>,    // Parallel to `subscribed`; seeded with NaN
    pending: Option<Vec<String>>, // Subscription that arrived before the client was ready
}

/// Handles for an active SimConnect thread, so it can be signalled and joined.
/// The event is stored as isize rather than HANDLE so the struct is Send + Sync,
/// as required for Tauri managed state.
pub struct SimConnectSession {
    shutdown_event: isize,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

/// Simulator process lifecycle events from the detection thread
pub enum SimDetectionEvent {
    Started(SimVersion),
    Stopped,
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
| Audio state delivery | Push via `IMMNotificationClient` + change-gated emit | Removes the frontend polling timer entirely; emits only on actual change |
| Notification apartment | MTA on a separate thread | MTA callbacks arrive directly; STA would require a message pump on the audio thread |
| Volume update throttle | 40ms | Prevents flooding Windows audio API during slider drag |
| Window behaviour | Hide on focus loss | Widget-like UX; system tray as primary access point |
| Close behaviour | Hide instead of exit | Persistent background operation; tray icon always available |
| COM threading | Dedicated `audio-com` thread with `COINIT_APARTMENTTHREADED` | All COM calls confined to one thread; eliminates cross-thread access violations |
| Tray menu | Native Win32 (`TrackPopupMenu`) | Consistent with Windows shell UX; avoids web-rendered menus |
| Device identification | Dual API (Joystick + HID) | Joystick API provides data; HID API provides human-readable names |
| Binary optimisation | LTO fat + `opt-level = "z"` | Minimised binary size for distribution |
| Input data delivery | Push via Tauri events | Eliminates cross-thread `invoke()` for axis data; frontend uses `listen()` |
| Simulator detection | Toolhelp32 process snapshots | No COM, no WMI, no elevation; cannot fail unrecoverably |
| SimConnect lifecycle | Spawned on detection events, not at startup | No connection thread exists whilst the simulator is not running |
| SimConnect dispatch | `WaitForMultipleObjects` on 4 handles | Zero CPU whilst idle; simulator exit becomes an event rather than a poll |
| SimConnect retry | Re-inject `Started` into the detection channel | Keeps retry inside the event-driven path rather than adding a loop |
| Retry backoff | 5s doubling to 60s | A repeatable immediate failure would otherwise churn inside the sim process |
| SimConnect bindings | Raw `simconnect-sys` FFI, statically linked | Needs the `hEventHandle` and ClientDataArea calls wrappers abstract away |
| LVar transport | MobiFlight WASM Event Module | Established, widely installed; avoids shipping a bespoke WASM module |
| MobiFlight client | Own named client (`ClearComms`) | `MF.SimVars.Clear` is per-client; coexists with the MobiFlight Connector |
| LVar subscription | `PERIOD_ON_SET` + `FLAG_CHANGED` + one-shot prime | Module rewrites every tick; `CHANGED` moves the diffing into the sim process |
| Aircraft knowledge | Frontend registry only | Adding an aircraft is a data change; backend stays a dumb transport |
| Loop prevention | Value + time window, not a suppression flag | An unbounded flag would permanently deafen a channel to its own LVar |
| Sim seat | Captain's panel only (schema carries both) | Ships the common case; F/O needs no schema change to enable |

---

## 13. Future Considerations

**First Officer Audio Panel:** The aircraft profile schema already carries both `captain` and `firstOfficer` seats, and verified F/O LVars are present for all three shipped profiles. What remains is a user-facing seat selection and threading the choice through `SIM_SEAT`, which is currently a compile-time constant. Note that the Fenix A320's ACP2 publishes volume variables but no receive-switch variables, so mute is unavailable on that seat — the UI would need to reflect per-seat capability rather than assuming symmetry.

**Aircraft Profile Coverage:** The registry currently covers the Fenix A320, FlyByWire A380X and iniBuilds A350. Extending it is a data-only change, gated only on verifying LVars against HubHop — a non-existent LVar reads back as a constant `0` and would be silently misinterpreted as a muted channel rather than failing visibly. A future refinement would be to source profiles from a user-editable file rather than a compiled-in registry, allowing community contributions without a release.

**Multi-Profile Support:** The persistence layer could be extended to support named configuration profiles, allowing different axis/button mappings per aircraft type. The current `localStorage` keys would be namespaced by profile identifier.

**Network Remote Control:** A WebSocket or TCP server could expose the volume control API to external clients, enabling tablet-based or touchscreen-based remote mixing from a secondary device. The existing command architecture would require minimal adaptation.

**Plugin Architecture:** The modular Rust backend (separate modules for audio, hardware input, simulator detection and SimConnect transport) is structured to support dynamic loading of additional input/output adapters. Future input sources (MIDI controllers, network streams) could be implemented as independent modules conforming to a standardised trait interface.

**Cross-Platform Support:** The Rust backend uses `#[cfg(target_os = "windows")]` guards on all platform-specific code, with stub implementations for non-Windows targets. macOS support would require CoreAudio integration in `audio_management` and IOKit/HID integration in `hardware_input`, while the frontend and IPC layer remain platform-independent. The simulator integration is inherently Windows-bound: SimConnect, the MobiFlight WASM module and the Win32 event primitives the dispatch loop is built on have no cross-platform equivalent.

---

*ClearComms — Technical Documentation v1.0.0*
