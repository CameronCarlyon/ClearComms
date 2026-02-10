use std::sync::Mutex;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};

#[cfg(windows)]
use windows::{
    core::*,
    Win32::System::Com::*,
    Win32::Media::Audio::*,
    Win32::Media::Audio::Endpoints::*,
    Win32::Foundation::*,
    Win32::System::Threading::*,
};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum path length for Windows process names (MAX_PATH)
const MAX_PATH_LENGTH: usize = 260;

/// Maximum number of cached audio sessions before pruning
const MAX_SESSION_CACHE_SIZE: usize = 1000;

/// Initial capacity for session vectors (reasonable estimate for typical systems)
const INITIAL_SESSION_CAPACITY: usize = 64;

/// Interval for logging enumerate calls (every N calls)
const LOG_INTERVAL: usize = 200;

/// Information about an audio session (application)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSession {
    pub session_id: String,
    pub display_name: String,
    pub process_id: u32,
    pub process_name: String, // e.g., "Discord.exe"
    pub volume: f32, // 0.0 to 1.0
    pub is_muted: bool,
}

/// Manages Windows Core Audio API for application volume control
pub struct AudioManager {
    sessions: HashMap<String, AudioSession>,
    current_device_id: String,
    enumerate_calls: usize,
    last_logged_counts: Option<(usize, usize)>,
}

#[cfg(windows)]
/// RAII wrapper for process handles to ensure proper cleanup
struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(process_id: u32) -> std::result::Result<Self, String> {
        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                process_id,
            ).map_err(|e| format!("Failed to open process {}: {}", process_id, e))?;
            Ok(ProcessHandle(handle))
        }
    }
    
    fn as_handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
/// Get the executable name from a process ID with proper resource cleanup
fn get_process_name(process_id: u32) -> String {
    if process_id == 0 {
        return "System".to_string();
    }

    if let Ok(process_handle) = ProcessHandle::open(process_id) {
        unsafe {
            // Buffer for the executable path
            let mut buffer = vec![0u16; MAX_PATH_LENGTH];
            let mut size = buffer.len() as u32;

            // Get the full executable path
            let result = QueryFullProcessImageNameW(
                process_handle.as_handle(),
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );

            if result.is_ok() && size > 0 {
                // Convert to String
                let path = String::from_utf16_lossy(&buffer[0..size as usize]);

                // Extract just the filename from the full path
                if let Some(filename) = path.split('\\').next_back() {
                    return filename.to_string();
                }

                return path;
            }
            // ProcessHandle automatically closes on drop
        }
    }

    // Fallback if we can't get the process name
    format!("Process {}", process_id)
}

