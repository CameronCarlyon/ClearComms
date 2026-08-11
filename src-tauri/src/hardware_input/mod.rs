use std::sync::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use hidapi::HidApi;
use tauri::Emitter;

#[cfg(windows)]
use windows::Win32::Media::Multimedia::{
    joyGetDevCapsW, joyGetPosEx, JOYCAPSW, JOYINFOEX, 
    JOY_USEDEADZONE, JOYERR_NOERROR,
};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of joystick devices Windows supports
const MAX_JOYSTICK_DEVICES: u32 = 16;

/// Maximum axis value from Windows Joystick API (for normalisation)
const MAX_AXIS_VALUE: f32 = 65535.0;

/// Maximum number of buttons per device
const MAX_BUTTONS_PER_DEVICE: u32 = 32;

/// Initial capacity for device and cache collections
const INITIAL_DEVICE_CAPACITY: usize = 16;

/// Initial capacity for HID device map
const INITIAL_HID_DEVICE_CAPACITY: usize = 32;

/// How often to refresh the device list to detect hot-plugged controllers
const DEVICE_REENUMERATION_INTERVAL: Duration = Duration::from_secs(2);

/// Axis and button data from a hardware device
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AxisData {
    pub device_handle: String,
    pub device_name: String,
    pub manufacturer: String,
    pub product_id: u16,
    pub vendor_id: u16,
    pub axes: HashMap<String, f32>, // axis name -> normalised value (0.0-1.0)
    pub buttons: HashMap<String, bool>, // button name -> pressed state
}

/// Information about a discovered input device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: u32,
    pub name: String,
    pub manufacturer: String,
    pub vendor_id: u16,
    pub product_id: u16,
    #[allow(dead_code)]
    pub num_axes: u32,
    pub num_buttons: u32,
}

impl DeviceInfo {
    /// Convert device info to a human-readable string
    pub fn to_display_string(&self) -> String {
        if !self.manufacturer.is_empty() {
            format!("{} {} (VID:{:04X} PID:{:04X})", 
                self.manufacturer, self.name, self.vendor_id, self.product_id)
        } else {
            format!("{} (VID:{:04X} PID:{:04X})", 
                self.name, self.vendor_id, self.product_id)
        }
    }
}

/// Manages game controller input using Windows Joystick API + HID for device names
pub struct HidInputManager {
    devices: Vec<DeviceInfo>,
    axis_cache: HashMap<u32, HashMap<String, f32>>,
    button_cache: HashMap<u32, HashMap<String, bool>>,
    hid_api: HidApi,
    /// Snapshot of connected joystick IDs from the last enumeration, used to
    /// detect hot-plug changes cheaply without a full HID bus scan.
    known_joy_ids: Vec<u32>,
    last_hotplug_check: Option<Instant>,
}

#[cfg(windows)]
impl HidInputManager {
    /// Create a new input manager instance
    pub fn new() -> Result<Self, String> {
        let hid_api = HidApi::new()
            .map_err(|e| format!("Failed to initialise HID API: {}", e))?;
        
        Ok(Self {
            devices: Vec::with_capacity(INITIAL_DEVICE_CAPACITY), // Pre-allocate for typical device count
            axis_cache: HashMap::with_capacity(INITIAL_DEVICE_CAPACITY),
            button_cache: HashMap::with_capacity(INITIAL_DEVICE_CAPACITY),
            hid_api,
            known_joy_ids: Vec::with_capacity(INITIAL_DEVICE_CAPACITY),
            last_hotplug_check: None,
        })
    }
    
    /// Clean up resources and caches
    pub fn cleanup(&mut self) {
        tracing::info!("[Input] Cleaning up HID input manager resources...");
        
        // Clear all caches
        self.devices.clear();
        self.axis_cache.clear();
        self.button_cache.clear();
        
        // Release allocated memory back to the system
        self.devices.shrink_to_fit();
        self.axis_cache.shrink_to_fit();
        self.button_cache.shrink_to_fit();
        
        tracing::info!("[Input] HID input manager cleanup complete");
    }

