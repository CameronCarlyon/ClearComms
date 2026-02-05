# Copilot Instructions for ClearComms

This document provides guidance for using GitHub Copilot when contributing to the **ClearComms** project.  
The goal is to keep generated code aligned with the project's **purpose, style, and technical stack**.

---

## Project Overview

ClearComms is a lightweight companion application for Microsoft Flight Simulator, built with Tauri and Svelte, and powered by Rust. Optimised for performance, it integrates Flight Simulator LVars, HID input devices, and the Windows Core Audio API to provide synchronised intercom control through flightdeck and hardware inputs.

### Technology Stack
- **Framework**: Tauri 2.x (lightweight desktop app framework)
- **Frontend**: SvelteKit with TypeScript for minimal, performant UI
- **Backend**: Rust
- **Integrations**: 
  - **Flight Simulator WASM** - Read/write simulator LVars via a WASM module bridge
  - **HID Input** - Read axis/knobs from flight sim hardware via Human Interface Device APIs
  - **Windows Core Audio API** - Control application volume at the OS level

The app must be **lightweight, minimal, performant, and visually clean**.

## Project Structure
```
src/
├── lib/
│   ├── components/     # Reusable Svelte UI components
│   │   ├── index.ts              # Barrel export for all components
│   │   ├── ApplicationChannel.svelte  # Complete mixer channel strip
│   │   ├── VolumeSlider.svelte   # Vertical volume control
│   │   ├── MuteButton.svelte     # Mute toggle button
│   │   ├── ChannelButton.svelte  # Unified channel control button (bind, toggle, action)
│   │   ├── ListOption.svelte     # List option item
│   │   ├── ExpandableButton.svelte # Button with expandable menu
│   │   ├── Dock.svelte           # Bottom hover dock
│   │   ├── BootScreen.svelte     # Loading/error state
│   │   └── Footer.svelte         # Attribution footer
│   ├── stores/         # Svelte stores for state management
│   │   ├── index.ts              # Barrel export for stores
│   │   ├── audioStore.ts         # Audio sessions and mappings
│   │   ├── hardwareStore.ts      # Hardware input device state
│   │   └── uiStore.ts            # UI state (edit mode, menus)
│   └── types/          # TypeScript type definitions
│       └── index.ts              # Centralised interfaces
├── routes/
│   └── +page.svelte    # Main application page (orchestrates components)
└── app.html

src-tauri/
├── src/
│   ├── audio_management/      # Windows Core Audio API calls
│   ├── hardware_input/        # HID input handling
│   ├── lvar_input/            # Flight Simulator LVar integration
│   ├── native_menu.rs         # System tray and context menus
│   ├── window_utils.rs        # Window positioning utilities
│   ├── lib.rs
│   └── main.rs
├── icons/                     # Application icons
├── Cargo.toml
└── tauri.conf.json
```

## Development Guidelines

### Rust (Backend)
- Use `snake_case` for variables and functions
- Use `PascalCase` for structs and enums
- Prefer `Result<T, E>` for error handling
- Use `#[tauri::command]` for frontend-exposed functions
- Organise integrations into **separate modules**:
  - `audio_management/` for Windows Core Audio API calls
  - `hardware_input/` for HID input handling
  - `lvar_input/` for Flight Simulator LVar integration
  - `native_menu.rs` for system tray and native menus
  - `window_utils.rs` for window positioning and DPI handling
- Handle errors gracefully, return meaningful messages to the frontend
- Use proper Windows API error handling and COM initialisation where required

### TypeScript/Svelte (Frontend)
- Use `camelCase` for variables and functions
- Use `PascalCase` for components
- Prefer TypeScript interfaces over types
- Use Svelte stores for state management
- Keep Tauri API calls in dedicated service files
- **Component-based architecture**: Break UI into focused, reusable components
- Each component should be <150 lines and single-purpose
- Main `+page.svelte` should primarily compose components, not contain all UI logic
- Minimal, functional UI (avoid clutter)
- Prefer semantic HTML and clean CSS over third-party UI libraries

### Component Architecture Best Practices
- **Extract repeated UI patterns** into reusable components (buttons, toggles, menus, etc.)
- **Props over duplication**: Use props for similar UI elements with different data
- **Events for communication**: Components emit custom events rather than directly calling Tauri commands
- **Slots for flexibility**: Use `<slot>` where components need custom content
- **Keep styling with components**: Each component includes its own `<style>` block
- **Single responsibility**: Each component should do one thing well

## Key Conventions
1. **Simplicity and Performance**: Prefer simple solutions and keep CPU/memory footprint minimal
2. **Component-Based UI**: Break down large files into focused, reusable components
3. **Error Handling**: Always use proper error types and propagation
4. **Audio Management**: Isolate Windows Core Audio API calls in dedicated modules
5. **LVar Access**: Use Flight Simulator WASM bridge for simulator variable access
6. **Input Handling**: Use async patterns for HID input polling
7. **Tauri Commands**: Keep them focused and well-typed
8. **State Management**: Use Svelte stores for reactive UI state (avoid prop drilling)
9. **Comments**: Use comments to explain complex logic and provide context, making sure to explain why and not just what
10. **Language**: Use British English spelling and terminology throughout code comments and documentation

## Security Considerations
- Validate all user inputs in Tauri commands
- Use Tauri's built-in security features and capability system
- Sanitise file paths and external API calls
- Follow principle of least privilege for system access

## Testing
- Write unit tests for core Rust modules
- Use Svelte Testing Library for component tests
- Mock external APIs (Flight Simulator WASM, HID) in tests
- Test audio functionality with proper cleanup
- Test components in isolation

## Performance Notes
- Use async/await for I/O operations
- Implement proper cleanup for audio resources
- Cache LVar connections when possible
- Debounce input events to prevent spam

## Copilot Usage Guidelines

When using Copilot suggestions:
- Accept suggestions that align with project style and minimalism
- Ask Copilot to generate helper functions for repetitive logic (e.g., parsing LVar data)
- Use Copilot for boilerplate Rust FFI bindings with Windows APIs
- Request component-based solutions for UI patterns
- Ensure all generated code uses British English spelling
- Always consult official Tauri documentation before implementing features
- Do not accept overly complex or bloated solutions
- Avoid unnecessary external dependencies unless absolutely needed
- Do not accept "performance optimisations" that sacrifice maintainability without evidence

---

## Mission Statement

ClearComms provides **synchronised intercom volume control** for Microsoft Flight Simulator by linking cockpit audio controls, hardware, and external applications into one seamless system.  
It is designed to be **minimal, performant, and reliable**.

---

## Key Resources

- [Tauri v2 Documentation](https://v2.tauri.app/)
- [Svelte Documentation](https://svelte.dev/docs)
- [SvelteKit Documentation](https://kit.svelte.dev/docs)
- [Rust Book](https://doc.rust-lang.org/book/)
- [Flight Simulator WASM Module Documentation](https://docs.mobiflight.com/guides/wasm-module/)