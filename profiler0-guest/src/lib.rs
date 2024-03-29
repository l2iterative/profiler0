#![allow(unused_imports)]
pub use self::inner::*;

#[macro_use]
#[cfg(all(target_os = "zkvm", feature = "print-trace"))]
pub mod inner {
    #[repr(align(4))]
    pub struct MsgChannel(pub [u8; 512]);

    #[no_mangle]
    pub static mut TRACE_MSG_CHANNEL: MsgChannel = MsgChannel([0u8; 512]);
    #[no_mangle]
    pub static mut TRACE_MSG_LEN_CHANNEL: u32 = 0;
    #[no_mangle]
    pub static mut TRACE_SIGNAL_CHANNEL: u32 = 0;

    #[inline(always)]
    pub fn init_trace_logger() {
        unsafe {
            core::arch::asm!(
                r#"
                nop
                li x0, 0xCDCDCDCD
                la x0, TRACE_MSG_CHANNEL
                la x0, TRACE_MSG_LEN_CHANNEL
                la x0, TRACE_SIGNAL_CHANNEL
                nop
            "#
            );
        }
    }

    #[macro_export]
    macro_rules! start_timer {
        ($msg: expr) => {{
            unsafe {
                let len = $msg.len();
                core::ptr::copy($msg.as_ptr(), TRACE_MSG_CHANNEL.0.as_mut_ptr(), len);
                // prevent out-of-order execution
                core::arch::asm!(
                    r#"
                        nop
                    "#
                );
                core::ptr::write_volatile((&mut TRACE_MSG_LEN_CHANNEL) as *mut u32, len as u32);
            }
        }};
    }

    #[macro_export]
    macro_rules! stop_timer {
        () => {{
            unsafe {
                core::ptr::write_volatile((&mut TRACE_SIGNAL_CHANNEL) as *mut u32, 0u32);
                core::arch::asm!(
                    r#"
                        nop
                    "#
                );
            }
        }};
    }

    #[macro_export]
    macro_rules! stop_start_timer {
        ($msg: expr) => {{
            stop_timer!();
            start_timer!($msg);
        }};
    }
}

#[macro_use]
#[cfg(any(not(target_os = "zkvm"), not(feature = "print-trace")))]
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
    macro_rules! stop_timer {
        () => {{}};
    }
    #[macro_export]
    macro_rules! stop_start_timer {
        ($msg: expr) => {{
            let _ = $msg;
        }};
    }
}