    /// Enumerate all connected game controllers with improved memory management
    pub fn enumerate_devices(&mut self) -> Result<(), String> {
        self.devices.clear();
        
        // Refresh HID device list
        self.hid_api.refresh_devices()
            .map_err(|e| format!("Failed to refresh HID devices: {}", e))?;
        
        // Build a map of joystick devices from HID (for names)
        let mut hid_devices: HashMap<(u16, u16), (String, String)> = HashMap::with_capacity(INITIAL_HID_DEVICE_CAPACITY);
        for device in self.hid_api.device_list() {
            // Filter for game controllers (Usage Page 0x01, Usage 0x04/0x05/0x08)
            if device.usage_page() == 0x01 {
                let usage = device.usage();
                if usage == 0x04 || usage == 0x05 || usage == 0x08 {
                    let name = device.product_string().unwrap_or("Unknown Device").to_string();
                    let manufacturer = device.manufacturer_string().unwrap_or("").to_string();
                    let vid = device.vendor_id();
                    let pid = device.product_id();
                    hid_devices.insert((vid, pid), (name, manufacturer));
                }
            }
        }
        
        // Windows supports up to MAX_JOYSTICK_DEVICES joysticks (JOYSTICKID1 through JOYSTICKID16)
        for joy_id in 0..MAX_JOYSTICK_DEVICES {
            unsafe {
                let mut caps: JOYCAPSW = std::mem::zeroed();
                let result = joyGetDevCapsW(
                    joy_id as usize,
                    &mut caps as *mut JOYCAPSW,
                    std::mem::size_of::<JOYCAPSW>() as u32,
                );
                
                if result == JOYERR_NOERROR {
                    // Get VID/PID from capabilities
                    let vendor_id = caps.wMid;
                    let product_id = caps.wPid;
                    
                    // Try to get real device name from HID
                    let (name, manufacturer) = hid_devices
                        .get(&(vendor_id, product_id))
                        .cloned()
                        .unwrap_or_else(|| {
                            // Fallback to caps name if not found in HID
                            let name_buf = caps.szPname;
                            let fallback_name = String::from_utf16_lossy(&name_buf)
                                .trim_end_matches('\0')
                                .to_string();
                            (fallback_name, String::new())
                        });
                    
                    self.devices.push(DeviceInfo {
                        id: joy_id,
                        name,
                        manufacturer,
                        vendor_id,
                        product_id,
                        num_axes: caps.wNumAxes as u32,
                        num_buttons: caps.wNumButtons as u32,
                    });
                }
            }
        }

        // Clear stale cache entries for devices that are no longer present
        let active_ids: std::collections::HashSet<u32> = self.devices.iter().map(|d| d.id).collect();
        self.axis_cache.retain(|id, _| active_ids.contains(id));
        self.button_cache.retain(|id, _| active_ids.contains(id));

        // Update the known-device snapshot for future lightweight hot-plug checks
        self.known_joy_ids = self.devices.iter().map(|d| d.id).collect();
        self.last_hotplug_check = Some(Instant::now());

        Ok(())
    }

    /// Lightweight check for device changes without a full HID bus scan.
    /// Only performs the 16 cheap joyGetDevCapsW calls to detect whether the
    /// set of connected joystick IDs has changed. Triggers a full
    /// re-enumeration (including HID name resolution) only when it has.
    fn maybe_refresh_devices_for_hotplug(&mut self) -> Result<(), String> {
        let should_check = self
            .last_hotplug_check
            .map(|last| last.elapsed() >= DEVICE_REENUMERATION_INTERVAL)
            .unwrap_or(true);

        if !should_check {
            return Ok(());
        }

        self.last_hotplug_check = Some(Instant::now());

        // Collect the set of currently-present joystick IDs (very cheap — 16 Win32 calls)
        let mut current_ids: Vec<u32> = Vec::with_capacity(MAX_JOYSTICK_DEVICES as usize);
        for joy_id in 0..MAX_JOYSTICK_DEVICES {
            unsafe {
                let mut caps: JOYCAPSW = std::mem::zeroed();
                let result = joyGetDevCapsW(
                    joy_id as usize,
                    &mut caps as *mut JOYCAPSW,
                    std::mem::size_of::<JOYCAPSW>() as u32,
                );
                if result == JOYERR_NOERROR {
                    current_ids.push(joy_id);
                }
            }
        }

        // Only run the expensive full enumeration when the device set has changed
        if current_ids != self.known_joy_ids {
            tracing::info!(
                "[Input] Device set changed (was {:?}, now {:?}) — re-enumerating",
                self.known_joy_ids,
                current_ids
            );
            self.enumerate_devices()?;
        }

        Ok(())
    }

