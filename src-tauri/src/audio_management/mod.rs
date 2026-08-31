use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use tauri::Emitter;

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
use windows::core::implement;
#[cfg(windows)]
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum path length for Windows process names (MAX_PATH)
const MAX_PATH_LENGTH: usize = 260;

/// Initial capacity for session vectors (reasonable estimate for typical systems)
const INITIAL_SESSION_CAPACITY: usize = 64;

/// Interval for logging enumerate calls (every N calls)
const LOG_INTERVAL: usize = 200;

/// How often the audio thread polls its topology-changed flag between commands.
/// Short enough that device/session changes are noticed promptly.
const FLAG_CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// Safety-net interval for a proactive full session enumeration.
/// Catches external volume changes not covered by COM notifications.
const SAFETY_NET_INTERVAL: Duration = Duration::from_secs(10);

/// Information about an audio session (application)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Join handle for the audio COM thread. Joined during shutdown so the
    /// thread's COM teardown completes before the process exits — terminating
    /// it mid-call via `ExitProcess` risks an access violation inside MMDevAPI.
    audio_join_handle: Option<std::thread::JoinHandle<()>>,
    /// Signalled by the audio COM thread immediately before it returns, so
    /// shutdown can wait for it with a bounded timeout instead of blocking
    /// indefinitely on a stalled COM call.
    audio_done_rx: mpsc::Receiver<()>,
    /// Win32 event handle (as isize) used to signal the notification thread to shut down.
    #[cfg(windows)]
    notification_shutdown_event: isize,
    /// Join handle for the notification background thread.
    #[cfg(windows)]
    notification_join_handle: Option<std::thread::JoinHandle<()>>,
}

/// Maximum time shutdown waits for a background thread to finish before giving
/// up and letting the process exit without joining it.
const THREAD_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

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
        // A 5-second timeout prevents Tauri commands from blocking indefinitely
        // if the audio thread panics, deadlocks, or stalls mid-operation.
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => "Audio thread timed out (>5s)".to_string(),
                mpsc::RecvTimeoutError::Disconnected => "Audio thread did not respond".to_string(),
            })?
    }
}

/// COM callback object that receives Windows audio endpoint change notifications.
/// Registered with IMMDeviceEnumerator::RegisterEndpointNotificationCallback on
/// a dedicated MTA notification thread so callbacks are delivered without needing
/// a Win32 message pump. Sets an atomic flag that the audio COM thread checks.
#[cfg(windows)]
#[implement(IMMNotificationClient)]
struct DeviceChangeCallback {
    flag: Arc<AtomicBool>,
}

