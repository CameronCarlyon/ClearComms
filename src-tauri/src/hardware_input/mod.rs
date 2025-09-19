// Placeholder for future integration code
pub fn init() {
    println!("Module initialised");
}
// Link to the DirectInput 8 library (part of Windows)
use std::ffi::c_void;
use windows::core::GUID;

// Reuse our earlier declaration of DirectInput8Create
#[link(name = "dinput8")]
extern "system" {
    fn DirectInput8Create(
        hinst: isize,
        dwVersion: u32,
        riidltf: *const GUID,
        ppvOut: *mut *mut c_void,
        punkOuter: *mut c_void,
    ) -> i32;
}

// The interface ID for IDirectInput8
// (from Windows SDK headers; fixed GUID value)
const IID_IDIRECTINPUT8W: GUID = GUID::from_u128(0xBF798031_483A_4DA2_A5A5_5B7A5C5E0C77);

/// Initialise DirectInput and return whether it succeeded
#[tauri::command]
pub fn init_direct_input() -> Result<String, String> {
    unsafe {
        // Prepare a null pointer to hold the created interface
        let mut direct_input_ptr: *mut c_void = std::ptr::null_mut();

        // Call DirectInput8Create
        let hr = DirectInput8Create(
            0,                        // hInstance (0 = current process)
            0x0800,                   // version (0x0800 = DirectX 8.0)
            &IID_IDIRECTINPUT8W,      // which interface we want
            &mut direct_input_ptr,    // where to store the pointer
            std::ptr::null_mut(),     // unused (COM aggregation)
        );

        // HRESULT < 0 means failure
        if hr < 0 {
            Err(format!("DirectInput init failed with HRESULT {:#X}", hr))
        } else {
            Ok(format!("DirectInput initialised! Pointer: {:?}", direct_input_ptr))
        }
    }
}