    /// Get the list of discovered devices
    pub fn get_devices(&self) -> &[DeviceInfo] {
        &self.devices
    }

    /// Read axis values from all devices with memory management
    pub fn read_all_axes(&mut self) -> Result<Vec<AxisData>, String> {
        let mut all_axes = Vec::with_capacity(self.devices.len());
        
        for device in &self.devices {
            unsafe {
                let mut joy_info: JOYINFOEX = std::mem::zeroed();
                joy_info.dwSize = std::mem::size_of::<JOYINFOEX>() as u32;
                joy_info.dwFlags = 0xFFu32 | (JOY_USEDEADZONE as u32); // Request all axes
                
                let result = joyGetPosEx(device.id, &mut joy_info as *mut JOYINFOEX);
                
                if result == JOYERR_NOERROR {
                    let mut axes = HashMap::new();
                    let mut buttons = HashMap::new();
                    
                    // Windows Joystick API provides raw values (typically 0-65535)
                    // Normalise to 0.0-1.0
                    
                    // X axis
                    axes.insert("X".to_string(), (joy_info.dwXpos as f32 / MAX_AXIS_VALUE).clamp(0.0, 1.0));
                    
                    // Y axis
                    axes.insert("Y".to_string(), (joy_info.dwYpos as f32 / MAX_AXIS_VALUE).clamp(0.0, 1.0));
                    
                    // Z axis (throttle on many devices)
                    axes.insert("Z".to_string(), (joy_info.dwZpos as f32 / MAX_AXIS_VALUE).clamp(0.0, 1.0));
                    
                    // R axis (rudder/twist)
                    axes.insert("R".to_string(), (joy_info.dwRpos as f32 / MAX_AXIS_VALUE).clamp(0.0, 1.0));
                    
                    // U axis
                    axes.insert("U".to_string(), (joy_info.dwUpos as f32 / MAX_AXIS_VALUE).clamp(0.0, 1.0));
                    
                    // V axis
                    axes.insert("V".to_string(), (joy_info.dwVpos as f32 / MAX_AXIS_VALUE).clamp(0.0, 1.0));
                    
                    // Read button states (up to MAX_BUTTONS_PER_DEVICE buttons)
                    let button_mask = joy_info.dwButtons;
                    for btn_num in 0..MAX_BUTTONS_PER_DEVICE {
                        let is_pressed = (button_mask & (1 << btn_num)) != 0;
                        if is_pressed || btn_num < device.num_buttons {
                            // Only include buttons that exist or are currently pressed
                            buttons.insert(format!("Button{}", btn_num + 1), is_pressed);
                        }
                    }
                    
                    // POV Hat switch (returns angle in hundredths of degrees, 0-35900, or 0xFFFF for centered)
                    if joy_info.dwPOV != 0xFFFF {
                        let pov_angle = joy_info.dwPOV as f32 / 100.0; // Convert to degrees
                        axes.insert("POV".to_string(), pov_angle / 360.0); // Normalize to 0.0-1.0
                        
                        // Also provide discrete POV directions as buttons for convenience
                        buttons.insert("POV_Up".to_string(), pov_angle >= 315.0 || pov_angle <= 45.0);
                        buttons.insert("POV_Right".to_string(), (45.0..=135.0).contains(&pov_angle));
                        buttons.insert("POV_Down".to_string(), (135.0..=225.0).contains(&pov_angle));
                        buttons.insert("POV_Left".to_string(), (225.0..=315.0).contains(&pov_angle));
                    } else {
                        buttons.insert("POV_Centered".to_string(), true);
                    }
                    
                    // Cache and add to results
                    self.axis_cache.insert(device.id, axes.clone());
                    self.button_cache.insert(device.id, buttons.clone());
                    
                    all_axes.push(AxisData {
                        device_handle: device.id.to_string(),
                        device_name: device.name.clone(),
                        manufacturer: device.manufacturer.clone(),
                        product_id: device.product_id,
                        vendor_id: device.vendor_id,
                        axes,
                        buttons,
                    });
                } else if let Some(cached_axes) = self.axis_cache.get(&device.id) {
                    // Use cached values if read failed
                    let cached_buttons = self.button_cache.get(&device.id).cloned().unwrap_or_default();
                    all_axes.push(AxisData {
                        device_handle: device.id.to_string(),
                        device_name: device.name.clone(),
                        manufacturer: device.manufacturer.clone(),
                        product_id: device.product_id,
                        vendor_id: device.vendor_id,
                        axes: cached_axes.clone(),
                        buttons: cached_buttons,
                    });
                }
            }
        }
        
        Ok(all_axes)
    }
}

