use std::sync::Mutex;
use std::sync::mpsc;
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
    Win32::UI::WindowsAndMessaging::*,
};

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

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

// ─────────────────────────────────────────────────────────────────────────────
// Channel-Based Audio Thread Infrastructure
// ─────────────────────────────────────────────────────────────────────────────

/// Messages sent from Tauri commands to the dedicated audio thread.
/// Each variant carries a oneshot sender for the result.
enum AudioCommand {
    EnumerateSessions {
        reply: mpsc::Sender<std::result::Result<Vec<AudioSession>, String>>,
    },
    SetSessionVolume {
        session_id: String,
        volume: f32,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    SetSessionMute {
        session_id: String,
        muted: bool,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    CheckDeviceChanged {
        reply: mpsc::Sender<std::result::Result<bool, String>>,
    },
    GetSystemVolume {
        reply: mpsc::Sender<std::result::Result<f32, String>>,
    },
    GetSystemMute {
        reply: mpsc::Sender<std::result::Result<bool, String>>,
    },
    SetSystemVolume {
        volume: f32,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    SetSystemMute {
        muted: bool,
        reply: mpsc::Sender<std::result::Result<(), String>>,
    },
    Cleanup {
        reply: mpsc::Sender<std::result::Result<String, String>>,
    },
    Shutdown,
}

/// Handle to the dedicated audio thread, used by Tauri commands to send messages.
pub struct AudioThreadHandle {
    sender: mpsc::Sender<AudioCommand>,
}

impl AudioThreadHandle {
    /// Send a command and wait for the response.
    fn send_and_recv<T>(
        &self,
        build_cmd: impl FnOnce(mpsc::Sender<std::result::Result<T, String>>) -> AudioCommand,
    ) -> std::result::Result<T, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(build_cmd(reply_tx))
            .map_err(|_| "Audio thread is not running".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "Audio thread did not respond".to_string())?
    }
}

/// Manages Windows Core Audio API for application volume control.
/// Lives exclusively on the dedicated audio thread.
struct AudioManager {
    sessions: HashMap<String, AudioSession>,
    current_device_id: String,
    enumerate_calls: usize,
    last_logged_counts: Option<(usize, usize)>,
    /// Cached COM objects — only recreated on device change (Fix 3)
    #[cfg(windows)]
    cached_enumerator: Option<IMMDeviceEnumerator>,
    #[cfg(windows)]
    cached_device: Option<IMMDevice>,
    #[cfg(windows)]
    cached_endpoint_volume: Option<IAudioEndpointVolume>,
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
/// Get the executable name and full path from a process ID with proper resource cleanup
fn get_process_name(process_id: u32) -> (String, String) {
    if process_id == 0 {
        return ("System".to_string(), String::new());
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
                let path = String::from_utf16_lossy(&buffer[0..size as usize]).to_string();

                // Extract just the filename from the full path
                let filename = path.split('\\')
                    .next_back()
                    .unwrap_or("Unknown")
                    .to_string();

                return (filename, path);
            }
            // ProcessHandle automatically closes on drop
        }
    }

    // Fallback if we can't get the process name
    (format!("Process {}", process_id), String::new())
}

#[cfg(windows)]
/// Query a single string value from a pre-loaded version info buffer.
/// Returns the string if found and non-empty.
unsafe fn query_version_string(
    buffer: &[u8],
    lang: u32,
    cp: u32,
    field: &str,
) -> Option<String> {
    extern "system" {
        fn VerQueryValueW(
            pblock: *const std::ffi::c_void,
            lpsubblock: *const u16,
            lplpbuffer: *mut *mut std::ffi::c_void,
            pulen: *mut u32,
        ) -> windows::Win32::Foundation::BOOL;
    }

    let query = format!("\\StringFileInfo\\{:04x}{:04x}\\{}\0", lang, cp, field);
    let query_wide: Vec<u16> = query.encode_utf16().collect();

    let mut value_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut value_len = 0u32;

    if VerQueryValueW(
        buffer.as_ptr() as *const std::ffi::c_void,
        query_wide.as_ptr(),
        &mut value_ptr,
        &mut value_len,
    )
    .as_bool()
        && !value_ptr.is_null()
        && value_len > 0
    {
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(
            value_ptr as *const u16,
            value_len as usize - 1, // exclude null terminator
        ));
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(windows)]
/// Get the friendly application name from the executable's version resource.
/// Tries FileDescription then ProductName, across the embedded translation
/// and several common language/codepage fallbacks.
fn get_friendly_name(executable_path: &str) -> Option<String> {
    use std::ptr;

    // Fields to try, in order of preference
    const FIELDS: &[&str] = &["FileDescription", "ProductName"];

    // Common language + codepage pairs to try when the Translation block is
    // absent or its entries don't contain the fields we want.
    const FALLBACK_LANGS: &[(u32, u32)] = &[
        (0x0409, 0x04B0), // English US, Unicode
        (0x0409, 0x04E4), // English US, Windows-1252
        (0x0000, 0x04B0), // Language-neutral, Unicode
        (0x0809, 0x04B0), // English UK, Unicode
    ];

    unsafe {
        // Convert path to null-terminated UTF-16 string
        let wide_path: Vec<u16> = OsStr::new(executable_path)
            .encode_wide()
            .chain(Some(0))
            .collect();

        extern "system" {
            fn GetFileVersionInfoSizeW(lptstrfilename: *const u16, lpdwhandle: *mut u32) -> u32;
            fn GetFileVersionInfoW(
                lptstrfilename: *const u16,
                dwhandle: u32,
                dwlen: u32,
                lpdata: *mut std::ffi::c_void,
            ) -> windows::Win32::Foundation::BOOL;
            fn VerQueryValueW(
                pblock: *const std::ffi::c_void,
                lpsubblock: *const u16,
                lplpbuffer: *mut *mut std::ffi::c_void,
                pulen: *mut u32,
            ) -> windows::Win32::Foundation::BOOL;
        }

        let size = GetFileVersionInfoSizeW(wide_path.as_ptr(), ptr::null_mut());
        if size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        if !GetFileVersionInfoW(
            wide_path.as_ptr(),
            0,
            size,
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
        )
        .as_bool()
        {
            return None;
        }

        // Collect language/codepage pairs to try: embedded translations first,
        // then common fallbacks
        let mut lang_pairs: Vec<(u32, u32)> = Vec::new();

        let mut translation_ptr: *mut std::ffi::c_void = ptr::null_mut();
        let mut translation_size = 0u32;

        if VerQueryValueW(
            buffer.as_ptr() as *const std::ffi::c_void,
            "\\VarFileInfo\\Translation\0"
                .encode_utf16()
                .collect::<Vec<_>>()
                .as_ptr(),
            &mut translation_ptr,
            &mut translation_size,
        )
        .as_bool()
            && !translation_ptr.is_null()
            && translation_size >= 4
        {
            let pair_count = (translation_size as usize) / 4;
            let data = translation_ptr as *const u16;
            for idx in 0..pair_count {
                let lang = *data.add(idx * 2) as u32;
                let cp = *data.add(idx * 2 + 1) as u32;
                lang_pairs.push((lang, cp));
            }
        }

        // Append fallback codes (duplicates are harmless; we stop on first hit)
        for &pair in FALLBACK_LANGS {
            if !lang_pairs.contains(&pair) {
                lang_pairs.push(pair);
            }
        }

        // Try each field with each language pair
        for field in FIELDS {
            for &(lang, cp) in &lang_pairs {
                if let Some(name) = query_version_string(&buffer, lang, cp, field) {
                    return Some(name);
                }
            }
        }
    }

    None
}

#[cfg(windows)]
/// Get the main window title for a process. This is the fallback the Windows
/// Volume Mixer uses when neither version resources nor COM display names are set.
fn get_window_title(process_id: u32) -> Option<String> {
    // Struct to carry data in/out of the EnumWindows callback
    struct EnumData {
        target_pid: u32,
        best_title: String,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumData);

        let mut window_pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));

        if window_pid != data.target_pid {
            return BOOL(1); // continue
        }

        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1); // skip invisible windows
        }

        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        if len > 0 {
            let title = String::from_utf16_lossy(&buffer[..len as usize]);
            // Keep the longest visible window title (main window typically has the longest)
            if title.len() > data.best_title.len() {
                data.best_title = title;
            }
        }

        BOOL(1) // continue enumeration
    }

    unsafe {
        let mut data = EnumData {
            target_pid: process_id,
            best_title: String::new(),
        };

        let _ = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut data as *mut EnumData as isize),
        );

        if data.best_title.is_empty() {
            None
        } else {
            Some(data.best_title)
        }
    }
}

