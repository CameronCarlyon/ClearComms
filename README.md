# ClearComms

**ClearComms** is a lightweight companion application for **Microsoft Flight Simulator**, built with **Tauri** and **Svelte**, and powered by **Rust**. 

Optimised for performance, ClearComms leverages **SimVars**, **DirectInput**, and the **Windows Core Audio API** to provide synchronised **radio-stack volume control** through both flightdeck audio controls and external hardware inputs.

---

## Why ClearComms?

Flight simulators like MSFS already simulate radio stacks, but there’s a disconnect:  
- **Radio-stack controls** don’t actually interact with your ATC client (e.g., vPilot) or ground services (e.g., GSX).  
- **Hardware axes** can be mapped to SimVars, but they don’t affect third-party applications.  
- **App volume mixers** require alt-tabbing and break immersion.  

ClearComms bridges this gap by connecting **in-sim controls**, **hardware devices**, and **third-party applications** into a single, seamless system.

---

## Features

- 🎚 **Real-time radio volume sync**  
  Map cockpit SimVars (e.g., COM1/COM2 volume) directly to application audio levels.

- 🎛 **Hardware axis integration**  
  Use throttle quadrant knobs or dedicated sliders to adjust app volume, while keeping SimVars updated.

- 🔊 **Per-application audio control**  
  Adjust ATC clients like **vPilot** or utility applications such as **GSX Pro** independently, using Windows Core Audio APIs.

- 🪶 **Lightweight by design**  
  Built with Tauri, Svelte, and Rust for minimal CPU and memory impact while MSFS is running.

- 📡 **Stretch Goals** *(planned)*  
- **ACARS integration** for aircraft without native support (such as the A380X)
- **Automated in-flight announcements** on PA channel

---

## Technology Stack

- **Frontend:** [Svelte](https://svelte.dev/) + TypeScript  
- **Shell:** [Tauri](https://tauri.app/) 2.x  
- **Backend:** [Rust](https://www.rust-lang.org/)  
- **Integrations:**  
  - [SimConnect](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_API_Reference.htm) → MSFS SimVars
  - [DirectInput](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/ee418273(v=vs.85)) → Hardware axis/knobs
  - [Windows Core Audio API](https://learn.microsoft.com/en-us/windows/win32/coreaudio/about-the-windows-core-audio-apis) → Application volume

---

## Project Structure

*wip*