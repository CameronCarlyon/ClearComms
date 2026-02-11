use std::sync::Mutex;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use hidapi::HidApi;

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

/// Axis and button data from a hardware device
#[derive(Debug, Clone, Serialize, Deserialize)]
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

        // Clear old cache entries to prevent unbounded growth
        self.axis_cache.clear();
        self.button_cache.clear();

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

// Global input manager instance
static INPUT_MANAGER: Mutex<Option<HidInputManager>> = Mutex::new(None);

/// Initialise input system and enumerate devices
#[tauri::command]
pub fn init_direct_input() -> Result<String, String> {
    tracing::info!("[Input] Initialising HID input manager...");
    let mut manager = HidInputManager::new()?;

    tracing::info!("[Input] Enumerating devices...");
    manager.enumerate_devices()?;

    let device_count = manager.get_devices().len();
    tracing::info!("[Input] Found {} joystick device(s)", device_count);

    // Log device details if any found
    if device_count > 0 {
        for device in manager.get_devices() {
            tracing::info!("[Input]   - {}", device.to_display_string());
        }
    }
    
    let mut lock = INPUT_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock input mutex: {}", e))?;
    
    *lock = Some(manager);
    
    Ok(format!("Input initialised successfully ({} controllers found)", device_count))
}

/// Get the current status of input system
#[tauri::command]
pub fn get_direct_input_status() -> Result<String, String> {
    let lock = INPUT_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock input mutex: {}", e))?;
    
    match lock.as_ref() {
        Some(manager) => {
            let device_count = manager.get_devices().len();
            Ok(format!(
                "Input active ({} controllers discovered)",
                device_count
            ))
        },
        None => Ok("Input not initialised".to_string()),
    }
}

/// Enumerate all connected game controllers
#[tauri::command]
pub fn enumerate_input_devices() -> Result<Vec<String>, String> {
    let mut lock = INPUT_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock input mutex: {}", e))?;
    
    let manager = lock
        .as_mut()
        .ok_or("Input not initialised. Call init_direct_input first.")?;
    
    // Re-enumerate devices
    manager.enumerate_devices()?;
    
    // Return device info as human-readable strings
    let device_list: Vec<String> = manager
        .get_devices()
        .iter()
        .map(|dev| dev.to_display_string())
        .collect();
    
    Ok(device_list)
}

/// Get axis values from all game controllers
#[tauri::command]
pub fn get_all_axis_values() -> Result<Vec<AxisData>, String> {
    let mut lock = INPUT_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock input mutex: {}", e))?;
    
    let manager = lock
        .as_mut()
        .ok_or("Input not initialised. Call init_direct_input first.")?;
    
    manager.read_all_axes()
}

/// Clean up input manager resources
#[tauri::command]
pub fn cleanup_input_manager() -> Result<String, String> {
    let mut lock = INPUT_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock input mutex: {}", e))?;
    
    match lock.as_mut() {
        Some(manager) => {
            manager.cleanup();
            Ok("Input manager cleaned up successfully".to_string())
        }
        None => Ok("Input manager not initialised".to_string())
    }
}