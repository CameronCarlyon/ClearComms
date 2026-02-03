//! LVar Input Module
//!
//! This module provides integration with Microsoft Flight Simulator's
//! local variables (LVars) via a WASM module bridge. It allows
//! ClearComms to read and manipulate cockpit audio panel controls.
//!
//! ## Features
//! - Connect to Flight Simulator via a WASM bridge
//! - Read audio panel LVars (cockpit audio controls)
//! - Subscribe to LVar changes for real-time updates
//! - Map LVars to audio session volumes