//! macOS: Quartz Event Services (CoreGraphics) FFI. No third-party deps.
//!
//! Note: macOS may require this binary to be granted "Accessibility" or
//! "Input Monitoring" permission under System Settings → Privacy & Security
//! before synthesized events take effect, depending on macOS version.

use std::os::raw::{c_double, c_void};

#[repr(C)]
#[derive(Copy, Clone)]
struct CGPoint {
    x: c_double,
    y: c_double,
}

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;

const KCG_HID_EVENT_TAP: u32 = 0;
const KCG_EVENT_MOUSE_MOVED: u32 = 5;
const KCG_MOUSE_BUTTON_LEFT: u32 = 0;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut c_void);
}

pub struct Mouse;

impl Mouse {
    pub fn new() -> Result<Self, String> {
        Ok(Mouse)
    }

    pub fn move_relative(&self, dx: i32, dy: i32) -> Result<(), String> {
        unsafe {
            let probe = CGEventCreate(std::ptr::null_mut());
            if probe.is_null() {
                return Err("CGEventCreate returned null".into());
            }
            let pt = CGEventGetLocation(probe);
            CFRelease(probe);

            let target = CGPoint {
                x: pt.x + dx as c_double,
                y: pt.y + dy as c_double,
            };
            let evt = CGEventCreateMouseEvent(
                std::ptr::null_mut(),
                KCG_EVENT_MOUSE_MOVED,
                target,
                KCG_MOUSE_BUTTON_LEFT,
            );
            if evt.is_null() {
                return Err("CGEventCreateMouseEvent returned null".into());
            }
            CGEventPost(KCG_HID_EVENT_TAP, evt);
            CFRelease(evt);
        }
        Ok(())
    }
}
