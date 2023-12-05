#![allow(unused_imports)]
pub use self::inner::*;

#[macro_use]
#[cfg(feature = "print-trace")]
pub mod inner {
    #[no_mangle]
    pub static mut TRACE_MSG_CHANNEL: [u32; 128] = [0u32; 128];
    #[no_mangle]
    pub static mut TRACE_MSG_LEN_CHANNEL: u32 = 0;
    #[no_mangle]
    pub static mut TRACE_CYCLE_CHANNEL: u32 = 0;

    #[inline(always)]
    pub fn init_trace_logger() {
        unsafe {
            core::arch::asm!(
                r#"
                nop
                li x0, 0xCDCDCDCD
                la x0, TRACE_MSG_CHANNEL
                la x0, TRACE_MSG_LEN_CHANNEL
                la x0, TRACE_CYCLE_CHANNEL
                nop
            "#
            );
        }
    }

    #[macro_export]
    macro_rules! start_timer {
        ($msg: expr) => {{
            use $crate::cycle_trace::inner::{
                TRACE_CYCLE_CHANNEL, TRACE_MSG_CHANNEL, TRACE_MSG_LEN_CHANNEL,
            };

            extern "C" {
                fn sys_cycle_count() -> usize;
            }

            unsafe {
                let len = $msg.len();
                core::ptr::copy(
                    core::mem::transmute::<*const u8, *const u32>($msg.as_ptr()),
                    TRACE_MSG_CHANNEL.as_mut_ptr(),
                    (len + 3) / 4,
                );
                TRACE_MSG_LEN_CHANNEL = len as u32;
                TRACE_CYCLE_CHANNEL = sys_cycle_count() as u32;
            }
        }};
    }

    #[macro_export]
    macro_rules! end_timer {
        () => {{
            use $crate::cycle_trace::inner::TRACE_CYCLE_CHANNEL;
            extern "C" {
                fn sys_cycle_count() -> usize;
            }
            unsafe {
                TRACE_CYCLE_CHANNEL = sys_cycle_count() as u32;
            }
        }};
    }

    #[macro_export]
    macro_rules! timer {
        ($msg: expr) => {{
            end_timer!();
            start_timer!($msg);
        }};
    }
}

#[macro_use]
#[cfg(not(feature = "print-trace"))]
pub mod inner {
    #[inline(always)]
    pub fn init_trace_logger() {}

    #[macro_export]
    macro_rules! start_timer {
        ($msg: expr) => {{
            let _ = $msg;
        }};
    }

    #[macro_export]
    macro_rules! end_timer {
        () => {{}};
    }
    #[macro_export]
    macro_rules! timer {
        ($msg: expr) => {{
            let _ = $msg;
        }};
    }
}
