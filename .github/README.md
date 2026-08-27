<p align="center">
  <img src="ClearCommsLogoDropShadow.png" width="200" alt="ClearComms Logo" style="margin-bottom: -30px;"/>
  <h3 align="center">ClearComms<br><i>Crafted by Cameron Carlyon</i></h3>
</p>

## What’s this all about?
Optimised for performance, **ClearComms** is a lightweight companion application for **Microsoft Flight Simulator**, built with **Tauri** and **Svelte**, and powered by **Rust**. The application allows you to control the volume of audio applications (such as vPilot or GSX Pro) using dedicated hardware and in-simulator flightdeck controls, synchronised through ClearComms.

The goal is to keep you in the flightdeck environment and not fiddling with volume mixer menus, allowing you to stay focused on that final approach and not on the ongoing podcast of a departure clearance on the Tower frequency.

## Features

- **Hardware-Based Volume Control**  
  Utilise dedicated flight simulator hardware to adjust application volume levels, using axes for volume control and buttons for mute.

- **Microsoft Flight Simulator Integration**  
  Assign an application to a flightdeck radio channel — COM1–3, HF1–2, CAB, PA or INT — and its volume and mute state stay synchronised with the aircraft's audio control panel **in both directions**. Turn the VHF1 knob in the flightdeck and vPilot follows; move the ClearComms slider and the flightdeck knob follows. Pull the receive switch and the application mutes.

- **Performance that Flies**  
  Zero drag. Built with Tauri, Svelte, and Rust for negligible memory usage and performance impact. The simulator integration is entirely event-driven — nothing is polled, so while connected and idle it costs no CPU time that MSFS could otherwise be using.

- **Intuitive Design in Motion**  
  Marrying a clean user interface with purposeful animations to craft a seamless, intuitive user experience.

- **Stretch Goals**
  - First officer audio panel support.
  - ACARS integration for aircraft without native support.
  - Automated in-flight announcements on PA channel.
  - Custom-built WASM bridge.

## Supported Aircraft

Simulator channel assignment requires a profile for the loaded aircraft. ClearComms matches the aircraft automatically and the feature becomes available once a match is found.

| Aircraft | Channels | Mute (receive switch) |
|----------|----------|-----------------------|
| Fenix A320 family | COM1–3, HF1–2, CAB, PA, INT | Yes |
| iniBuilds A350 | COM1–3, HF1–2, CAB, PA, INT | Yes |
| FlyByWire A380X | COM1–3, HF1–2, CAB, PA, INT | COM1–3 only |

Hardware axis and button bindings work with any aircraft — they are independent of simulator integration.

> [!NOTE]
> Simulator channels currently follow the **captain's** audio control panel. Support for the first officer's panel is planned.

## Installation

