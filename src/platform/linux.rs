//! Linux: X11 (XTest) via runtime dlopen. No build-time X11 dependency.
//!
//! Requires libX11.so.6 and libXtst.so.6 to be installed at runtime
//! (standard on any Xorg desktop; Debian/Ubuntu: libx11-6 + libxtst6).
//!
//! Wayland: not natively supported. Most Wayland sessions ship XWayland,
//! which means this works for X11 clients but may not move the real cursor
//! on a pure Wayland compositor. Run inside an Xorg session for full effect.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_ulong, c_void};

type Display = c_void;

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_LAZY: c_int = 1;

type XOpenDisplayFn = unsafe extern "C" fn(*const c_char) -> *mut Display;
type XFlushFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XCloseDisplayFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XTestFakeRelativeMotionEventFn =
    unsafe extern "C" fn(*mut Display, c_int, c_int, c_ulong) -> c_int;

pub struct Mouse {
    display: *mut Display,
    x_flush: XFlushFn,
    x_close: XCloseDisplayFn,
    xtest_motion: XTestFakeRelativeMotionEventFn,
}

// SAFETY: only used from a single thread in this binary; X11 calls are
// not actually thread-safe without XInitThreads, so do not Send across threads.
// We mark Send/Sync as needed only if multithreading is added later.

impl Mouse {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let xlib = load_lib(&["libX11.so.6", "libX11.so"]).ok_or_else(|| {
                "failed to dlopen libX11 — is Xorg installed? \
                 Wayland sessions: ensure XWayland is present"
                    .to_string()
            })?;
            let xtst = load_lib(&["libXtst.so.6", "libXtst.so"]).ok_or_else(|| {
                "failed to dlopen libXtst — install libxtst \
                 (Debian/Ubuntu: libxtst6, Fedora: libXtst)"
                    .to_string()
            })?;

            let x_open: XOpenDisplayFn = sym(xlib, "XOpenDisplay")?;
            let x_flush: XFlushFn = sym(xlib, "XFlush")?;
            let x_close: XCloseDisplayFn = sym(xlib, "XCloseDisplay")?;
            let xtest_motion: XTestFakeRelativeMotionEventFn =
                sym(xtst, "XTestFakeRelativeMotionEvent")?;

            let display = x_open(std::ptr::null());
            if display.is_null() {
                return Err(
                    "XOpenDisplay returned null — DISPLAY not set or X server unreachable".into(),
                );
            }
            Ok(Mouse {
                display,
                x_flush,
                x_close,
                xtest_motion,
            })
        }
    }

    pub fn move_relative(&self, dx: i32, dy: i32) -> Result<(), String> {
        unsafe {
            (self.xtest_motion)(self.display, dx as c_int, dy as c_int, 0 as c_ulong);
            (self.x_flush)(self.display);
        }
        Ok(())
    }
}

impl Drop for Mouse {
    fn drop(&mut self) {
        unsafe {
            (self.x_close)(self.display);
        }
    }
}

unsafe fn load_lib(names: &[&str]) -> Option<*mut c_void> {
    for name in names {
        if let Ok(cs) = CString::new(*name) {
            let h = dlopen(cs.as_ptr(), RTLD_LAZY);
            if !h.is_null() {
                return Some(h);
            }
        }
    }
    None
}

unsafe fn sym<T>(handle: *mut c_void, name: &str) -> Result<T, String> {
    let cs = CString::new(name).map_err(|_| format!("bad symbol name: {name}"))?;
    let p = dlsym(handle, cs.as_ptr());
    if p.is_null() {
        return Err(format!("dlsym({name}) returned null"));
    }
    // Function pointers are pointer-sized; transmute_copy avoids generic
    // size-check issues that would trip plain transmute.
    Ok(std::mem::transmute_copy::<*mut c_void, T>(&p))
}

