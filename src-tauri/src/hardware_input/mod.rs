use std::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use hidapi::HidApi;

/// Axis data from a hardware device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisData {
    pub device_handle: String, // Changed to String for better identification
    pub device_name: String,
    pub manufacturer: String,
    pub product_id: u16,
    pub vendor_id: u16,
    pub axes: HashMap<String, f32>, // axis name -> normalized value (0.0-1.0)
}

/// Information about a discovered HID input device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub manufacturer: String,
    pub product_id: u16,
    pub vendor_id: u16,
    pub path: String, // Device path for opening
    #[allow(dead_code)] // Stored for potential future filtering
    pub usage_page: u16,
    #[allow(dead_code)]
    pub usage: u16,
}

impl DeviceInfo {
    /// Convert device info to a human-readable string
    pub fn to_display_string(&self) -> String {
        if !self.manufacturer.is_empty() && !self.name.is_empty() {
            format!("{} {} (VID:{:04X} PID:{:04X})", 
                self.manufacturer, self.name, self.vendor_id, self.product_id)
        } else if !self.name.is_empty() {
            format!("{} (VID:{:04X} PID:{:04X})", 
                self.name, self.vendor_id, self.product_id)
        } else {
            format!("Unknown Device (VID:{:04X} PID:{:04X})", 
                self.vendor_id, self.product_id)
        }
    }
}

/// Manages HID device enumeration and reading
pub struct HidInputManager {
    api: Arc<Mutex<HidApi>>,
    devices: Vec<DeviceInfo>,
    // Cache last known axis values to avoid constant re-reads
    axis_cache: HashMap<String, HashMap<String, f32>>,
    // Track observed maximum values per axis per device to auto-detect bit depth
    axis_maxima: HashMap<String, HashMap<String, u16>>,
    // Track which axes have shown variation (to filter out dead/ghost axes)
    axis_active: HashMap<String, HashMap<String, bool>>,
}

impl HidInputManager {
    /// Create a new HID input manager instance
    pub fn new() -> Result<Self, String> {
        let api = HidApi::new()
            .map_err(|e| format!("Failed to initialise HID API: {}", e))?;
        
        Ok(Self {
            api: Arc::new(Mutex::new(api)),
            devices: Vec::new(),
            axis_cache: HashMap::new(),
            axis_maxima: HashMap::new(),
            axis_active: HashMap::new(),
        })
    }

    /// Enumerate all connected HID devices (filters for game controllers)
    pub fn enumerate_devices(&mut self) -> Result<(), String> {
        let mut api = self.api.lock().map_err(|e| format!("Failed to lock HID API: {}", e))?;
        
        // Refresh the device list
        api.refresh_devices()
            .map_err(|e| format!("Failed to refresh HID devices: {}", e))?;
        
        let device_list = api.device_list();
        let mut discovered_devices = Vec::new();

        // USB HID Usage Pages:
        // 0x01 = Generic Desktop Controls
        // Common usages for game controllers: 0x04 (Joystick), 0x05 (Gamepad), 0x08 (Multi-axis Controller)
        const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
        const USAGE_JOYSTICK: u16 = 0x04;
        const USAGE_GAMEPAD: u16 = 0x05;
        const USAGE_MULTI_AXIS: u16 = 0x08;

        for device in device_list {
            // Filter for game controllers and joysticks
            if device.usage_page() == USAGE_PAGE_GENERIC_DESKTOP {
                let usage = device.usage();
                if usage == USAGE_JOYSTICK || usage == USAGE_GAMEPAD || usage == USAGE_MULTI_AXIS {
                    let name = device.product_string()
                        .unwrap_or("Unknown Device")
                        .to_string();
                    
                    let manufacturer = device.manufacturer_string()
                        .unwrap_or("")
                        .to_string();
                    
                    let path = device.path().to_string_lossy().to_string();
                    
                    discovered_devices.push(DeviceInfo {
                        name,
                        manufacturer,
                        product_id: device.product_id(),
                        vendor_id: device.vendor_id(),
                        path,
                        usage_page: device.usage_page(),
                        usage: device.usage(),
                    });
                }
            }
        }

        eprintln!("[HID] Found {} game controller devices", discovered_devices.len());
        self.devices = discovered_devices;
        
        // Clear caches when devices change
        self.axis_cache.clear();
        self.axis_maxima.clear();
        self.axis_active.clear();
        
        Ok(())
    }

    /// Get the list of discovered devices
    pub fn get_devices(&self) -> &[DeviceInfo] {
        &self.devices
    }

    /// Read axis values from all devices (cached, non-blocking)
    pub fn read_all_axes(&mut self) -> Result<Vec<AxisData>, String> {
        let mut all_axes = Vec::new();
        
        // Clone devices list to avoid borrow issues
        let devices_clone = self.devices.clone();
        
        for device in &devices_clone {
            // Try to read from this device (non-blocking, with timeout)
            match self.read_device_axes_safe(&device.path) {
                Ok(Some(axes)) => {
                    if !axes.is_empty() {
                        // Update cache
                        self.axis_cache.insert(device.path.clone(), axes.clone());
                        
                        all_axes.push(AxisData {
                            device_handle: device.path.clone(),
                            device_name: device.name.clone(),
                            manufacturer: device.manufacturer.clone(),
                            product_id: device.product_id,
                            vendor_id: device.vendor_id,
                            axes,
                        });
                    }
                },
                Ok(None) => {
                    // No new data, use cached values if available
                    if let Some(cached_axes) = self.axis_cache.get(&device.path) {
                        all_axes.push(AxisData {
                            device_handle: device.path.clone(),
                            device_name: device.name.clone(),
                            manufacturer: device.manufacturer.clone(),
                            product_id: device.product_id,
                            vendor_id: device.vendor_id,
                            axes: cached_axes.clone(),
                        });
                    }
                },
                Err(e) => {
                    // Silently skip devices that can't be read
                    eprintln!("[HID] Skipping {}: {}", device.name, e);
                }
            }
        }
        
        Ok(all_axes)
    }