#[cfg(windows)]
impl AudioManager {
    /// Create a new audio manager instance
    pub fn new() -> std::result::Result<Self, String> {
        eprintln!("[Audio] Initialising COM library...");
        // Initialize COM for this thread
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e: Error| format!("Failed to initialize COM: {}", e))?;
        }
        
        eprintln!("[Audio] Detecting default audio device...");
        // Get initial default device ID
        let device_id = Self::get_default_device_id()?;
        eprintln!("[Audio] Default device: {}", device_id);
        
        Ok(Self {
            sessions: HashMap::new(),
            current_device_id: device_id,
            enumerate_calls: 0,
            last_logged_counts: None,
        })
    }
    
    /// Get the current default audio device ID
    fn get_default_device_id() -> std::result::Result<String, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e: Error| format!("Failed to get default audio endpoint: {}", e))?;

            let id = device.GetId()
                .map_err(|e: Error| format!("Failed to get device ID: {}", e))?;

            let id_string = id.to_string()
                .map_err(|e| format!("Failed to convert device ID: {}", e));

            // Free COM-allocated PWSTR to prevent memory leak
            // Win32 docs: "the caller is responsible for freeing the memory"
            CoTaskMemFree(Some(id.0 as *const core::ffi::c_void));

            id_string
        }
    }
    
    /// Check if default device has changed, return true if changed
    pub fn check_device_changed(&mut self) -> std::result::Result<bool, String> {
        let new_device_id = Self::get_default_device_id()?;
        
        if new_device_id != self.current_device_id {
            eprintln!("[Audio] Default device changed: {} -> {}", self.current_device_id, new_device_id);
            self.current_device_id = new_device_id;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Get the system audio endpoint volume interface
    fn get_endpoint_volume() -> std::result::Result<IAudioEndpointVolume, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e: Error| format!("Failed to get default audio endpoint: {}", e))?;

            device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e: Error| format!("Failed to activate endpoint volume: {}", e))
        }
    }

    /// Get the system (device endpoint) master volume level (0.0 to 1.0)
    pub fn get_system_volume(&self) -> std::result::Result<f32, String> {
        unsafe {
            Self::get_endpoint_volume()?
                .GetMasterVolumeLevelScalar()
                .map_err(|e: Error| format!("Failed to get master volume: {}", e))
        }
    }

    /// Get the system (device endpoint) mute state
    pub fn get_system_mute(&self) -> std::result::Result<bool, String> {
        unsafe {
            Ok(Self::get_endpoint_volume()?
                .GetMute()
                .map_err(|e: Error| format!("Failed to get mute state: {}", e))?
                .as_bool())
        }
    }

    /// Set the system (device endpoint) master volume level (0.0 to 1.0)
    pub fn set_system_volume(&self, volume: f32) -> std::result::Result<(), String> {
        let volume = volume.clamp(0.0, 1.0);
        unsafe {
            Self::get_endpoint_volume()?
                .SetMasterVolumeLevelScalar(volume, std::ptr::null())
                .map_err(|e: Error| format!("Failed to set master volume: {}", e))
        }
    }

    /// Set the system (device endpoint) mute state
    pub fn set_system_mute(&self, muted: bool) -> std::result::Result<(), String> {
        unsafe {
            Self::get_endpoint_volume()?
                .SetMute(BOOL(muted as i32), std::ptr::null())
                .map_err(|e: Error| format!("Failed to set mute state: {}", e))
        }
    }

    /// Enumerate all active audio sessions from all audio devices with proper resource management
    pub fn enumerate_sessions(&mut self) -> std::result::Result<Vec<AudioSession>, String> {
        unsafe {
            // Create device enumerator
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            // Get all audio render devices
            let device_collection = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e: Error| format!("Failed to enumerate audio endpoints: {}", e))?;

            let device_count = device_collection
                .GetCount()
                .map_err(|e: Error| format!("Failed to get device count: {}", e))?;

            let mut sessions = Vec::with_capacity(INITIAL_SESSION_CAPACITY); // Pre-allocate reasonable capacity
            let mut live_session_ids: HashSet<String> = HashSet::with_capacity(INITIAL_SESSION_CAPACITY);

            // Iterate through all audio devices
            for device_index in 0..device_count {
                let device = match device_collection.Item(device_index) {
                    Ok(dev) => dev,
                    Err(_) => continue, // Skip devices we can't access
                };

                // Get audio session manager for this device
                let session_manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                    Ok(mgr) => mgr,
                    Err(_) => continue, // Skip if we can't get session manager
                };

                // Get session enumerator for this device
                let session_enum = match session_manager.GetSessionEnumerator() {
                    Ok(enumerator) => enumerator,
                    Err(_) => continue,
                };

                let count = match session_enum.GetCount() {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // Enumerate sessions for this device
                for i in 0..count {
                    if let Ok(session_control) = session_enum.GetSession(i) {
                        if let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() {
                            // Get session details
                            let process_id = session_control2
                                .GetProcessId()
                                .unwrap_or(0);

                            // Skip system sessions (process_id 0)
                            if process_id == 0 {
                                continue;
                            }

                            let session_id = match session_control2.GetSessionInstanceIdentifier() {
                                Ok(pwstr) => {
                                    let s = pwstr.to_string()
                                        .unwrap_or_else(|_| format!("session_{}", i));
                                    // Free COM-allocated PWSTR to prevent memory leak
                                    CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                                    s
                                }
                                Err(_) => format!("session_{}", i),
                            };

                            let display_name = match session_control2.GetDisplayName() {
                                Ok(pwstr) => {
                                    let s = pwstr.to_string()
                                        .unwrap_or_else(|_| format!("Process {}", process_id));
                                    // Free COM-allocated PWSTR to prevent memory leak
                                    CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                                    s
                                }
                                Err(_) => format!("Process {}", process_id),
                            };

                            // Get the actual process executable name
                            let process_name = get_process_name(process_id);

                            // Get volume control
                            if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                let volume = simple_volume.GetMasterVolume().unwrap_or(1.0);
                                let is_muted = simple_volume.GetMute().unwrap_or(BOOL(0)).as_bool();

                                let session = AudioSession {
                                    session_id: session_id.clone(),
                                    display_name,
                                    process_id,
                                    process_name: process_name.clone(),
                                    volume,
                                    is_muted,
                                };

                                live_session_ids.insert(session_id.clone());
                                sessions.push(session.clone());
                                self.sessions.insert(session_id, session);
                            }
                        }
                    }
                }
            } // End device loop

            // Remove sessions that are no longer active to prevent cache growth
            self.sessions.retain(|id, _| live_session_ids.contains(id));
            
            // Prevent unbounded memory growth by limiting cache size
            if self.sessions.len() > MAX_SESSION_CACHE_SIZE {
                // Keep only the most recent entries
                let mut session_keys: Vec<String> = self.sessions.keys().cloned().collect();
                session_keys.truncate(MAX_SESSION_CACHE_SIZE / 2); // Remove oldest half
                self.sessions.retain(|k, _| session_keys.contains(k));
                eprintln!("[Audio] Cache size limit reached, pruned to {} entries", self.sessions.len());
            }

            self.enumerate_calls = self.enumerate_calls.wrapping_add(1);
            let active_count = live_session_ids.len();
            let cache_count = self.sessions.len();

            let counts_changed = match self.last_logged_counts {
                Some((last_active, last_cache)) => last_active != active_count || last_cache != cache_count,
                None => true,
            };

            if counts_changed || self.enumerate_calls % LOG_INTERVAL == 0 {
                eprintln!(
                    "[Audio] enumerate_sessions: {} active (cache size {}, calls: {})",
                    active_count,
                    cache_count,
                    self.enumerate_calls
                );
                self.last_logged_counts = Some((active_count, cache_count));
            }

            Ok(sessions)
        }
    }

    /// Set volume for a specific session and all sessions of the same process (searches all devices)
    pub fn set_session_volume(&mut self, session_id: &str, volume: f32) -> std::result::Result<(), String> {
        let volume = volume.clamp(0.0, 1.0);
        
        // First, find the process_id for this session
        let target_process_id = self.sessions.get(session_id)
            .map(|s| s.process_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;
        
        let mut updated_count = 0;
        
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            // Get all audio render devices
            let device_collection = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e: Error| format!("Failed to enumerate audio endpoints: {}", e))?;

            let device_count = device_collection.GetCount().unwrap_or(0);

            // Search through all devices for sessions with matching process_id
            for device_index in 0..device_count {
                let device = match device_collection.Item(device_index) {
                    Ok(dev) => dev,
                    Err(_) => continue,
                };

                let session_manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                    Ok(mgr) => mgr,
                    Err(_) => continue,
                };

                let session_enum = match session_manager.GetSessionEnumerator() {
                    Ok(enumerator) => enumerator,
                    Err(_) => continue,
                };

                let count = session_enum.GetCount().unwrap_or(0);

                for i in 0..count {
                    if let Ok(session_control) = session_enum.GetSession(i) {
                        if let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() {
                            let process_id = session_control2
                                .GetProcessId()
                                .unwrap_or(0);

                            // Apply volume to ALL sessions with matching process_id
                            if process_id == target_process_id {
                                if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                    let _ = simple_volume.SetMasterVolume(volume, std::ptr::null());
                                    updated_count += 1;
                                }
                            }
                        }
                    }
                }
            } // End device loop

            // Update cache for the requested session
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.volume = volume;
            }

            if updated_count > 0 {
                Ok(())
            } else {
                Err(format!("No sessions found for process_id: {}", target_process_id))
            }
        }
    }

    /// Mute or unmute all sessions of the same process (searches all devices)
    pub fn set_session_mute(&mut self, session_id: &str, muted: bool) -> std::result::Result<(), String> {
        // First, find the process_id for this session
        let target_process_id = self.sessions.get(session_id)
            .map(|s| s.process_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;
        
        let mut updated_count = 0;
        
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            // Get all audio render devices
            let device_collection = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e: Error| format!("Failed to enumerate audio endpoints: {}", e))?;

            let device_count = device_collection.GetCount().unwrap_or(0);

            // Search through all devices for sessions with matching process_id
            for device_index in 0..device_count {
                let device = match device_collection.Item(device_index) {
                    Ok(dev) => dev,
                    Err(_) => continue,
                };

                let session_manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                    Ok(mgr) => mgr,
                    Err(_) => continue,
                };

                let session_enum = match session_manager.GetSessionEnumerator() {
                    Ok(enumerator) => enumerator,
                    Err(_) => continue,
                };

                let count = session_enum.GetCount().unwrap_or(0);

                for i in 0..count {
                    if let Ok(session_control) = session_enum.GetSession(i) {
                        if let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() {
                            let process_id = session_control2
                                .GetProcessId()
                                .unwrap_or(0);

                            // Apply mute to ALL sessions with matching process_id
                            if process_id == target_process_id {
                                if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                    let _ = simple_volume.SetMute(BOOL(muted as i32), std::ptr::null());
                                    updated_count += 1;
                                }
                            }
                        }
                    }
                }
            } // End device loop

            // Update cache for the requested session
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.is_muted = muted;
            }

            if updated_count > 0 {
                Ok(())
            } else {
                Err(format!("No sessions found for process_id: {}", target_process_id))
            }
        }
    }
}