#[cfg(windows)]
impl AudioManager {
    /// Create a new audio manager instance.
    /// Must be called on the dedicated audio thread after CoInitializeEx.
    fn new() -> std::result::Result<Self, String> {
        tracing::info!("[Audio] Detecting default audio device...");
        let device_id = Self::get_default_device_id_fresh()?;
        tracing::info!("[Audio] Default device: {}", device_id);
        
        let mut mgr = Self {
            sessions: HashMap::new(),
            current_device_id: device_id,
            enumerate_calls: 0,
            last_logged_counts: None,
            cached_enumerator: None,
            cached_device: None,
            cached_endpoint_volume: None,
        };
        
        // Pre-populate the COM cache
        mgr.rebuild_com_cache()?;
        
        Ok(mgr)
    }
    
    /// Build or rebuild the cached COM object chain.
    /// Called once at init and again whenever the default device changes.
    fn rebuild_com_cache(&mut self) -> std::result::Result<(), String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e: Error| format!("Failed to get default audio endpoint: {}", e))?;

            let endpoint_volume: IAudioEndpointVolume = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e: Error| format!("Failed to activate endpoint volume: {}", e))?;

            self.cached_enumerator = Some(enumerator);
            self.cached_device = Some(device);
            self.cached_endpoint_volume = Some(endpoint_volume);

            tracing::debug!("[Audio] COM cache rebuilt successfully");
            Ok(())
        }
    }

    /// Get a fresh default device ID without relying on cached objects.
    /// Used for device-change detection.
    fn get_default_device_id_fresh() -> std::result::Result<String, String> {
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

            CoTaskMemFree(Some(id.0 as *const core::ffi::c_void));

            id_string
        }
    }
    
    /// Check if default device has changed, return true if changed.
    /// Automatically rebuilds the COM cache on device change.
    fn check_device_changed(&mut self) -> std::result::Result<bool, String> {
        let new_device_id = Self::get_default_device_id_fresh()?;
        
        if new_device_id != self.current_device_id {
            tracing::info!("[Audio] Default device changed: {} -> {}", self.current_device_id, new_device_id);
            self.current_device_id = new_device_id;
            // Invalidate and rebuild the COM cache for the new device
            self.cached_enumerator = None;
            self.cached_device = None;
            self.cached_endpoint_volume = None;
            self.rebuild_com_cache()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Get the cached endpoint volume interface (no per-call COM allocations).
    fn get_endpoint_volume(&self) -> std::result::Result<&IAudioEndpointVolume, String> {
        self.cached_endpoint_volume
            .as_ref()
            .ok_or_else(|| "Endpoint volume not cached — device may have changed".to_string())
    }

    /// Get the cached device enumerator.
    fn get_enumerator(&self) -> std::result::Result<&IMMDeviceEnumerator, String> {
        self.cached_enumerator
            .as_ref()
            .ok_or_else(|| "Device enumerator not cached".to_string())
    }

    /// Get the system (device endpoint) master volume level (0.0 to 1.0)
    fn get_system_volume(&self) -> std::result::Result<f32, String> {
        unsafe {
            self.get_endpoint_volume()?
                .GetMasterVolumeLevelScalar()
                .map_err(|e: Error| format!("Failed to get master volume: {}", e))
        }
    }

    /// Get the system (device endpoint) mute state
    fn get_system_mute(&self) -> std::result::Result<bool, String> {
        unsafe {
            Ok(self.get_endpoint_volume()?
                .GetMute()
                .map_err(|e: Error| format!("Failed to get mute state: {}", e))?
                .as_bool())
        }
    }

    /// Set the system (device endpoint) master volume level (0.0 to 1.0)
    fn set_system_volume(&self, volume: f32) -> std::result::Result<(), String> {
        let volume = volume.clamp(0.0, 1.0);
        unsafe {
            self.get_endpoint_volume()?
                .SetMasterVolumeLevelScalar(volume, std::ptr::null())
                .map_err(|e: Error| format!("Failed to set master volume: {}", e))
        }
    }

    /// Set the system (device endpoint) mute state
    fn set_system_mute(&self, muted: bool) -> std::result::Result<(), String> {
        unsafe {
            self.get_endpoint_volume()?
                .SetMute(BOOL(muted as i32), std::ptr::null())
                .map_err(|e: Error| format!("Failed to set mute state: {}", e))
        }
    }

    /// Enumerate all active audio sessions from all audio devices with proper resource management.
    /// Uses the cached IMMDeviceEnumerator to avoid per-call COM allocations.
    fn enumerate_sessions(&mut self) -> std::result::Result<Vec<AudioSession>, String> {
        unsafe {
            // Use the cached enumerator instead of creating a new one each call
            let enumerator = self.get_enumerator()?;

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

                            // Get the actual process executable name and path
                            let (process_name, executable_path) = get_process_name(process_id);

                            // Determine the best display name from multiple sources:
                            // 1. Version resource (FileDescription → ProductName)
                            // 2. COM session display name (set at runtime via SetDisplayName)
                            // 3. Process window title (what the Windows Volume Mixer uses as fallback)
                            // 4. Empty string → frontend formats the process name
                            let friendly_display_name = {
                                let version_name = if !executable_path.is_empty() {
                                    get_friendly_name(&executable_path)
                                } else {
                                    None
                                };

                                let com_name_is_useful = !display_name.is_empty()
                                    && !display_name.starts_with('@')
                                    && !display_name.starts_with("Process ");

                                if let Some(name) = version_name {
                                    name
                                } else if com_name_is_useful {
                                    display_name.clone()
                                } else if let Some(title) = get_window_title(process_id) {
                                    title
                                } else {
                                    String::new()
                                }
                            };

                            // Get volume control
                            if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                let volume = simple_volume.GetMasterVolume().unwrap_or(1.0);
                                let is_muted = simple_volume.GetMute().unwrap_or(BOOL(0)).as_bool();

                                let session = AudioSession {
                                    session_id: session_id.clone(),
                                    display_name: friendly_display_name,
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

            // Post-process: propagate the best display name across all sessions
            // of the same process. Games like MSFS2024 create multiple audio sessions
            // but only set SetDisplayName() on one of them, so others appear nameless.
            let mut best_names: HashMap<String, String> = HashMap::new();
            for session in &sessions {
                if !session.display_name.is_empty() {
                    let existing = best_names.get(&session.process_name);
                    // Keep the longer/more descriptive name if multiple sessions have names
                    if existing.is_none() || session.display_name.len() > existing.unwrap().len() {
                        best_names.insert(session.process_name.clone(), session.display_name.clone());
                    }
                }
            }
            // Apply best names to all sessions that are missing one
            for session in &mut sessions {
                if session.display_name.is_empty() {
                    if let Some(name) = best_names.get(&session.process_name) {
                        session.display_name = name.clone();
                        // Also update the cache entry
                        if let Some(cached) = self.sessions.get_mut(&session.session_id) {
                            cached.display_name = name.clone();
                        }
                    }
                }
            }

            // Remove sessions that are no longer active to prevent cache growth
            self.sessions.retain(|id, _| live_session_ids.contains(id));
            
            // Prevent unbounded memory growth by limiting cache size
            if self.sessions.len() > MAX_SESSION_CACHE_SIZE {
                // Keep only the most recent entries
                let mut session_keys: Vec<String> = self.sessions.keys().cloned().collect();
                session_keys.truncate(MAX_SESSION_CACHE_SIZE / 2); // Remove oldest half
                self.sessions.retain(|k, _| session_keys.contains(k));
                tracing::warn!("[Audio] Cache size limit reached, pruned to {} entries", self.sessions.len());
            }

            self.enumerate_calls = self.enumerate_calls.wrapping_add(1);
            let active_count = live_session_ids.len();
            let cache_count = self.sessions.len();

            let counts_changed = match self.last_logged_counts {
                Some((last_active, last_cache)) => last_active != active_count || last_cache != cache_count,
                None => true,
            };

            if counts_changed || self.enumerate_calls % LOG_INTERVAL == 0 {
                tracing::debug!(
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

    /// Set volume for all audio sessions belonging to the same process as the target session.
    /// Games like MSFS2024 create multiple sessions; controlling only one leaves others unaffected.
    fn set_session_volume(&mut self, session_id: &str, volume: f32) -> std::result::Result<(), String> {
        let volume = volume.clamp(0.0, 1.0);

        // Look up the target process ID from the session cache so we can update
        // every session belonging to the same application (not just one instance)
        let target_process_id = self.sessions.get(session_id).map(|s| s.process_id);
        
        unsafe {
            let enumerator = self.get_enumerator()?;

            let device_collection = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e: Error| format!("Failed to enumerate audio endpoints: {}", e))?;

            let device_count = device_collection.GetCount().unwrap_or(0);
            let mut updated_count: u32 = 0;

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
                            // Determine whether this session belongs to the target application
                            let should_update = if let Some(target_pid) = target_process_id {
                                // Match by process ID to capture ALL sessions of the app
                                let pid = session_control2.GetProcessId().unwrap_or(0);
                                pid == target_pid
                            } else {
                                // Fallback: match by exact session ID if not in cache
                                let current_session_id = match session_control2.GetSessionInstanceIdentifier() {
                                    Ok(pwstr) => {
                                        let s = pwstr.to_string()
                                            .unwrap_or_else(|_| format!("session_{}", i));
                                        CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                                        s
                                    }
                                    Err(_) => format!("session_{}", i),
                                };
                                current_session_id == session_id
                            };

                            if should_update {
                                if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                    match simple_volume.SetMasterVolume(volume, std::ptr::null()) {
                                        Ok(()) => { updated_count += 1; }
                                        Err(e) => {
                                            tracing::warn!("[Audio] SetMasterVolume failed: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } // End device loop

            if updated_count > 0 {
                // Update all cached sessions for this process
                if let Some(target_pid) = target_process_id {
                    for session in self.sessions.values_mut() {
                        if session.process_id == target_pid {
                            session.volume = volume;
                        }
                    }
                } else if let Some(session) = self.sessions.get_mut(session_id) {
                    session.volume = volume;
                }
                Ok(())
            } else {
                Err(format!("No sessions updated for: {}", session_id))
            }
        }
    }

    /// Mute or unmute all audio sessions belonging to the same process as the target session.
    fn set_session_mute(&mut self, session_id: &str, muted: bool) -> std::result::Result<(), String> {
        // Look up the target process ID from the session cache
        let target_process_id = self.sessions.get(session_id).map(|s| s.process_id);

        unsafe {
            let enumerator = self.get_enumerator()?;

            let device_collection = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e: Error| format!("Failed to enumerate audio endpoints: {}", e))?;

            let device_count = device_collection.GetCount().unwrap_or(0);
            let mut updated_count: u32 = 0;

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
                            // Determine whether this session belongs to the target application
                            let should_update = if let Some(target_pid) = target_process_id {
                                let pid = session_control2.GetProcessId().unwrap_or(0);
                                pid == target_pid
                            } else {
                                let current_session_id = match session_control2.GetSessionInstanceIdentifier() {
                                    Ok(pwstr) => {
                                        let s = pwstr.to_string()
                                            .unwrap_or_else(|_| format!("session_{}", i));
                                        CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                                        s
                                    }
                                    Err(_) => format!("session_{}", i),
                                };
                                current_session_id == session_id
                            };

                            if should_update {
                                if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                    match simple_volume.SetMute(BOOL(muted as i32), std::ptr::null()) {
                                        Ok(()) => { updated_count += 1; }
                                        Err(e) => {
                                            tracing::warn!("[Audio] SetMute failed: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } // End device loop

            if updated_count > 0 {
                // Update all cached sessions for this process
                if let Some(target_pid) = target_process_id {
                    for session in self.sessions.values_mut() {
                        if session.process_id == target_pid {
                            session.is_muted = muted;
                        }
                    }
                } else if let Some(session) = self.sessions.get_mut(session_id) {
                    session.is_muted = muted;
                }
                Ok(())
            } else {
                Err(format!("No sessions updated for: {}", session_id))
            }
        }
    }
}

#[cfg(windows)]
impl AudioManager {
    /// Explicit cleanup method for proper resource management
    fn cleanup(&mut self) {
        tracing::info!("[Audio] Cleaning up audio manager resources...");
        
        // Drop cached COM objects
        self.cached_endpoint_volume = None;
        self.cached_device = None;
        self.cached_enumerator = None;
        
        // Clear internal caches
        self.sessions.clear();
        self.sessions.shrink_to_fit();
        
        self.enumerate_calls = 0;
        self.last_logged_counts = None;
        self.current_device_id = String::new();
        
        tracing::info!("[Audio] Audio manager cleanup complete");
    }
}

// CoUninitialize is called explicitly in the audio thread's run loop on exit,
// so Drop does not need to handle it.

// ─────────────────────────────────────────────────────────────────────────────
// Dedicated Audio Thread
// ─────────────────────────────────────────────────────────────────────────────

/// Global handle to the audio thread, set once by init_audio_manager.
static AUDIO_THREAD_HANDLE: Mutex<Option<AudioThreadHandle>> = Mutex::new(None);

/// Spawn the dedicated audio thread. CoInitializeEx is called at the top of
/// the thread and CoUninitialize when it exits. The thread processes commands
/// from the mpsc channel until it receives Shutdown (or the channel closes).
#[cfg(windows)]
fn spawn_audio_thread() -> std::result::Result<AudioThreadHandle, String> {
    let (tx, rx) = mpsc::channel::<AudioCommand>();

    std::thread::Builder::new()
        .name("audio-com".to_string())
        .spawn(move || {
            // Initialise COM on this dedicated thread
            unsafe {
                if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
                    tracing::error!("[Audio] CoInitializeEx failed on audio thread: {}", e);
                    // Drain any waiting commands with an error
                    while let Ok(cmd) = rx.try_recv() {
                        reply_error(cmd, format!("COM init failed: {}", e));
                    }
                    return;
                }
            }

            let manager_result = AudioManager::new();
            let mut manager = match manager_result {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("[Audio] AudioManager::new() failed: {}", e);
                    while let Ok(cmd) = rx.try_recv() {
                        reply_error(cmd, e.clone());
                    }
                    unsafe { CoUninitialize(); }
                    return;
                }
            };

            tracing::info!("[Audio] Dedicated audio thread running");

            // Process commands until shutdown or channel disconnect
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    AudioCommand::EnumerateSessions { reply } => {
                        let _ = reply.send(manager.enumerate_sessions());
                    }
                    AudioCommand::SetSessionVolume { session_id, volume, reply } => {
                        let _ = reply.send(manager.set_session_volume(&session_id, volume));
                    }
                    AudioCommand::SetSessionMute { session_id, muted, reply } => {
                        let _ = reply.send(manager.set_session_mute(&session_id, muted));
                    }
                    AudioCommand::CheckDeviceChanged { reply } => {
                        let _ = reply.send(manager.check_device_changed());
                    }
                    AudioCommand::GetSystemVolume { reply } => {
                        let _ = reply.send(manager.get_system_volume());
                    }
                    AudioCommand::GetSystemMute { reply } => {
                        let _ = reply.send(manager.get_system_mute());
                    }
                    AudioCommand::SetSystemVolume { volume, reply } => {
                        let _ = reply.send(manager.set_system_volume(volume));
                    }
                    AudioCommand::SetSystemMute { muted, reply } => {
                        let _ = reply.send(manager.set_system_mute(muted));
                    }
                    AudioCommand::Cleanup { reply } => {
                        manager.cleanup();
                        let _ = reply.send(Ok("Audio manager cleaned up successfully".to_string()));
                    }
                    AudioCommand::Shutdown => {
                        tracing::info!("[Audio] Shutdown command received");
                        break;
                    }
                }
            }

            // Clean up before thread exits
            manager.cleanup();
            unsafe { CoUninitialize(); }
            tracing::info!("[Audio] Dedicated audio thread exited");
        })
        .map_err(|e| format!("Failed to spawn audio thread: {}", e))?;

    Ok(AudioThreadHandle { sender: tx })
}

/// Helper: send an error reply for any command variant when the thread cannot process it.
#[cfg(windows)]
fn reply_error(cmd: AudioCommand, err: String) {
    match cmd {
        AudioCommand::EnumerateSessions { reply } => { let _ = reply.send(Err(err)); }
        AudioCommand::SetSessionVolume { reply, .. } => { let _ = reply.send(Err(err)); }
        AudioCommand::SetSessionMute { reply, .. } => { let _ = reply.send(Err(err)); }
        AudioCommand::CheckDeviceChanged { reply } => { let _ = reply.send(Err(err)); }
        AudioCommand::GetSystemVolume { reply } => { let _ = reply.send(Err(err)); }
        AudioCommand::GetSystemMute { reply } => { let _ = reply.send(Err(err)); }
        AudioCommand::SetSystemVolume { reply, .. } => { let _ = reply.send(Err(err)); }
        AudioCommand::SetSystemMute { reply, .. } => { let _ = reply.send(Err(err)); }
        AudioCommand::Cleanup { reply } => { let _ = reply.send(Err(err)); }
        AudioCommand::Shutdown => {}
    }
}

/// Get a reference to the audio thread handle, or return an appropriate error.
fn get_handle() -> std::result::Result<std::sync::MutexGuard<'static, Option<AudioThreadHandle>>, String> {
    AUDIO_THREAD_HANDLE
        .lock()
        .map_err(|e| format!("Failed to lock audio thread handle: {}", e))
}

fn with_handle<T>(
    f: impl FnOnce(&AudioThreadHandle) -> std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    let guard = get_handle()?;
    let handle = guard
        .as_ref()
        .ok_or("Audio manager not initialised. Call init_audio_manager first.")?;
    f(handle)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands — thin wrappers that send/receive via the channel
// ─────────────────────────────────────────────────────────────────────────────

/// Initialise the audio manager on a dedicated COM thread
#[tauri::command]
pub fn init_audio_manager() -> std::result::Result<String, String> {
    tracing::info!("[Audio] Spawning dedicated audio thread...");
    let handle = spawn_audio_thread()?;
    
    let mut lock = AUDIO_THREAD_HANDLE
        .lock()
        .map_err(|e| format!("Failed to lock audio thread handle: {}", e))?;
    *lock = Some(handle);
    
    tracing::info!("[Audio] Audio manager ready (dedicated thread)");
    Ok("Audio manager initialised successfully".to_string())
}

/// Get all active audio sessions
#[tauri::command]
pub fn get_audio_sessions() -> std::result::Result<Vec<AudioSession>, String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::EnumerateSessions { reply }))
}

/// Set volume for a specific audio session
#[tauri::command]
pub fn set_session_volume(session_id: String, volume: f32) -> std::result::Result<(), String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::SetSessionVolume { session_id: session_id.clone(), volume, reply }))
}

/// Mute or unmute a specific audio session
#[tauri::command]
pub fn set_session_mute(session_id: String, muted: bool) -> std::result::Result<(), String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::SetSessionMute { session_id: session_id.clone(), muted, reply }))
}

