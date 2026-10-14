//! The only Win32 calls MarkDraft makes directly. Kept in one small module
//! so the `unsafe` is easy to audit.
#![allow(unsafe_code)]

use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

/// Tells Explorer (and so newly started terminals) that the user's
/// environment changed, as the System Properties dialog does.
pub fn broadcast_environment_change() {
    let name: Vec<u16> = "Environment\0".encode_utf16().collect();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the
    // call; a null result pointer is allowed.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            name.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            std::ptr::null_mut(),
        );
    }
}

/// Release builds have no console of their own; borrow the terminal's so
/// `markdraft export …` can print progress and errors.
pub fn attach_parent_console() {
    // SAFETY: no pointers; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