#[cfg(not(windows))]
impl AudioManager {
    pub fn new() -> std::result::Result<Self, String> {
        Err("Audio manager only supported on Windows".to_string())
    }

    pub fn enumerate_sessions(&mut self) -> std::result::Result<Vec<AudioSession>, String> {
        Err("Audio manager only supported on Windows".to_string())
    }

    pub fn set_session_volume(&mut self, _session_id: &str, _volume: f32) -> std::result::Result<(), String> {
        Err("Audio manager only supported on Windows".to_string())
    }

    pub fn set_session_mute(&mut self, _session_id: &str, _muted: bool) -> std::result::Result<(), String> {
        Err("Audio manager only supported on Windows".to_string())
    }
}

#[cfg(windows)]
impl AudioManager {
    /// Explicit cleanup method for proper resource management
    pub fn cleanup(&mut self) {
        eprintln!("[Audio] Cleaning up audio manager resources...");
        
        // Clear internal caches
        self.sessions.clear();
        // Release memory back to the system
        self.sessions.shrink_to_fit();
        
        // Reset counters
        self.enumerate_calls = 0;
        self.last_logged_counts = None;
        
        // Reset device ID to release string memory
        self.current_device_id = String::new();
        
        eprintln!("[Audio] Audio manager cleanup complete");
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            eprintln!("[Audio] Dropping audio manager...");
            self.cleanup();
            unsafe {
                CoUninitialize();
            }
            eprintln!("[Audio] Audio manager dropped");
        }
    }
}

