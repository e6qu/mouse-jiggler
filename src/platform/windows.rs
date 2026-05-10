//! Windows: SendInput via user32.dll. No third-party deps.

use std::os::raw::{c_int, c_long};

const INPUT_MOUSE: u32 = 0;
const MOUSEEVENTF_MOVE: u32 = 0x0001;

#[repr(C)]
#[derive(Copy, Clone)]
struct MOUSEINPUT {
    dx: c_long,
    dy: c_long,
    mouse_data: u32,
    dw_flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct KEYBDINPUT {
    w_vk: u16,
    w_scan: u16,
    dw_flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct HARDWAREINPUT {
    u_msg: u32,
    w_param_l: u16,
    w_param_h: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
union INPUT_U {
    mi: MOUSEINPUT,
    ki: KEYBDINPUT,
    hi: HARDWAREINPUT,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct INPUT {
    r#type: u32,
    u: INPUT_U,
}

#[link(name = "user32")]
extern "system" {
    fn SendInput(c_inputs: u32, p_inputs: *const INPUT, cb_size: c_int) -> u32;
}

pub struct Mouse;

impl Mouse {
    pub fn new() -> Result<Self, String> {
        Ok(Mouse)
    }

    pub fn move_relative(&self, dx: i32, dy: i32) -> Result<(), String> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            u: INPUT_U {
                mi: MOUSEINPUT {
                    dx: dx as c_long,
                    dy: dy as c_long,
                    mouse_data: 0,
                    dw_flags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dw_extra_info: 0,
                },
            },
        };
        let n = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as c_int) };
        if n != 1 {
            return Err("SendInput failed".into());
        }
        Ok(())
    }
}