    /// Safely read from a device with error handling and timeouts
    fn read_device_axes_safe(&mut self, device_path: &str) -> Result<Option<HashMap<String, f32>>, String> {
        let api = self.api.lock().map_err(|e| format!("Failed to lock HID API: {}", e))?;
        
        // Try to open the device (this can fail if device is busy)
        let device = match api.open_path(std::ffi::CString::new(device_path).unwrap().as_c_str()) {
            Ok(dev) => dev,
            Err(_) => {
                // Device busy or inaccessible - return cached data instead of error
                return Ok(None);
            }
        };

        // Set non-blocking mode
        device.set_blocking_mode(false)
            .map_err(|e| format!("Failed to set non-blocking mode: {}", e))?;

        // Try to read with minimal timeout for maximum responsiveness
        let mut buf = [0u8; 256];
        let size = match device.read_timeout(&mut buf, 5) { // 5ms timeout - fast response
            Ok(s) => s,
            Err(_) => return Ok(None), // No data available
        };

        if size == 0 {
            return Ok(None);
        }

        // Get or create the maxima tracker for this device
        let device_maxima = self.axis_maxima.entry(device_path.to_string())
            .or_insert_with(HashMap::new);
        
        // Get or create the active tracker for this device
        let device_active = self.axis_active.entry(device_path.to_string())
            .or_insert_with(HashMap::new);

        // Parse the HID report and track maximum values
        let mut axes = HashMap::new();
        let axis_names = ["X", "Y", "Z", "Rx", "Ry", "Rz", "Slider", "Dial"];
        
        for (idx, &axis_name) in axis_names.iter().enumerate() {
            let byte_offset = 1 + (idx * 2); // Skip report ID (byte 0)
            if size >= byte_offset + 2 {
                let raw_value = u16::from_le_bytes([buf[byte_offset], buf[byte_offset + 1]]);
                
                // Track the maximum value seen for this axis
                let max_seen = device_maxima.entry(axis_name.to_string())
                    .or_insert(0);
                
                // Check if this axis has shown variation (not stuck at 0 or a single value)
                let is_active = device_active.entry(axis_name.to_string())
                    .or_insert(false);
                
                // Mark as active if we see the value change significantly
                if raw_value != *max_seen && (raw_value > 100 || *max_seen > 100) {
                    *is_active = true;
                }
                
                *max_seen = (*max_seen).max(raw_value);
                
                // Only include axes that have shown they're active
                if *is_active {
                    // Normalize: use the observed maximum, with a floor of 1023 for safety
                    // This auto-detects 10-bit (1023), 12-bit (4095), or 16-bit (65535) axes
                    let divisor = if *max_seen > 4095 {
                        65535.0 // 16-bit
                    } else if *max_seen > 1023 {
                        4095.0  // 12-bit
                    } else if *max_seen > 255 {
                        1023.0  // 10-bit
                    } else if *max_seen > 0 {
                        255.0   // 8-bit
                    } else {
                        65535.0 // Default until we see data
                    };
                    
                    let normalized = (raw_value as f32 / divisor).clamp(0.0, 1.0);
                    axes.insert(axis_name.to_string(), normalized);
                }
            }
        }

        Ok(Some(axes))
    }
}

// Global HID manager instance
static HID_MANAGER: Mutex<Option<HidInputManager>> = Mutex::new(None);

/// Initialise HID and enumerate devices
#[tauri::command]
pub fn init_direct_input() -> Result<String, String> {
    let mut manager = HidInputManager::new()?;
    manager.enumerate_devices()?;
    
    let device_count = manager.get_devices().len();
    
    let mut hid_lock = HID_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock HID mutex: {}", e))?;
    
    *hid_lock = Some(manager);
    
    Ok(format!("HID initialised successfully ({} game controllers found)", device_count))
}

/// Get the current status of HID
#[tauri::command]
pub fn get_direct_input_status() -> Result<String, String> {
    let hid_lock = HID_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock HID mutex: {}", e))?;
    
    match hid_lock.as_ref() {
        Some(manager) => {
            let device_count = manager.get_devices().len();
            Ok(format!(
                "HID active ({} game controllers discovered)",
                device_count
            ))
        },
        None => Ok("HID not initialised".to_string()),
    }
}

/// Enumerate all connected game controller devices
#[tauri::command]
pub fn enumerate_input_devices() -> Result<Vec<String>, String> {
    let mut hid_lock = HID_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock HID mutex: {}", e))?;
    
    let manager = hid_lock
        .as_mut()
        .ok_or("HID not initialised. Call init_direct_input first.")?;
    
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

/// Get axis values from all game controller devices
#[tauri::command]
pub fn get_all_axis_values() -> Result<Vec<AxisData>, String> {
    let mut hid_lock = HID_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock HID mutex: {}", e))?;
    
    let manager = hid_lock
        .as_mut()
        .ok_or("HID not initialised. Call init_direct_input first.")?;
    
    // Use the new cached read method
    manager.read_all_axes()
}

/// Update a test axis value (removed - now reading real hardware)
#[tauri::command]
pub fn update_test_axis_value(_device_handle: String, _axis_name: String, _value: f32) -> Result<String, String> {
    Err("Test axis updates are no longer supported. Reading real hardware data now.".to_string())
}