// Global audio manager instance
static AUDIO_MANAGER: Mutex<Option<AudioManager>> = Mutex::new(None);

/// Initialize the audio manager
#[tauri::command]
pub fn init_audio_manager() -> std::result::Result<String, String> {
    eprintln!("[Audio] Initialising audio manager...");
    let manager = AudioManager::new()?;
    
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    *lock = Some(manager);
    
    eprintln!("[Audio] Audio manager ready");
    Ok("Audio manager initialised successfully".to_string())
}

/// Get all active audio sessions
#[tauri::command]
pub fn get_audio_sessions() -> std::result::Result<Vec<AudioSession>, String> {
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_mut()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.enumerate_sessions()
}

/// Set volume for a specific audio session
#[tauri::command]
pub fn set_session_volume(session_id: String, volume: f32) -> std::result::Result<(), String> {
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_mut()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.set_session_volume(&session_id, volume)
}

/// Mute or unmute a specific audio session
#[tauri::command]
pub fn set_session_mute(session_id: String, muted: bool) -> std::result::Result<(), String> {
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_mut()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.set_session_mute(&session_id, muted)
}

/// Check if the default audio device has changed
/// Returns true if changed, false otherwise
#[tauri::command]
pub fn check_default_device_changed() -> std::result::Result<bool, String> {
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_mut()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.check_device_changed()
}

/// Clean up audio manager resources
#[tauri::command]
pub fn cleanup_audio_manager() -> std::result::Result<String, String> {
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    match lock.as_mut() {
        Some(manager) => {
            manager.cleanup();
            Ok("Audio manager cleaned up successfully".to_string())
        }
        None => Ok("Audio manager not initialised".to_string())
    }
}

/// Get the system (device endpoint) master volume level
#[tauri::command]
pub fn get_system_volume() -> std::result::Result<f32, String> {
    let lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_ref()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.get_system_volume()
}

/// Get the system (device endpoint) mute state
#[tauri::command]
pub fn get_system_mute() -> std::result::Result<bool, String> {
    let lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_ref()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.get_system_mute()
}

/// Set the system (device endpoint) master volume level
#[tauri::command]
pub fn set_system_volume(volume: f32) -> std::result::Result<(), String> {
    let lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_ref()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.set_system_volume(volume)
}

/// Set the system (device endpoint) mute state
#[tauri::command]
pub fn set_system_mute(muted: bool) -> std::result::Result<(), String> {
    let lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    let manager = lock
        .as_ref()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    
    manager.set_system_mute(muted)
}
