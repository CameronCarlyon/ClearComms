# ClearComms

Optimised for performance, **ClearComms** is a lightweight companion application for **Microsoft Flight Simulator**, built with **Tauri** and **Svelte**, and powered by **Rust**. The application allows you to control the volume of audio applications (such as vPilot or GSX Pro) using dedicated hardware and flightdeck controls, syncronised through ClearComms.

The goal is to keep you in the flightdeck environment and not fiddling with volume mixer menus, allowing you to stay focused on that final approach and not on the ongoing podcast of a departure clearance on the Tower frequency.

## Features

- **Hardware-Based Volume Control**  
  Utilise dedicated flight simulator hardware to adjust application volume levels, using axes for volume control and buttons for mute.

- **Microsoft Flight Simulator Integration**  *(In Development)*
  Map flightdeck controls (e.g. VHF1/INT/CAB volume) directly to application audio levels such as **vPilot** or **GSX Pro**.

- **Performance that Flies**  
  Zero drag. Built with Tauri, Svelte, and Rust for negligible memory usage and performance impact.

- **Intuitive Design in Motion**  
  Marrying a clean user interface with purposeful animations to craft a seamless, intuitive user experience.

- **Stretch Goals**
  - ACARS integration for aircraft without native support.
  - Automated in-flight announcements on PA channel.
  - Custom-built WASM bridge.

## Installation

1. Download the latest installer from the [Releases](https://github.com/cameroncarlyon/ClearComms/releases) page.
2. Run `ClearComms-Setup.exe`.
3. Follow the on-screen prompts to complete installation.
4. *(Optional)* Install the [MobiFlight WASM Event Module](https://docs.mobiflight.com/guides/wasm-module/) for simulator integration, if not already installed.

> [!NOTE]
> In order to enable simulator integration, the [MobiFlight Event Module](https://docs.mobiflight.com/guides/wasm-module/) must be installed, though this is not a requirement for the application to function.

> [!NOTE]
> A portable version of ClearComms is available, however for some older Windows 10 installations, the [Microsoft WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2) may need to be installed separately.

## Usage

1. Launch **ClearComms**. The application runs in the system tray and a window will not appear until you click on the tray icon.
2. With the ClearComms window open, you will be first presented with the onboarding view. Click on the **+** button to reveal a list of available audio sessions and add your first audio application (e.g. vPilot, GSX Pro) to the mixer.
3. Once an application has been added, a column of controls will appear in the following order:
  - The **volume slider** represents the application volume level. The user may use the mouse buttons or scrollwheel to make manual adjustments.
  - The **gamepad button** may be used to invoke axis binding mode. Once active, move a hardware axis to assign it to the application. (An axis may be reused for multiple applications.)
  - The **mute button** may be used to invoke mute button binding mode to assign a hardware button for the mute toggle. (A button may be reused for multiple applications.)
  - The **vertical arrow button** is used to swap axis travel direction.
  - The **red litter bin icon** may be used to remove the pinned application completely.
4. Hover the mouse over the handle at the bottom of the window to open the dock. You will find the following buttons:
  - The **settings button** which may be used to open the settings menu. Inside you will find the following options:
    - A link to the user guide (you're reading it!).
    - A link to the GitHub repo.
    - A toggle to pin the application above other windows. Losing window focus will not minimise the application.
  - The **edit button** is used to toggle edit mode. Exiting edit mode will hide the axis bind button, mute bind button, reverse axis button and delete button.
  - Click the **close button** to open the close menu with the following options inside:
    - Return (Exit the close menu).
    - Minimise (Return the application to the system tray).
    - Quit (Completely exit the application).

## Tech Stack

- **Frontend:** Svelte + TypeScript  
- **Shell:** Tauri 2.x  
- **Backend:** Rust  
- **Integrations:**  
  - Windows Core Audio API  
  - Windows Joystick API + HID API  
  - MobiFlight WASM Event Module *(In Development)* 

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

#### **MobiFlight WASM Event Module**
The MobiFlight Event Module is used as an in-sim WASM bridge to access aircraft-specific **LVars and HVars**, enabling reliable interaction with complex third-party aircraft systems. This approach avoids aircraft-specific DLLs or reverse-engineered interfaces while remaining compatible with a wide range of aircraft.

## Summary
This stack balances **performance, portability, and extensibility**, allowing ClearComms to integrate appropriately with both the Windows operating system and Microsoft Flight Simulator whilst maintaining a small footprint and a scalable framework to enable future enhancements.

# Documentation

For a comprehensive technical breakdown of the application's architecture, please refer to the [documentation](DOCUMENTATION.md).

© 2026 [Cameron Carlyon](https://cameroncarlyon.com/) • [MIT Licence](LICENSE.md)
