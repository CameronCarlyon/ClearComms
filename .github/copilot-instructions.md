# Copilot Instructions for ClearComms

This document provides guidance for using GitHub Copilot when contributing to the **ClearComms** project.  
The goal is to keep generated code aligned with the project's **purpose, style, and technical stack**.

---

## 🎯 Project Overview

ClearComms is a lightweight companion application for Microsoft Flight Simulator, built with Tauri and Svelte, and powered by Rust. Optimised for performance, it leverages SimVars, DirectInput, and the Windows Core Audio API to provide synchronised intercom control through flightdeck and hardware inputs.

### Technology Stack
- **Framework**: Tauri 2.x (lightweight desktop app framework)
- **Frontend**: SvelteKit with TypeScript for minimal, performant UI
- **Backend**: Rust
- **Integrations**: 
  - **SimConnect/SimVars** → Access and manipulate MSFS SimVars
  - **DirectInput** → Read axis/knobs from flight sim hardware
  - **Windows Core Audio API** → Control application volume at the OS level

The app must be **lightweight, minimal, performant, and visually clean**.

## 📦 Project Structure
```
src/
├── lib/
│   ├── components/
│   ├── stores/
│   └── services/
├── routes/
└── app.html

src-tauri/
├── src/
│   ├── audio/
│   ├── input/
│   ├── simvars/
│   └── main.rs
├── Cargo.toml
└── tauri.conf.json
```

## 💻 Development Guidelines

### Rust (Backend)
- Use `snake_case` for variables and functions
- Use `PascalCase` for structs and enums
- Prefer `Result<T, E>` for error handling
- Use `#[tauri::command]` for frontend-exposed functions
- Organise integrations into **separate modules**:
  - `audio/` for Windows Core Audio API calls
  - `input/` for DirectInput handling
  - `simvars/` for SimConnect/SimVars integration
- Handle errors gracefully, return meaningful messages to the frontend

### TypeScript/Svelte (Frontend)
- Use `camelCase` for variables and functions
- Use `PascalCase` for components
- Prefer TypeScript interfaces over types
- Use Svelte stores for state management
- Keep Tauri API calls in dedicated service files
- Minimal, functional UI (avoid clutter)
- Prefer semantic HTML and clean CSS over third-party UI libraries

## Key Conventions
1. **Simplicity & Performance**: Prefer simple solutions and keep CPU/memory footprint minimal
2. **Error Handling**: Always use proper error types and propagation
3. **Audio Management**: Isolate Windows Core Audio API calls in dedicated modules
4. **SimVars**: Cache frequently accessed simulation variables
5. **Input Handling**: Use async patterns for DirectInput polling
6. **Tauri Commands**: Keep them focused and well-typed
7. **State Management**: Use Svelte stores for reactive UI state
8. **Comments**: Use comments to explain complex logic and provide context, making sure to explain why and not just what
9. **Language**: Use British English spelling and terminology throughout code comments and documentation

## Security Considerations
- Validate all user inputs in Tauri commands
- Use Tauri's built-in security features
- Sanitise file paths and external API calls
- Follow principle of least privilege for system access

## Testing
- Write unit tests for core Rust modules
- Use Svelte Testing Library for component tests
- Mock external APIs (SimConnect, DirectInput) in tests
- Test audio functionality with proper cleanup

## Performance Notes
- Use async/await for I/O operations
- Implement proper cleanup for audio resources
- Cache SimVar connections when possible
- Debounce input events to prevent spam

## 🤖 Copilot Usage Guidelines

When using Copilot suggestions:
- ✅ Accept suggestions that align with project style and minimalism
- ✅ Ask Copilot to **generate helper functions** for repetitive logic (e.g., parsing SimConnect data)
- ✅ Use Copilot for **boilerplate Rust FFI bindings** with Windows APIs
- ✅ Ensure all generated code uses British English spelling
- ❌ Do not accept overly complex or bloated solutions
- ❌ Avoid unnecessary external dependencies unless absolutely needed

## 📝 Example Prompts for Copilot

Here are some example prompts you can use to get useful completions:

- *Rust → SimConnect binding:*  
  > "Write a Rust function to subscribe to a SimVar using SimConnect and return its value to Tauri."

- *Rust → Core Audio API:*  
  > "Generate Rust code that adjusts the output volume of a Windows application using the Core Audio API."

- *Rust → DirectInput:*  
  > "Write a Rust function to read values from a hardware axis using DirectInput."

- *Svelte → UI:*  
  > "Create a Svelte component for a volume slider with two-way binding to a store."

---

## ✅ Mission Statement

ClearComms provides **synchronised intercom volume control** for Microsoft Flight Simulator by linking cockpit audio controls, hardware, and external applications into one seamless system.  
It is designed to be **minimal, performant, and reliable**.
