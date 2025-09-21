use std::sync::Mutex;
use windows::Win32::UI::Input::{
    RegisterRawInputDevices, GetRawInputDeviceList, GetRawInputDeviceInfoW,
    RAWINPUTDEVICE, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
    RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDEV_DEVNOTIFY,
};
use windows::Win32::Devices::HumanInterfaceDevice::{HID_USAGE_PAGE_GENERIC, HID_USAGE_GENERIC_JOYSTICK};
use windows::Win32::Foundation::HWND;

/// Information about a discovered HID input device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub device_type: String,
    #[allow(dead_code)] // Will be used for axis reading in future implementation
    pub handle: isize,
}

impl DeviceInfo {
    /// Convert device info to a human-readable string
    pub fn to_display_string(&self) -> String {
        format!("{} ({})", self.name, self.device_type)
    }
}

/// Manages Raw Input device registration and monitoring
pub struct RawInputManager {
    devices: Vec<DeviceInfo>,
}

impl RawInputManager {
    /// Create a new Raw Input manager instance
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            devices: Vec::new(),
        })
    }

    /// Register for Raw Input from HID devices (joysticks, throttles, etc.)
    pub fn register_devices(&mut self) -> Result<(), String> {
        unsafe {
            // Register for HID joystick/game controller input
            let devices = [
                RAWINPUTDEVICE {
                    usUsagePage: HID_USAGE_PAGE_GENERIC,
                    usUsage: HID_USAGE_GENERIC_JOYSTICK,
                    dwFlags: RIDEV_DEVNOTIFY,
                    hwndTarget: HWND(std::ptr::null_mut()), // NULL = follow keyboard focus
                },
            ];

            RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32)
                .map_err(|e| format!("Failed to register Raw Input devices: {:?}", e))?;

            eprintln!("[RawInput] Registered for HID joystick input");
            Ok(())
        }
    }

    /// Enumerate all connected Raw Input devices
    pub fn enumerate_devices(&mut self) -> Result<(), String> {
        unsafe {
            // First call to get the number of devices
            let mut device_count: u32 = 0;
            let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
            
            let result = GetRawInputDeviceList(
                None,
                &mut device_count,
                size
            );

            if result == u32::MAX {
                return Err("Failed to get Raw Input device count".into());
            }

            if device_count == 0 {
                eprintln!("[RawInput] No devices found");
                self.devices.clear();
                return Ok(());
            }

            // Allocate buffer and get device list
            let mut device_list = vec![RAWINPUTDEVICELIST::default(); device_count as usize];
            let actual_count = GetRawInputDeviceList(
                Some(device_list.as_mut_ptr()),
                &mut device_count,
                size
            );

            if actual_count == u32::MAX {
                return Err("Failed to enumerate Raw Input devices".into());
            }

            let mut discovered_devices = Vec::new();

            // Query info for each device
            for i in 0..actual_count as usize {
                let device = &device_list[i];
                
                // Get device name
                let mut name_size: u32 = 0;
                let _ = GetRawInputDeviceInfoW(
                    device.hDevice,
                    RIDI_DEVICENAME,
                    None,
                    &mut name_size
                );

                if name_size > 0 {
                    let mut name_buffer: Vec<u16> = vec![0; name_size as usize];
                    let result = GetRawInputDeviceInfoW(
                        device.hDevice,
                        RIDI_DEVICENAME,
                        Some(name_buffer.as_mut_ptr() as *mut _),
                        &mut name_size
                    );

                    if result != u32::MAX {
                        let name = String::from_utf16_lossy(&name_buffer)
                            .trim_end_matches('\0')
                            .to_string();

                        // Get device type info
                        let mut info_size = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
                        let mut device_info = RID_DEVICE_INFO {
                            cbSize: info_size,
                            ..Default::default()
                        };

                        let result = GetRawInputDeviceInfoW(
                            device.hDevice,
                            RIDI_DEVICEINFO,
                            Some(&mut device_info as *mut _ as *mut _),
                            &mut info_size
                        );

                        let device_type = if result != u32::MAX {
                            match device.dwType.0 {
                                0 => "Mouse".to_string(),
                                1 => "Keyboard".to_string(),
                                2 => "HID".to_string(),
                                _ => "Unknown".to_string(),
                            }
                        } else {
                            "Unknown".to_string()
                        };

                        discovered_devices.push(DeviceInfo {
                            name,
                            device_type,
                            handle: device.hDevice.0 as isize,
                        });
                    }
                }
            }

            eprintln!("[RawInput] Found {} devices", discovered_devices.len());
            self.devices = discovered_devices;
            Ok(())
        }
    }

    /// Get the list of discovered devices
    pub fn get_devices(&self) -> &[DeviceInfo] {
        &self.devices
    }
}

// Global Raw Input manager instance
static RAW_INPUT: Mutex<Option<RawInputManager>> = Mutex::new(None);

/// Initialise Raw Input and store the manager globally
#[tauri::command]
pub fn init_direct_input() -> Result<String, String> {
    let mut manager = RawInputManager::new()?;
    manager.register_devices()?;
    
    let mut ri_lock = RAW_INPUT
        .lock()
        .map_err(|e| format!("Failed to lock Raw Input mutex: {}", e))?;
    
    *ri_lock = Some(manager);
    
    Ok("Raw Input initialised successfully".to_string())
}

/// Get the current status of Raw Input
#[tauri::command]
pub fn get_direct_input_status() -> Result<String, String> {
    let ri_lock = RAW_INPUT
        .lock()
        .map_err(|e| format!("Failed to lock Raw Input mutex: {}", e))?;
    
    match ri_lock.as_ref() {
        Some(manager) => {
            let device_count = manager.get_devices().len();
            Ok(format!(
                "Raw Input active ({} devices discovered)",
                device_count
            ))
        },
        None => Ok("Raw Input not initialised".to_string()),
    }
}

/// Enumerate all connected input devices
#[tauri::command]
pub fn enumerate_input_devices() -> Result<Vec<String>, String> {
    let mut ri_lock = RAW_INPUT
        .lock()
        .map_err(|e| format!("Failed to lock Raw Input mutex: {}", e))?;
    
    let manager = ri_lock
        .as_mut()
        .ok_or("Raw Input not initialised. Call init_direct_input first.")?;
    
    // Enumerate devices
    manager.enumerate_devices()?;
    
    // Return device info as strings
    let device_list: Vec<String> = manager
        .get_devices()
        .iter()
        .map(|dev| dev.to_display_string())
        .collect();
    
    Ok(device_list)
}
