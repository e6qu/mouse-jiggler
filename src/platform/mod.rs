// Platform-specific input drivers. Each module exposes a `Mouse` type
// with `Mouse::new() -> Result<Self, String>` and
// `Mouse::move_relative(&self, dx: i32, dy: i32) -> Result<(), String>`.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Mouse;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::Mouse;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Mouse;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
compile_error!("unsupported target_os: only macos, windows, and linux are supported");
