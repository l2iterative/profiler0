#![allow(unused_imports)]
pub use self::inner::*;

#[macro_use]
#[cfg(feature = "print-trace")]
pub mod inner {
    // print-trace requires std, so these imports are well-defined
    pub use std::{
        format, println,
        string::{String, ToString},
        sync::Mutex
    };
    use std::collections::BTreeMap;

    lazy_static::lazy_static! {
        pub static ref NUM_INDENT: Mutex<usize> = Mutex::new(0);
        pub static ref REPORTS: Mutex<Vec<(&'static str, usize, usize)>> = Mutex::new(vec![]);
    }

    pub const PAD_CHAR: &str = "····";

    pub struct TimerInfo {
        pub msg: &'static str,
        pub cycle: usize,
    }

    #[macro_export]
    macro_rules! start_timer {
        ($msg:expr) => {{
            use $crate::perf_trace::inner::NUM_INDENT;
            use risc0_zkvm::guest::env;

            (*NUM_INDENT.lock().unwrap()) += 1;
            $crate::perf_trace::TimerInfo {
                msg: $msg,
                cycle: env::get_cycle_count(),
            }
        }};
    }

    #[macro_export]
    macro_rules! end_timer {
        ($time:expr) => {{
            use $crate::perf_trace::inner::{
                REPORTS, NUM_INDENT,
            };

            (*NUM_INDENT.lock().unwrap()) -= 1;
            (*REPORTS.lock().unwrap()).push(($time.msg, (*NUM_INDENT.lock().unwrap()), env::get_cycle_count() - ($time.cycle)));
            (*REPORTS.lock().unwrap()).push(($time.msg, (*NUM_INDENT.lock().unwrap()), env::get_cycle_count() - ($time.cycle)));
        }};
    }

    pub fn compute_indent(indent_amount: usize) -> String {
        let mut indent = String::new();
        for _ in 0..indent_amount {
            indent.push_str(&PAD_CHAR);
        }
        if indent_amount != 0 {
            indent.push_str(" ");
        }
        indent
    }

    pub fn print_trace() {
        let mut output: BTreeMap<usize, String> = BTreeMap::new();
        let mut cur_level = 0;

        let reports = REPORTS.lock().unwrap();
        for report in reports.iter() {
            if report.1 >= cur_level {
                cur_level = report.1;
                let mut cur_string = output.get(&cur_level).cloned().unwrap_or_default();
                cur_string += &format!("{}{}: {}\n", compute_indent(cur_level), report.0, report.2);
                output.insert(cur_level, cur_string);
            } else if report.1 < cur_level {
                let tmp_string = output.get(&cur_level).cloned().unwrap_or_default();
                output.insert(cur_level, "".to_string());

                cur_level = report.1;

                let mut cur_string = output.get(&cur_level).cloned().unwrap_or_default();
                cur_string += &format!("{}{}: {}\n{}", compute_indent(cur_level), report.0, report.2, tmp_string);
                output.insert(cur_level, cur_string);
            }
        }

        println!("{}", output.get(&0).cloned().unwrap_or_default());
    }
}

#[macro_use]
#[cfg(not(feature = "print-trace"))]
mod inner {
    pub struct TimerInfo;

    #[macro_export]
    macro_rules! start_timer {
        ($msg:expr) => {{
            let _ = $msg;
            $crate::perf_trace::TimerInfo
        }};
    }

    #[macro_export]
    macro_rules! end_timer {
        ($time:expr, $msg:expr) => {
            let _ = $msg;
            let _ = $time;
        };
        ($time:expr) => {
            let _ = $time;
        };
    }
}
