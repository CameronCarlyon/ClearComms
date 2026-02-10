# ClearComms

Optimised for performance, **ClearComms** is a lightweight companion application for **Microsoft Flight Simulator**, built with **Tauri** and **Svelte**, and powered by **Rust**. The application allows you to control the volume of audio applications (such as vPilot or GSX Pro) using dedicated hardware and flightdeck controls, syncronised through ClearComms.

The goal is to keep you in the flightdeck environment and not fiddling with volume mixer menus, allowing you to stay focused on that final approach and not on the ongoing podcast of a departure clearance on the Tower frequency.

> [!NOTE]
> In order to enable simulator integration, the [MobiFlight Event Module](https://docs.mobiflight.com/guides/wasm-module/) must be installed, though this is not a requirement for the application to function.

## Requirements

- **Operating System:** Windows 10 or later (x64)
- **Microsoft Flight Simulator** (2020 or 2024) — optional, required only for simulator integration
- **MobiFlight WASM Event Module** — optional, required only for simulator LVar access

## Installation

1. Download the latest installer from the [Releases](https://github.com/cameroncarlyon/ClearComms/releases) page.
2. Run `ClearComms_x64-setup.exe` (NSIS) or `ClearComms_x64_en-US.msi` (MSI).
3. Follow the on-screen prompts to complete installation.
4. *(Optional)* Install the [MobiFlight WASM Event Module](https://docs.mobiflight.com/guides/wasm-module/) for simulator integration.

## Usage

1. Launch **ClearComms** — the application runs in the system tray.
2. Click the tray icon to open the mixer panel.
3. Click **+** to add audio applications (e.g. vPilot, GSX Pro) to the mixer.
4. Adjust volume using the on-screen sliders, or map a hardware axis/button:
   - Click the **gamepad icon** on a channel to begin axis binding — move a hardware axis to assign it.
   - Click the **mute button binding icon** to assign a hardware button for mute toggle.
   - Click the **vertical arrow icon** to swap axis travel direction.
   - Click the **red rubbish bin icon** to remove the pinned application.
5. Hover the mouse over the handle at the bottom of the window to open the dock.
   - Click the **edit button** to exit edit mode.
   - Click the **gear cog button** to open the settings menu.
   - Click the **X** to open the close menu.

## Features

- **Hardware-Based Volume Control**  
  Utilise dedicated flight simulator hardware to adjust application volume levels, using axes for volume control and buttons for mute.

- **Microsoft Flight Simulator Integration**  *(In Development)*
  Map flightdeck controls (e.g. VHF1/INT/CAB volume) directly to application audio levels such as **vPilot** or **GSX Pro**.

- **Performance that Flies**  
  Zero drag. Built with Tauri, Svelte, and Rust for negligible performance impact.

- **Intuitive Design in Motion**  
  Marrying a clean user interface with purposeful animations to craft a seamless, intuitive user experience.

- **Stretch Goals**
  - ACARS integration for aircraft without native support.
  - Automated in-flight announcements on PA channel.
  - Custom-built WASM bridge.

## Tech Stack

- **Frontend:** Svelte + TypeScript  
- **Shell:** Tauri 2.x  
- **Backend:** Rust  
- **Integrations:**  
  - Windows Core Audio API  
  - Windows Joystick API + HID API  
  - MobiFlight WASM Event Module *(In Development)* 

### Justifications

The architecture of ClearComms was meticulously researched in order to develop a **lightweight, low-latency, and extensible desktop companion application** for Microsoft Flight Simulator. The chosen technology stack reflects a deliberate emphasis on performance, system-level integration, and a clean user experience.

### Frontend: **Svelte + TypeScript**
Svelte was selected for its minimal runtime overhead and compile-time optimisation model, which produces highly efficient client-side code compared to virtual-DOM–based frameworks. This aligns with ClearComms’ goal of remaining lightweight whilst providing a responsive interface.

TypeScript adds strong static typing and tooling support, improving correctness and maintainability as the application grows, particularly when interfacing with structured data exposed by the Rust backend.

### Shell: **Tauri 2.x**
Tauri provides a modern desktop shell with a significantly smaller footprint than Electron-based alternatives by leveraging the system’s native webview rather than bundling a full Chromium runtime.

Tauri 2.x was chosen for its improved security model, refined IPC system, and first-class Rust integration, allowing ClearComms to expose low-level system functionality whilst keeping the application size and resource usage minimal.

### Backend: **Rust**
Rust was chosen as the core backend language due to its strong guarantees around memory safety, predictable performance, and zero-cost abstractions. These characteristics are critical for an application that interfaces with low-level system APIs and is intended to run continuously alongside a performance-sensitive simulator.

Rust also enables precise control over threading, input polling, and audio manipulation, ensuring deterministic behaviour without the risk of runtime overhead or memory leaks commonly associated with managed runtimes.

### Integrations

#### **Windows Core Audio API (WASAPI)**
The Windows Core Audio API provides direct, low-latency control over per-application audio sessions. Using WASAPI allows ClearComms to manipulate intercom and radio audio at the system mixer level without introducing additional abstraction layers or third-party dependencies.

#### **Windows Joystick API + HID API (Raw Input)**
Windows Raw Input and HID APIs were selected to support a broad range of external hardware, with a focus on flight simulation peripherals and general-purpose devices such as dials and rotary encoders. This approach ensures device-agnostic input handling, high polling rates, and precise axis/button state access without relying on legacy or vendor-specific drivers.

#### **MobiFlight WASM Event Module**
The MobiFlight Event Module is used as an in-sim WASM bridge to access aircraft-specific **LVars and HVars**, enabling reliable interaction with complex third-party aircraft systems. This approach avoids aircraft-specific DLLs or reverse-engineered interfaces while remaining compatible with a wide range of aircraft.

The architecture is intentionally designed to allow this dependency to be substituted with a first-party WASM module in the future, preserving a stable interface while reducing external dependencies over time.

## Summary
This stack balances **performance, portability, and extensibility**, allowing ClearComms to integrate appropriately with both the Windows operating system and Microsoft Flight Simulator whilst maintaining a small footprint and a scalable framework to enable future enhancements.

A comprehensive technical breakdown of the application's architecture is provided [here](docs/DOCUMENTATION.md).

## Licence

This project is licensed under the [MIT Licence](LICENSE.md).
