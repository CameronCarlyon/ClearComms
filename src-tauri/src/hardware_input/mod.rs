use std::sync::Mutex;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[cfg(windows)]
use windows::Win32::Media::Multimedia::{
    joyGetDevCapsW, joyGetPosEx, JOYCAPSW, JOYINFOEX, 
    JOY_USEDEADZONE, JOYERR_NOERROR,
};

/// Axis data from a hardware device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisData {
    pub device_handle: String,
    pub device_name: String,
    pub manufacturer: String,
    pub product_id: u16,
    pub vendor_id: u16,
    pub axes: HashMap<String, f32>, // axis name -> normalized value (0.0-1.0)
}

/// Information about a discovered input device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: u32,
    pub name: String,
    pub num_axes: u32,
    pub num_buttons: u32,
}

impl DeviceInfo {
    /// Convert device info to a human-readable string
    pub fn to_display_string(&self) -> String {
        format!("{} ({} axes, {} buttons)", 
            self.name, self.num_axes, self.num_buttons)
    }
}

/// Manages game controller input using Windows Joystick API
pub struct HidInputManager {
    devices: Vec<DeviceInfo>,
    axis_cache: HashMap<u32, HashMap<String, f32>>,
}

#[cfg(windows)]
impl HidInputManager {
    /// Create a new input manager instance
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            devices: Vec::new(),
            axis_cache: HashMap::new(),
        })
    }

    /// Enumerate all connected game controllers
    pub fn enumerate_devices(&mut self) -> Result<(), String> {
        self.devices.clear();
        
        // Windows supports up to 16 joysticks (JOYSTICKID1 through JOYSTICKID16)
        for joy_id in 0..16u32 {
            unsafe {
                let mut caps: JOYCAPSW = std::mem::zeroed();
                let result = joyGetDevCapsW(
                    joy_id as usize,
                    &mut caps as *mut JOYCAPSW,
                    std::mem::size_of::<JOYCAPSW>() as u32,
                );
                
                if result == JOYERR_NOERROR {
                    // Device exists - copy the name to avoid unaligned reference
                    let name_buf = caps.szPname;
                    let name = String::from_utf16_lossy(&name_buf)
                        .trim_end_matches('\0')
                        .to_string();
                    
                    self.devices.push(DeviceInfo {
                        id: joy_id,
                        name,
                        num_axes: caps.wNumAxes as u32,
                        num_buttons: caps.wNumButtons as u32,
                    });
                }
            }
        }

        eprintln!("[Input] Found {} joystick devices", self.devices.len());
        self.axis_cache.clear();
        
        Ok(())
    }

    /// Get the list of discovered devices
    pub fn get_devices(&self) -> &[DeviceInfo] {
        &self.devices
    }

    /// Read axis values from all devices
    pub fn read_all_axes(&mut self) -> Result<Vec<AxisData>, String> {
        let mut all_axes = Vec::new();
        
        for device in &self.devices {
            unsafe {
                let mut joy_info: JOYINFOEX = std::mem::zeroed();
                joy_info.dwSize = std::mem::size_of::<JOYINFOEX>() as u32;
                joy_info.dwFlags = 0xFFu32 | (JOY_USEDEADZONE as u32); // Request all axes
                
                let result = joyGetPosEx(device.id, &mut joy_info as *mut JOYINFOEX);
                
                if result == JOYERR_NOERROR {
                    let mut axes = HashMap::new();
                    
                    // Windows Joystick API provides raw values (typically 0-65535)
                    // Normalize to 0.0-1.0
                    let max_val = 65535.0;
                    
                    // X axis
                    axes.insert("X".to_string(), (joy_info.dwXpos as f32 / max_val).clamp(0.0, 1.0));
                    
                    // Y axis
                    axes.insert("Y".to_string(), (joy_info.dwYpos as f32 / max_val).clamp(0.0, 1.0));
                    
                    // Z axis (throttle on many devices)
                    axes.insert("Z".to_string(), (joy_info.dwZpos as f32 / max_val).clamp(0.0, 1.0));
                    
                    // R axis (rudder/twist)
                    axes.insert("R".to_string(), (joy_info.dwRpos as f32 / max_val).clamp(0.0, 1.0));
                    
                    // U axis
                    axes.insert("U".to_string(), (joy_info.dwUpos as f32 / max_val).clamp(0.0, 1.0));
                    
                    // V axis
                    axes.insert("V".to_string(), (joy_info.dwVpos as f32 / max_val).clamp(0.0, 1.0));
                    
                    // Cache and add to results
                    self.axis_cache.insert(device.id, axes.clone());
                    
                    all_axes.push(AxisData {
                        device_handle: format!("{}", device.id),
                        device_name: device.name.clone(),
                        manufacturer: String::new(),
                        product_id: 0,
                        vendor_id: 0,
                        axes,
                    });
                } else if let Some(cached) = self.axis_cache.get(&device.id) {
                    // Use cached values if read failed
                    all_axes.push(AxisData {
                        device_handle: format!("{}", device.id),
                        device_name: device.name.clone(),
                        manufacturer: String::new(),
                        product_id: 0,
                        vendor_id: 0,
                        axes: cached.clone(),
                    });
                }
            }
        }
        
        Ok(all_axes)
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
    let mut manager = HidInputManager::new()?;
    manager.enumerate_devices()?;
    
    let device_count = manager.get_devices().len();
    
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

/// Update a test axis value (removed - reading real hardware)
#[tauri::command]
pub fn update_test_axis_value(_device_handle: String, _axis_name: String, _value: f32) -> Result<String, String> {
    Err("Test axis updates are no longer supported. Reading real hardware data now.".to_string())
}