#[cfg(windows)]
impl IMMNotificationClient_Impl for DeviceChangeCallback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        self.flag.store(true, Ordering::Release);
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.flag.store(true, Ordering::Release);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.flag.store(true, Ordering::Release);
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        // Only react to the default render endpoint used for console applications —
        // this is the device that the Windows Volume Mixer targets.
        if flow == eRender && role == eConsole {
            self.flag.store(true, Ordering::Release);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Manages Windows Core Audio API for application volume control.
/// Lives exclusively on the dedicated audio thread.
struct AudioManager {
    sessions: HashMap<String, AudioSession>,
    current_device_id: String,
    enumerate_calls: usize,
    last_logged_counts: Option<(usize, usize)>,
    /// Cached COM objects — only recreated on device change
    #[cfg(windows)]
    cached_enumerator: Option<IMMDeviceEnumerator>,
    #[cfg(windows)]
    cached_device: Option<IMMDevice>,
    #[cfg(windows)]
    cached_endpoint_volume: Option<IAudioEndpointVolume>,
    /// Volume interfaces per process ID, rebuilt by every `enumerate_sessions`.
    ///
    /// Without this, each volume or mute write had to walk every render endpoint
    /// and every session on it to find the handful belonging to one process.
    /// That runs up to 25 times a second while a slider is held, which is the
    /// worst possible moment to be making thousands of COM calls: the simulator
    /// is competing for the same cores.
    ///
    /// A process gets several entries when it opens sessions on more than one
    /// endpoint, which is why the value is a list.
    #[cfg(windows)]
    volume_cache: HashMap<u32, Vec<ISimpleAudioVolume>>,
    /// Tauri app handle for emitting push events to the frontend.
    app_handle: tauri::AppHandle,
    /// Last session list emitted to the frontend; used for change detection.
    last_emitted_sessions: Vec<AudioSession>,
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
        // VerQueryValueW returns a pointer *into* `buffer`. Bound the read by the
        // bytes actually remaining after that offset — clamping to the buffer's
        // total capacity would still allow a read past the end of the allocation
        // when the value sits near the end and a malformed PE reports an
        // oversized length.
        let base = buffer.as_ptr() as usize;
        let value_addr = value_ptr as usize;
        if value_addr < base || value_addr >= base + buffer.len() {
            // Pointer is outside the block we passed in — refuse to read it.
            return None;
        }
        let remaining_bytes = buffer.len() - (value_addr - base);

        // `value_len` is in characters and includes the null terminator.
        let char_count = (value_len as usize - 1) // exclude null terminator
            .min(remaining_bytes / 2);            // cap to what the buffer holds
        if char_count == 0 {
            return None;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(
            value_ptr as *const u16,
            char_count,
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

        // GetFileVersionInfoSizeW occasionally underreports the required buffer size for
        // certain executables. Allocate extra padding and pass the padded size to
        // GetFileVersionInfoW so it knows the true available space, preventing a heap
        // overflow if the actual data is slightly larger than reported.
        // Keep the arithmetic in u32 (matching the API type) to avoid a widening-then-
        // truncating cast that would silently pass a smaller size than the buffer.
        let padded_size = size.saturating_add(512);
        let mut buffer = vec![0u8; padded_size as usize];
        if !GetFileVersionInfoW(
            wide_path.as_ptr(),
            0,
            padded_size,
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
    fn new(app_handle: tauri::AppHandle) -> std::result::Result<Self, String> {
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
            volume_cache: HashMap::new(),
            app_handle,
            last_emitted_sessions: Vec::new(),
        };

        // Pre-populate the COM cache
        mgr.rebuild_com_cache()?;

        Ok(mgr)
    }

    /// Build or rebuild the cached COM object chain.
    /// Called once at init and again whenever the default device changes.
    ///
    /// # Why no IMMNotificationClient is registered here
    /// Endpoint notifications are owned exclusively by the dedicated MTA
    /// notification thread (see `spawn_notification_thread`), which holds a
    /// single registration for the entire process lifetime.
    ///
    /// Registering a second client against this short-lived enumerator was
    /// unsound: `RegisterEndpointNotificationCallback` does *not* AddRef the
    /// client — MMDevAPI stores a raw pointer and the caller must keep the
    /// object alive until `UnregisterEndpointNotificationCallback` is called.
    /// Every cache rebuild dropped the still-registered callback, leaving
    /// MMDevAPI holding a dangling pointer; the next endpoint notification then
    /// called through freed memory and faulted with STATUS_ACCESS_VIOLATION at
    /// an unpredictable point later in the session.
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
    ///
    /// The cache is also rebuilt when it is absent even if the device ID is
    /// unchanged — this is the recovery path after an explicit `Cleanup`
    /// command tears the cache down while the thread keeps running.
    fn check_device_changed(&mut self) -> std::result::Result<bool, String> {
        let new_device_id = Self::get_default_device_id_fresh()?;
        let cache_missing = self.cached_enumerator.is_none();

        if new_device_id != self.current_device_id || cache_missing {
            if cache_missing {
                tracing::info!("[Audio] COM cache absent — rebuilding for device {}", new_device_id);
            } else {
                tracing::info!("[Audio] Default device changed: {} -> {}", self.current_device_id, new_device_id);
            }
            self.current_device_id = new_device_id;
            // Invalidate and rebuild the COM cache for the new device. Nothing
            // registered against the old enumerator survives this, so dropping
            // it here is safe (see the note on `rebuild_com_cache`).
            self.cached_enumerator = None;
            self.cached_device = None;
            self.cached_endpoint_volume = None;
            // Interfaces obtained from the old endpoint are invalidated by the
            // switch; the next enumeration repopulates them.
            self.volume_cache.clear();
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

            // Rebuilt from scratch so interfaces for ended sessions are dropped
            // rather than accumulating.
            let mut volume_cache: HashMap<u32, Vec<ISimpleAudioVolume>> = HashMap::new();

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

                            // The fallback is keyed on the device and process
                            // rather than the loop index. An index shifts as
                            // sessions come and go, so it was not stable between
                            // enumerations, and two devices both produced
                            // "session_0" — merging unrelated applications in
                            // the cache and in anything mapped to them.
                            let session_id = match session_control2.GetSessionInstanceIdentifier() {
                                Ok(pwstr) => {
                                    let s = pwstr.to_string()
                                        .unwrap_or_else(|_| format!("device{}_pid{}", device_index, process_id));
                                    // Free COM-allocated PWSTR to prevent memory leak
                                    CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                                    s
                                }
                                Err(_) => format!("device{}_pid{}", device_index, process_id),
                            };

                            // If this session was already resolved in a previous enumeration,
                            // reuse the cached names and skip the expensive per-process I/O:
                            // QueryFullProcessImageNameW, GetFileVersionInfoW, and EnumWindows.
                            // These are one-time costs per session lifetime, not per-poll.
                            //
                            // An empty display name does not count as resolved.
                            // Applications commonly create their audio session
                            // before their main window exists, and caching that
                            // first failed lookup left them nameless for the
                            // rest of the session's life.
                            let cached_names = self
                                .sessions
                                .get(&session_id)
                                .filter(|cached| !cached.display_name.is_empty())
                                .map(|cached| (cached.process_name.clone(), cached.display_name.clone()));

                            let (process_name, friendly_display_name) = if let Some(names) = cached_names {
                                names
                            } else {
                                // New session: resolve names via the full lookup chain.
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
                                let (proc_name, executable_path) = get_process_name(process_id);

                                // Determine the best display name from multiple sources:
                                // 1. Version resource (FileDescription → ProductName)
                                // 2. COM session display name (set at runtime via SetDisplayName)
                                // 3. Process window title (what the Windows Volume Mixer uses as fallback)
                                // 4. Empty string → frontend formats the process name
                                let friendly = {
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
                                        display_name
                                    } else {
                                        get_window_title(process_id).unwrap_or_default()
                                    }
                                };

                                (proc_name, friendly)
                            };

                            // Get volume control
                            if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                let volume = simple_volume.GetMasterVolume().unwrap_or(1.0);
                                let is_muted = simple_volume.GetMute().unwrap_or(BOOL(0)).as_bool();

                                volume_cache
                                    .entry(process_id)
                                    .or_default()
                                    .push(simple_volume);

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

            self.volume_cache = volume_cache;

            // Drop everything that is no longer live. This is the real bound on
            // the cache: it can never hold more than the machine currently has
            // open, so there is nothing further to prune.
            self.sessions.retain(|id, _| live_session_ids.contains(id));

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

    /// Run `call` against every cached volume interface owned by the same
    /// process as `session_id`, returning how many succeeded.
    ///
    /// Games like MSFS create several sessions; controlling only one of them
    /// leaves the rest at their old level, so every interface for the process
    /// is driven together.
    fn apply_to_process<F>(&self, session_id: &str, mut call: F) -> usize
    where
        F: FnMut(&ISimpleAudioVolume) -> windows::core::Result<()>,
    {
        let Some(process_id) = self.sessions.get(session_id).map(|s| s.process_id) else {
            return 0;
        };
        let Some(interfaces) = self.volume_cache.get(&process_id) else {
            return 0;
        };

        let mut updated = 0;
        for interface in interfaces {
            match call(interface) {
                Ok(()) => updated += 1,
                // A session that has ended since the last enumeration fails
                // here. The caller re-enumerates and retries rather than
                // treating one dead interface as a failed write.
                Err(error) => tracing::debug!("[Audio] Session call failed: {}", error),
            }
        }
        updated
    }

    /// Set volume for all audio sessions belonging to the same process as the target session.
    fn set_session_volume(&mut self, session_id: &str, volume: f32) -> std::result::Result<(), String> {
        let volume = volume.clamp(0.0, 1.0);
        let apply = |v: &ISimpleAudioVolume| unsafe { v.SetMasterVolume(volume, std::ptr::null()) };

        let mut updated = self.apply_to_process(session_id, apply);
        if updated == 0 {
            // Either the session appeared after the last enumeration, or its
            // interfaces were invalidated by a device change.
            self.enumerate_sessions()?;
            updated = self.apply_to_process(session_id, apply);
        }

        if updated == 0 {
            return Err(format!("No sessions updated for: {}", session_id));
        }

        if let Some(process_id) = self.sessions.get(session_id).map(|s| s.process_id) {
            for session in self.sessions.values_mut() {
                if session.process_id == process_id {
                    session.volume = volume;
                }
            }
        }
        Ok(())
    }

    /// Mute or unmute all audio sessions belonging to the same process as the target session.
    fn set_session_mute(&mut self, session_id: &str, muted: bool) -> std::result::Result<(), String> {
        let apply = |v: &ISimpleAudioVolume| unsafe { v.SetMute(BOOL(muted as i32), std::ptr::null()) };

        let mut updated = self.apply_to_process(session_id, apply);
        if updated == 0 {
            self.enumerate_sessions()?;
            updated = self.apply_to_process(session_id, apply);
        }

        if updated == 0 {
            return Err(format!("No sessions updated for: {}", session_id));
        }

        if let Some(process_id) = self.sessions.get(session_id).map(|s| s.process_id) {
            for session in self.sessions.values_mut() {
                if session.process_id == process_id {
                    session.is_muted = muted;
                }
            }
        }
        Ok(())
    }


    /// Enumerates active audio sessions and emits an `audio-state-updated` Tauri event
    /// to the frontend only when the session list has changed since the last emission.
    /// This is the push-notification mechanism that replaces frontend polling.
    fn emit_if_changed(&mut self) {
        match self.enumerate_sessions() {
            Ok(sessions) => {
                if sessions != self.last_emitted_sessions {
                    if let Err(e) = self.app_handle.emit("audio-state-updated", &sessions) {
                        tracing::warn!("[Audio] Failed to emit audio-state-updated: {}", e);
                    }
                    self.last_emitted_sessions = sessions;
                }
            }
            Err(e) => tracing::warn!("[Audio] emit_if_changed: enumerate_sessions failed: {}", e),
        }
    }
}

#[cfg(windows)]
impl AudioManager {
    /// Explicit cleanup method for proper resource management.
    ///
    /// This manager registers no COM callbacks (see `rebuild_com_cache`), so
    /// dropping the cached interfaces is all that is required — the endpoint
    /// notification registration belongs to the MTA notification thread and is
    /// unregistered there.
    fn cleanup(&mut self) {
        tracing::info!("[Audio] Cleaning up audio manager resources...");

        // Drop cached COM objects
        self.cached_endpoint_volume = None;
        self.cached_device = None;
        self.cached_enumerator = None;

        // Clear internal caches
        self.sessions.clear();
        self.sessions.shrink_to_fit();
        self.volume_cache.clear();
        self.volume_cache.shrink_to_fit();

        self.enumerate_calls = 0;
        self.last_logged_counts = None;
        // Reset so that, if the thread keeps running after an explicit Cleanup
        // command, the next topology check treats the cache as stale and
        // rebuilds it rather than failing every call.
        self.current_device_id = String::new();

        tracing::info!("[Audio] Audio manager cleanup complete");
    }
}

// CoUninitialize is called explicitly in the audio thread's run loop on exit,
// so Drop does not need to handle it.

// ─────────────────────────────────────────────────────────────────────────────
// IMMNotificationClient — Notification Thread
// ─────────────────────────────────────────────────────────────────────────────

/// Spawns a dedicated MTA COM thread whose sole purpose is to keep an
/// `IMMNotificationClient` registration alive and deliver audio-topology
/// change events to the audio COM thread via a shared atomic flag.
///
/// Using a separate MTA thread avoids the need for a Win32 message pump on
/// the STA audio COM thread — MTA callbacks are delivered directly on the
/// MMDevice notification thread rather than being marshalled through a pump.
///
/// # Shutdown
/// Signal `shutdown_event` with `SetEvent` and then join the returned handle
/// for a clean exit.
#[cfg(windows)]
fn spawn_notification_thread(
    topology_changed_flag: Arc<AtomicBool>,
    shutdown_event: isize,
) -> std::result::Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("audio-notify".to_string())
        .spawn(move || {
            unsafe {
                // MTA apartment: callbacks are delivered directly on the
                // MMDevice notification thread — no message pump required.
                if CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_err() {
                    tracing::error!("[AudioNotify] CoInitializeEx failed");
                    return;
                }
            }

            let result = run_notification_loop(topology_changed_flag, shutdown_event);
            if let Err(e) = result {
                tracing::error!("[AudioNotify] Notification loop error: {}", e);
            }

            unsafe { CoUninitialize(); }
            tracing::info!("[AudioNotify] Notification thread exited");
        })
        .map_err(|e| format!("Failed to spawn notification thread: {}", e))
}

/// Inner loop for the notification thread. Registers an IMMNotificationClient
/// with the device enumerator and waits until the shutdown event is signalled.
#[cfg(windows)]
fn run_notification_loop(
    topology_changed_flag: Arc<AtomicBool>,
    shutdown_event: isize,
) -> std::result::Result<(), String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

        let callback: IMMNotificationClient =
            DeviceChangeCallback { flag: topology_changed_flag }.into();

        enumerator
            .RegisterEndpointNotificationCallback(&callback)
            .map_err(|e: Error| format!("Failed to register notification callback: {}", e))?;

        tracing::info!("[AudioNotify] IMMNotificationClient registered — waiting for events");

        // Block until the shutdown event is signalled. Zero CPU usage while idle.
        WaitForSingleObject(
            HANDLE(shutdown_event as *mut std::ffi::c_void),
            INFINITE,
        );

        enumerator
            .UnregisterEndpointNotificationCallback(&callback)
            .ok();

        tracing::info!("[AudioNotify] IMMNotificationClient unregistered");
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dedicated Audio Thread
// ─────────────────────────────────────────────────────────────────────────────

/// Global handle to the audio thread, set once by init_audio_manager.
static AUDIO_THREAD_HANDLE: Mutex<Option<AudioThreadHandle>> = Mutex::new(None);

/// Spawn the dedicated audio thread. CoInitializeEx is called at the top of
/// the thread and CoUninitialize when it exits. The thread processes commands
/// from the mpsc channel, also waking periodically to check the topology-changed
/// flag and run the safety-net session enumeration.
#[cfg(windows)]
fn spawn_audio_thread(app_handle: tauri::AppHandle) -> std::result::Result<AudioThreadHandle, String> {
    let (tx, rx) = mpsc::channel::<AudioCommand>();

    // Create the shared flag that the notification thread sets on topology changes
    let topology_changed_flag = Arc::new(AtomicBool::new(false));

    // Create the Win32 event used to signal the notification thread to shut down.
    // Manual-reset event, initially unsignalled.
    let notification_shutdown_event: isize = unsafe {
        let h = CreateEventW(None, true, false, None)
            .map_err(|e| format!("Failed to create notification shutdown event: {}", e))?;
        h.0 as isize
    };

    // Spawn the notification thread — it registers IMMNotificationClient on a
    // dedicated MTA thread so callbacks arrive without needing a message pump.
    let notification_thread = spawn_notification_thread(
        topology_changed_flag.clone(),
        notification_shutdown_event,
    )?;

    // Signals that the audio thread has finished its COM teardown.
    let (done_tx, done_rx) = mpsc::channel::<()>();

    let audio_thread = std::thread::Builder::new()
        .name("audio-com".to_string())
        .spawn(move || {
            // Initialise COM on this dedicated thread using a single-threaded apartment
            unsafe {
                if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
                    tracing::error!("[Audio] CoInitializeEx failed on audio thread: {}", e);
                    while let Ok(cmd) = rx.try_recv() {
                        reply_error(cmd, format!("COM init failed: {}", e));
                    }
                    let _ = done_tx.send(());
                    return;
                }
            }

            let manager_result = AudioManager::new(app_handle);
            let mut manager = match manager_result {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("[Audio] AudioManager::new() failed: {}", e);
                    while let Ok(cmd) = rx.try_recv() {
                        reply_error(cmd, e.clone());
                    }
                    unsafe { CoUninitialize(); }
                    let _ = done_tx.send(());
                    return;
                }
            };

            // Emit the initial session list immediately so the frontend has data on startup
            manager.emit_if_changed();

            tracing::info!("[Audio] Dedicated audio thread running");

            // Proactive safety-net timer: catches external volume changes not signalled by COM callbacks
            let mut last_safety_net_check = Instant::now();

            // Event-driven main loop: wakes on commands OR on the flag-check interval.
            // The flag-check interval is short (FLAG_CHECK_INTERVAL) to ensure COM notifications
            // (device add/remove/default change) are picked up promptly.
            loop {
                match rx.recv_timeout(FLAG_CHECK_INTERVAL) {
                    Ok(cmd) => {
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
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // Normal wake — check notification flags and safety-net timer below
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        tracing::warn!("[Audio] Command channel disconnected — exiting");
                        break;
                    }
                }

                // Check whether the IMMNotificationClient has signalled a topology change.
                // The flag is set by the notification thread on device add/remove/default change.
                if topology_changed_flag.swap(false, Ordering::AcqRel) {
                    tracing::debug!("[Audio] Topology change detected — rebuilding COM cache");
                    // check_device_changed rebuilds the COM cache only when the default
                    // render endpoint has actually changed, avoiding unnecessary work.
                    if let Err(e) = manager.check_device_changed() {
                        tracing::warn!("[Audio] check_device_changed after topology event: {}", e);
                    }
                    manager.emit_if_changed();
                    last_safety_net_check = Instant::now(); // Reset safety-net timer
                    continue;
                }

                // Safety-net proactive check: runs every SAFETY_NET_INTERVAL (10 s) to catch
                // external volume changes that COM notifications do not cover (e.g. another
                // application adjusting a per-session volume via the Windows Volume Mixer).
                if last_safety_net_check.elapsed() >= SAFETY_NET_INTERVAL {
                    last_safety_net_check = Instant::now();
                    manager.emit_if_changed();
                }
            }

            // Clean up before thread exits
            manager.cleanup();
            unsafe { CoUninitialize(); }
            tracing::info!("[Audio] Dedicated audio thread exited");
            // Announce completion last, so shutdown only stops waiting once COM
            // teardown on this thread is genuinely finished.
            let _ = done_tx.send(());
        })
        .map_err(|e| format!("Failed to spawn audio thread: {}", e))?;

    Ok(AudioThreadHandle {
        sender: tx,
        audio_join_handle: Some(audio_thread),
        audio_done_rx: done_rx,
        notification_shutdown_event,
        notification_join_handle: Some(notification_thread),
    })
}

/// Helper: send an error reply for any command variant when the thread cannot process it.
#[cfg(windows)]
fn reply_error(cmd: AudioCommand, err: String) {
    match cmd {
        AudioCommand::EnumerateSessions { reply } => { let _ = reply.send(Err(err)); }
        AudioCommand::SetSessionVolume { reply, .. } => { let _ = reply.send(Err(err)); }
        AudioCommand::SetSessionMute { reply, .. } => { let _ = reply.send(Err(err)); }
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

/// Initialise the audio manager on a dedicated COM thread.
/// Tauri automatically injects the `AppHandle` when this command is invoked.
#[tauri::command]
pub fn init_audio_manager(app: tauri::AppHandle) -> std::result::Result<String, String> {
    let mut lock = AUDIO_THREAD_HANDLE
        .lock()
        .map_err(|e| format!("Failed to lock audio thread handle: {}", e))?;

    // Guard against double-initialisation (e.g. rapid frontend retries).
    // Without this, a second call would replace the first sender, causing the
    // running thread's rx to close and the thread to exit, briefly leaving two
    // COM threads alive simultaneously.
    if lock.is_some() {
        tracing::info!("[Audio] Audio manager already initialised, skipping");
        return Ok("Audio manager already initialised".to_string());
    }

    tracing::info!("[Audio] Spawning dedicated audio thread...");
    let handle = spawn_audio_thread(app)?;
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

/// Send a shutdown signal to the audio thread and the notification thread, then
/// wait for both to finish.
///
/// Joining matters: `app.exit(0)` calls `ExitProcess`, which suspends every
/// other thread at an arbitrary instruction. A thread suspended inside a COM
/// call (or holding the loader lock) during teardown is a classic source of an
/// access violation at exit, so both threads are given a bounded window to
/// unwind cleanly first.
pub fn shutdown_audio_thread() {
    if let Ok(mut lock) = AUDIO_THREAD_HANDLE.lock() {
        if let Some(mut handle) = lock.take() {
            // Signal the audio COM thread to shut down
            let _ = handle.sender.send(AudioCommand::Shutdown);
            tracing::info!("[Audio] Shutdown signal sent to audio thread");

            // Wait for the audio thread to finish its COM teardown. A timeout
            // means it is wedged in a COM call — skip the join rather than
            // hanging the whole quit sequence.
            match handle.audio_done_rx.recv_timeout(THREAD_SHUTDOWN_TIMEOUT) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(thread) = handle.audio_join_handle.take() {
                        if let Err(e) = thread.join() {
                            tracing::error!("[Audio] Failed to join audio thread: {:?}", e);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!(
                        "[Audio] Audio thread did not finish within {:?} — not joining",
                        THREAD_SHUTDOWN_TIMEOUT
                    );
                }
            }

            // Signal the notification thread to shut down and join it cleanly
            #[cfg(windows)]
            if handle.notification_shutdown_event != 0 {
                unsafe {
                    SetEvent(
                        HANDLE(handle.notification_shutdown_event as *mut std::ffi::c_void)
                    ).ok();
                }
                if let Some(thread) = handle.notification_join_handle {
                    if let Err(e) = thread.join() {
                        tracing::error!("[Audio] Failed to join notification thread: {:?}", e);
                    }
                }
                // Close the Win32 event handle
                unsafe {
                    CloseHandle(
                        HANDLE(handle.notification_shutdown_event as *mut std::ffi::c_void)
                    ).ok();
                }
            }
        }
    }
}