/// Check if the default audio device has changed
#[tauri::command]
pub fn check_default_device_changed() -> std::result::Result<bool, String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::CheckDeviceChanged { reply }))
}

/// Clean up audio manager resources
#[tauri::command]
pub fn cleanup_audio_manager() -> std::result::Result<String, String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::Cleanup { reply }))
}

/// Get the system (device endpoint) master volume level
#[tauri::command]
pub fn get_system_volume() -> std::result::Result<f32, String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::GetSystemVolume { reply }))
}

/// Get the system (device endpoint) mute state
#[tauri::command]
pub fn get_system_mute() -> std::result::Result<bool, String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::GetSystemMute { reply }))
}

/// Set the system (device endpoint) master volume level
#[tauri::command]
pub fn set_system_volume(volume: f32) -> std::result::Result<(), String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::SetSystemVolume { volume, reply }))
}

/// Set the system (device endpoint) mute state
#[tauri::command]
pub fn set_system_mute(muted: bool) -> std::result::Result<(), String> {
    with_handle(|h| h.send_and_recv(|reply| AudioCommand::SetSystemMute { muted, reply }))
}

/// Send a shutdown signal to the audio thread.
/// Called during app shutdown to ensure clean COM teardown.
pub fn shutdown_audio_thread() {
    if let Ok(mut lock) = AUDIO_THREAD_HANDLE.lock() {
        if let Some(handle) = lock.take() {
            let _ = handle.sender.send(AudioCommand::Shutdown);
            tracing::info!("[Audio] Shutdown signal sent to audio thread");
        }
    }
}