1. Download the latest installer from the [Releases](https://github.com/cameroncarlyon/ClearComms/releases) page.
2. Run `ClearComms-Setup.exe`.
3. Follow the on-screen prompts to complete installation.
4. *(Optional)* Install the [MobiFlight WASM Event Module](https://docs.mobiflight.com/guides/wasm-module/) for simulator integration, if not already installed.

> [!NOTE]
> In order to enable simulator integration, the [MobiFlight Event Module](https://docs.mobiflight.com/guides/wasm-module/) must be installed, though this is not a requirement for the application to function.

## Usage

<div align="center">
  <img src="ClearCommsShowcase.gif" alt="ClearComms demonstration" width="500" style="max-width: 100%; height: auto;">
</div>

1. Launch **ClearComms**. The application runs in the system tray and a window will not appear until you click on the tray icon.

2. With the ClearComms window open, you will be first presented with the onboarding view. Click on the **+** button to reveal a list of available audio sessions and add your first audio application (e.g. vPilot, GSX Pro) to the mixer.

3. Once an application has been added, a column of controls will appear:
  - The **volume slider** represents the application volume level. The user may use the mouse buttons or scrollwheel to make manual adjustments.
  - The **mute bind button** may be used to invoke mute button binding mode to assign a hardware button for the mute toggle. (A button may be reused for multiple applications.)
  - The **mute button** toggles the application's mute state.
  - The **gamepad button** may be used to invoke axis binding mode. Once active, move a hardware axis to assign it to the application. (An axis may be reused for multiple applications.)
  - The **simulator function button** assigns the application to a flightdeck radio channel (COM1–3, HF1–2, CAB, PA, INT). Selecting the channel already assigned clears it. See [Simulator Channels](#simulator-channels) below.
  - The **vertical arrow button** is used to swap axis travel direction.
  - The **red litter bin icon** may be used to remove the pinned application completely.

4. Hover the mouse over the handle at the bottom of the window to open the dock. You will find the following buttons:
  - The **settings button** which may be used to open the settings menu. Inside you will find the following options:
    - A link to the user guide (you're reading it!).
    - A toggle to pin the application above other windows. Losing window focus will not minimise the application.
    - **Nerd Zone**, which reveals the **SimConnect** and **WASM** status indicators along with additional debugging information.
    - Reboot (Restart the application).
  - The **edit button** is used to toggle edit mode. Exiting edit mode will hide the axis bind button, mute bind button, simulator function button, reverse axis button and delete button.
  - The **close button**, used to open the close menu with the following options inside:
    - Return (Exit the close menu).
    - Quit (Completely exit the application).
    - Minimise (Return the application to the system tray).

## Simulator Channels

Assigning an application to a flightdeck radio channel links the two together in both directions, for as long as the assignment remains in place.

1. Launch Microsoft Flight Simulator (2020 or 2024) and load a [supported aircraft](#supported-aircraft). ClearComms detects the simulator automatically — there is nothing to start or connect, and the two applications may be launched in either order.

2. Open the settings menu and expand **Nerd Zone** to confirm both status lights are green. **SimConnect** green means ClearComms is talking to the simulator; **WASM** green means the MobiFlight Event Module answered. An orange WASM light means the module is not installed.

3. Enter edit mode, then use an application's **simulator function button** to pick a channel. The assignment is remembered between sessions.

4. From that point onward the application volume and the flightdeck knob track one another. Moving either moves the other, and the receive switch on the audio panel mutes and unmutes the application.

Several applications may share one channel — assign both vPilot and your ATC client to COM1 and the single flightdeck knob drives them together. Assignments persist across aircraft changes; the feature simply goes quiet whilst an unsupported aircraft is loaded and resumes when a supported one is loaded again.

## Tech Stack

The following tech stack balances **performance, functionality, and user experience**, allowing ClearComms to integrate appropriately with both the Windows operating system and Microsoft Flight Simulator whilst maintaining a negligible footprint and a scalable framework to enable future enhancements.

- **Frontend:** Svelte + TypeScript  
- **Shell:** Tauri 2.x  
- **Backend:** Rust  
- **Integrations:**  
  - Windows Core Audio API  
  - Windows Joystick API + HID API  
  - SimConnect (Microsoft Flight Simulator 2020 / 2024)  
  - MobiFlight WASM Event Module  

### Justifications

The architecture of ClearComms was meticulously researched in order to develop a **lightweight, low-latency, and extensible desktop companion application** for Microsoft Flight Simulator. The chosen technology stack reflects a deliberate emphasis on performance, system-level integration, and a clean, responsive user experience.

### Frontend: **Svelte + TypeScript**
Svelte was selected for its minimal runtime overhead and compile-time optimisation model, which produces highly efficient client-side code compared to virtual-DOM–based frameworks. This aligns with ClearComms’ goal of remaining lightweight whilst providing a responsive interface.

TypeScript adds strong static typing and tooling support, improving type safety and long-term maintainability, particularly when interfacing with structured data exposed by the Rust backend.

### Shell: **Tauri 2.x**
Tauri provides a modern desktop shell with a significantly smaller footprint than Electron-based alternatives by leveraging the system’s native webview rather than bundling a full Chromium runtime.

Tauri 2.x was chosen for its improved security model, refined IPC system, and first-class Rust integration, allowing ClearComms to expose low-level system functionality whilst keeping the application size and resource usage minimal.

### Backend: **Rust**
Rust was chosen as the core backend language due to its strong guarantees around memory safety, predictable performance, and zero-cost abstractions. These characteristics are critical for an application that interfaces with low-level system APIs and is intended to run continuously alongside a performance-sensitive simulator.

Rust also enables precise control over threading, input polling, and audio manipulation, ensuring deterministic behaviour without the risk of runtime overhead or memory leaks commonly associated with managed runtimes.

### Integrations

#### **Windows Core Audio API (WASAPI)**
The Windows Core Audio API provides direct, low-latency control over per-application audio sessions. Using WASAPI allows ClearComms to manipulate intercom and radio audio at the system mixer level without introducing additional abstraction layers or third-party dependencies.

#### **Windows Joystick API + HID API**
The Windows Joystick API is used to read axis and button values from connected controllers, providing low-latency input polling. The HID API is used to resolve device metadata (manufacturer, product name, VID/PID) so the UI can present accurate device identification for bindings. This combination supports a broad range of flight simulation peripherals and general-purpose devices without relying on legacy or vendor-specific drivers.

#### **SimConnect**
SimConnect is the simulator's official client interface, used here to detect the connection, identify the loaded aircraft, and carry the MobiFlight client data traffic. It is driven entirely through Win32 event handles rather than a polling loop: the connection thread sleeps at the OS level and is woken only when the simulator actually has something to say. Because ClearComms runs alongside one of the most CPU-hungry applications on the system, every cycle it does not spend is a cycle returned to the simulator.

#### **MobiFlight WASM Event Module**
The MobiFlight Event Module is used as an in-sim WASM bridge to access aircraft-specific **LVars and HVars**, enabling reliable interaction with complex third-party aircraft systems. This approach avoids aircraft-specific DLLs or reverse-engineered interfaces while remaining compatible with a wide range of aircraft.

ClearComms registers its own named MobiFlight client, so its subscriptions are isolated from the MobiFlight Connector application and the two may be run side by side without interfering with one another. Aircraft knowledge lives in a curated profile registry in the frontend rather than in the backend, so supporting a new aircraft is a data change rather than a code change.

# Documentation

For a comprehensive technical breakdown of the application's architecture, please refer to the [documentation](DOCUMENTATION.md).

© 2026 [Cameron Carlyon](https://cameroncarlyon.com/) • [MIT Licence](LICENSE.md)