#[cfg(windows)]
impl Drop for HidInputManager {
    fn drop(&mut self) {
        tracing::debug!("[Input] Dropping HID input manager...");
        self.cleanup();
        tracing::debug!("[Input] HID input manager dropped");
    }
}

#[cfg(not(windows))]
impl HidInputManager {
    pub fn new() -> Result<Self, String> {
        Err("Input manager only supported on Windows".to_string())
    }
    
    pub fn enumerate_devices(&mut self) -> Result<(), String> {
        Err("Input manager only supported on Windows".to_string())
    }
    
    pub fn get_devices(&self) -> &[DeviceInfo] {
        &[]
    }
    
    pub fn read_all_axes(&mut self) -> Result<Vec<AxisData>, String> {
        Err("Input manager only supported on Windows".to_string())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dedicated Input Polling Thread
// ─────────────────────────────────────────────────────────────────────────────

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc;

/// Interval between input polling iterations
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Longest the polling thread sleeps before re-checking the shutdown flag.
/// Keeps quit responsive even during the idle (no devices) 1-second interval.
const SHUTDOWN_CHECK_SLICE: Duration = Duration::from_millis(50);

/// Maximum time shutdown waits for the polling thread to exit before giving up
/// and letting the process continue without joining it.
const THREAD_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Handles for the dedicated input polling thread.
struct InputThread {
    shutdown: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<()>>,
    /// Signalled by the polling thread immediately before it returns.
    done_rx: mpsc::Receiver<()>,
}

/// Global handle to the input polling thread.
static INPUT_THREAD: Mutex<Option<InputThread>> = Mutex::new(None);

/// Initialise input system, enumerate devices, and start the dedicated polling thread.
/// The polling thread emits "input-axis-data" Tauri events at ~50ms intervals.
#[tauri::command]
pub fn init_input(app: tauri::AppHandle) -> Result<String, String> {
    // Stop any thread from a previous init (e.g. a dev-mode reload) before
    // starting a new one. Without this the old thread is orphaned: its handle is
    // replaced, so nothing can ever signal or join it, and it keeps emitting
    // duplicate events until the process dies.
    let already_running = INPUT_THREAD
        .lock()
        .map(|lock| lock.is_some())
        .unwrap_or(false);
    if already_running {
        tracing::info!("[Input] Polling thread already running — stopping it before re-init");
        shutdown_input_thread();
    }

    tracing::info!("[Input] Initialising HID input manager...");
    let mut manager = HidInputManager::new()?;

    tracing::info!("[Input] Enumerating devices...");
    manager.enumerate_devices()?;

    let device_count = manager.get_devices().len();
    tracing::info!("[Input] Found {} joystick device(s)", device_count);

    if device_count > 0 {
        for device in manager.get_devices() {
            tracing::info!("[Input]   - {}", device.to_display_string());
        }
    }

    // Create shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let (done_tx, done_rx) = mpsc::channel::<()>();

    // Spawn the dedicated polling thread — all joystick/HID calls happen here
    let shutdown_flag = shutdown.clone();
    let join_handle = std::thread::Builder::new()
        .name("input-poll".to_string())
        .spawn(move || {
            tracing::info!("[Input] Dedicated input polling thread running");

            // Last axis/button values emitted to the frontend. Only emit when data has
            // changed to avoid flooding the JS event queue when no controls are moving.
            let mut last_emitted_data: Vec<AxisData> = Vec::new();

            loop {
                if shutdown_flag.load(Ordering::Relaxed) {
                    break;
                }

                // Check for hot-plugged devices
                #[cfg(windows)]
                if let Err(error) = manager.maybe_refresh_devices_for_hotplug() {
                    tracing::warn!("[Input] Device hot-plug refresh failed: {}", error);
                }

                // Read all axis/button values
                match manager.read_all_axes() {
                    Ok(data) => {
                        // Only emit when values have actually changed — avoids sending a
                        // Tauri event (and triggering JS processing) on every tick when
                        // the user is not interacting with any hardware control.
                        if data != last_emitted_data {
                            if let Err(e) = app.emit("input-axis-data", &data) {
                                tracing::warn!("[Input] Failed to emit axis data: {}", e);
                            }
                            last_emitted_data = data;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[Input] read_all_axes failed: {}", e);
                    }
                }

                // Sleep for longer when no devices are connected to reduce idle wakeups.
                // The hot-plug check interval (DEVICE_REENUMERATION_INTERVAL = 2 s) still
                // governs how quickly we detect newly connected hardware.
                let sleep_duration = if manager.get_devices().is_empty() {
                    Duration::from_secs(1)
                } else {
                    POLL_INTERVAL
                };

                // Sleep in short slices rather than one long block, so a quit
                // request is noticed promptly. The process must not call
                // ExitProcess while this thread is suspended inside a winmm or
                // HID call.
                let mut remaining = sleep_duration;
                while !remaining.is_zero() {
                    if shutdown_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let slice = remaining.min(SHUTDOWN_CHECK_SLICE);
                    std::thread::sleep(slice);
                    remaining -= slice;
                }
            }

            manager.cleanup();
            tracing::info!("[Input] Dedicated input polling thread exited");
            let _ = done_tx.send(());
        })
        .map_err(|e| format!("Failed to spawn input polling thread: {}", e))?;

    {
        let mut lock = INPUT_THREAD
            .lock()
            .map_err(|e| format!("Failed to lock input thread handle: {}", e))?;
        *lock = Some(InputThread {
            shutdown,
            join_handle: Some(join_handle),
            done_rx,
        });
    }

    Ok(format!("Input initialised successfully ({} controllers found)", device_count))
}

/// Get the current status of input system
#[tauri::command]
pub fn get_input_status() -> Result<String, String> {
    let lock = INPUT_THREAD
        .lock()
        .map_err(|e| format!("Failed to lock input thread handle: {}", e))?;

    match lock.as_ref() {
        Some(thread) => {
            if thread.shutdown.load(Ordering::Relaxed) {
                Ok("Input shut down".to_string())
            } else {
                Ok("Input active (polling thread running)".to_string())
            }
        }
        None => Ok("Input not initialised".to_string()),
    }
}

/// Clean up input manager resources and stop the polling thread
#[tauri::command]
pub fn cleanup_input_manager() -> Result<String, String> {
    shutdown_input_thread();
    Ok("Input manager cleaned up successfully".to_string())
}

/// Signal the input polling thread to shut down and wait for it to exit.
///
/// Called during app shutdown. The join matters because `app.exit(0)` calls
/// `ExitProcess`, which suspends every other thread at an arbitrary point —
/// including inside a winmm or hidapi call. The wait is bounded so a wedged
/// device driver can never hang the quit sequence.
pub fn shutdown_input_thread() {
    let Ok(mut lock) = INPUT_THREAD.lock() else {
        return;
    };
    let Some(mut thread) = lock.take() else {
        return;
    };

    thread.shutdown.store(true, Ordering::Relaxed);
    tracing::info!("[Input] Shutdown signal sent to input polling thread");

    match thread.done_rx.recv_timeout(THREAD_SHUTDOWN_TIMEOUT) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            if let Some(handle) = thread.join_handle.take() {
                if let Err(e) = handle.join() {
                    tracing::error!("[Input] Failed to join input polling thread: {:?}", e);
                }
            }
            tracing::info!("[Input] Input polling thread joined");
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            tracing::warn!(
                "[Input] Input polling thread did not exit within {:?} — not joining",
                THREAD_SHUTDOWN_TIMEOUT
            );
        }
    }
}
