// ffi.rs — Rust → Objective-C bridge.
//
// These four C functions are implemented in app_darwin.m. Each mutator copies
// its argument synchronously (into NSData/NSString) and then dispatches the
// actual UI work to the main queue, so callers may invoke them from any thread.
// The raw pointers we pass are only valid for the duration of the synchronous
// call — the safe wrappers below free their allocations immediately on return,
// matching the Go `defer C.free` pattern.

use std::ffi::CString;
use std::os::raw::c_char;

extern "C" {
    fn runApp();
    fn setStatusIcon(bytes: *const std::ffi::c_void, len: usize);
    fn setStatusTitle(title: *const c_char);
    fn setStatusTooltip(tooltip: *const c_char);
    fn updatePanelState(state_json: *const c_char);
}

/// Enter the NSApplication run loop. Blocks forever (main thread).
pub fn run_app() {
    unsafe { runApp() }
}

/// Push a template PNG to the status bar icon. `bytes` is copied by Obj-C.
pub fn set_status_icon(bytes: &[u8]) {
    unsafe { setStatusIcon(bytes.as_ptr() as *const _, bytes.len()) }
}

/// Set the short text shown next to the status icon (e.g. "84%").
pub fn set_status_title(title: &str) {
    let c = CString::new(title).unwrap_or_else(|_| CString::new("").unwrap());
    unsafe { setStatusTitle(c.as_ptr()) }
}

/// Set the status bar tooltip. The string is copied by Obj-C.
pub fn set_status_tooltip(tooltip: &str) {
    let c = CString::new(tooltip).unwrap_or_else(|_| CString::new("").unwrap());
    unsafe { setStatusTooltip(c.as_ptr()) }
    // c dropped here — after the synchronous copy above.
}

/// Push the panel state JSON. The string is copied by Obj-C.
pub fn update_panel_state(state_json: &str) {
    let c = CString::new(state_json).unwrap_or_else(|_| CString::new("").unwrap());
    unsafe { updatePanelState(c.as_ptr()) }
}
