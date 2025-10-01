use std::sync::Mutex;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[cfg(windows)]
use windows::{
    core::*,
    Win32::System::Com::*,
    Win32::Media::Audio::*,
    Win32::Foundation::*,
    Win32::System::Threading::*,
};

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
}

#[cfg(windows)]
/// Get the executable name from a process ID
fn get_process_name(process_id: u32) -> String {
    if process_id == 0 {
        return "System".to_string();
    }

    unsafe {
        // Open process with query limited information rights
        let process_handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            process_id,
        );

        match process_handle {
            Ok(handle) => {
                // Buffer for the executable path
                let mut buffer = vec![0u16; 260]; // MAX_PATH
                let mut size = buffer.len() as u32;

                // Get the full executable path
                let result = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut size,
                );

                let _ = CloseHandle(handle);

                if result.is_ok() && size > 0 {
                    // Convert to String
                    let path = String::from_utf16_lossy(&buffer[0..size as usize]);
                    
                    // Extract just the filename from the full path
                    if let Some(filename) = path.split('\\').last() {
                        return filename.to_string();
                    }
                    
                    return path;
                }
            }
            Err(_) => {}
        }
    }

    // Fallback if we can't get the process name
    format!("Process {}", process_id)
}

#[cfg(windows)]
impl AudioManager {
    /// Create a new audio manager instance
    pub fn new() -> std::result::Result<Self, String> {
        // Initialize COM for this thread
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e: Error| format!("Failed to initialize COM: {}", e))?;
        }
        
        Ok(Self {
            sessions: HashMap::new(),
        })
    }

    /// Enumerate all active audio sessions
    pub fn enumerate_sessions(&mut self) -> std::result::Result<Vec<AudioSession>, String> {
        unsafe {
            // Create device enumerator
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            // Get default audio endpoint
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e: Error| format!("Failed to get default audio endpoint: {}", e))?;

            // Get audio session manager
            let session_manager: IAudioSessionManager2 = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e: Error| format!("Failed to activate session manager: {}", e))?;

            // Get session enumerator
            let session_enum = session_manager
                .GetSessionEnumerator()
                .map_err(|e: Error| format!("Failed to get session enumerator: {}", e))?;

            let count = session_enum
                .GetCount()
                .map_err(|e: Error| format!("Failed to get session count: {}", e))?;

            let mut sessions = Vec::new();

            for i in 0..count {
                if let Ok(session_control) = session_enum.GetSession(i) {
                    if let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() {
                        // Get session details
                        let process_id = session_control2
                            .GetProcessId()
                            .unwrap_or(0);

                        let session_id = session_control2
                            .GetSessionInstanceIdentifier()
                            .ok()
                            .and_then(|s| s.to_string().ok())
                            .unwrap_or_else(|| format!("session_{}", i));

                        let display_name = session_control2
                            .GetDisplayName()
                            .ok()
                            .and_then(|s| s.to_string().ok())
                            .unwrap_or_else(|| format!("Process {}", process_id));

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

                            sessions.push(session.clone());
                            self.sessions.insert(session_id, session);
                        }
                    }
                }
            }

            eprintln!("[Audio] Found {} active audio sessions", sessions.len());
            Ok(sessions)
        }
    }

    /// Set volume for a specific session
    pub fn set_session_volume(&mut self, session_id: &str, volume: f32) -> std::result::Result<(), String> {
        let volume = volume.clamp(0.0, 1.0);
        
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e: Error| format!("Failed to get default audio endpoint: {}", e))?;

            let session_manager: IAudioSessionManager2 = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e: Error| format!("Failed to activate session manager: {}", e))?;

            let session_enum = session_manager
                .GetSessionEnumerator()
                .map_err(|e: Error| format!("Failed to get session enumerator: {}", e))?;

            let count = session_enum.GetCount().unwrap_or(0);

            for i in 0..count {
                if let Ok(session_control) = session_enum.GetSession(i) {
                    if let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() {
                        let current_id = session_control2
                            .GetSessionInstanceIdentifier()
                            .ok()
                            .and_then(|s| s.to_string().ok())
                            .unwrap_or_default();

                        if current_id == session_id {
                            if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                simple_volume
                                    .SetMasterVolume(volume, std::ptr::null())
                                    .map_err(|e: Error| format!("Failed to set volume: {}", e))?;
                                
                                // Update cache
                                if let Some(session) = self.sessions.get_mut(session_id) {
                                    session.volume = volume;
                                }
                                
                                eprintln!("[Audio] Set volume for {} to {:.2}", session_id, volume);
                                return Ok(());
                            }
                        }
                    }
                }
            }

            Err(format!("Session not found: {}", session_id))
        }
    }

    /// Mute or unmute a specific session
    pub fn set_session_mute(&mut self, session_id: &str, muted: bool) -> std::result::Result<(), String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e: Error| format!("Failed to create device enumerator: {}", e))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e: Error| format!("Failed to get default audio endpoint: {}", e))?;

            let session_manager: IAudioSessionManager2 = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e: Error| format!("Failed to activate session manager: {}", e))?;

            let session_enum = session_manager
                .GetSessionEnumerator()
                .map_err(|e: Error| format!("Failed to get session enumerator: {}", e))?;

            let count = session_enum.GetCount().unwrap_or(0);

            for i in 0..count {
                if let Ok(session_control) = session_enum.GetSession(i) {
                    if let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() {
                        let current_id = session_control2
                            .GetSessionInstanceIdentifier()
                            .ok()
                            .and_then(|s| s.to_string().ok())
                            .unwrap_or_default();

                        if current_id == session_id {
                            if let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>() {
                                simple_volume
                                    .SetMute(BOOL(muted as i32), std::ptr::null())
                                    .map_err(|e: Error| format!("Failed to set mute: {}", e))?;
                                
                                // Update cache
                                if let Some(session) = self.sessions.get_mut(session_id) {
                                    session.is_muted = muted;
                                }
                                
                                eprintln!("[Audio] Set mute for {} to {}", session_id, muted);
                                return Ok(());
                            }
                        }
                    }
                }
            }

            Err(format!("Session not found: {}", session_id))
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

impl Drop for AudioManager {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            CoUninitialize();
        }
    }
}

// Global audio manager instance
static AUDIO_MANAGER: Mutex<Option<AudioManager>> = Mutex::new(None);

/// Initialize the audio manager
#[tauri::command]
pub fn init_audio_manager() -> std::result::Result<String, String> {
    let manager = AudioManager::new()?;
    
    let mut lock = AUDIO_MANAGER
        .lock()
        .map_err(|e| format!("Failed to lock audio manager mutex: {}", e))?;
    
    *lock = Some(manager);
    
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
