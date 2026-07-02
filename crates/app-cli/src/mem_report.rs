//! Cross-platform resident-memory footprint reporting.
//!
//! Gated behind the `TELETIPO_MEM_REPORT=1` environment variable so it stays
//! completely out of the way during normal use. When enabled, [`report`] logs
//! the process's resident footprint at named milestones, letting us compare
//! memory before/after each optimization on macOS and Linux with the same
//! workload.
//!
//! Platform sources:
//! - **macOS**: `proc_pid_rusage(.., RUSAGE_INFO_V2, ..)` → `ri_phys_footprint`,
//!   the same figure Activity Monitor shows under "Memory".
//! - **Linux**: `VmRSS` from `/proc/self/status`.
//! - **Other**: returns `None`.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static INIT: std::sync::Once = std::sync::Once::new();

/// Whether `TELETIPO_MEM_REPORT` requests footprint logging. Cached after the
/// first call so we only read the environment once.
fn enabled() -> bool {
    INIT.call_once(|| {
        let on = std::env::var("TELETIPO_MEM_REPORT")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
        ENABLED.store(on, Ordering::Relaxed);
    });
    ENABLED.load(Ordering::Relaxed)
}

/// Resident memory (RSS/actual RAM) in bytes, or `None` if it cannot be determined.
pub(crate) fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `task_info()` reads per-task info from the kernel. We must
        // pass a pointer that the kernel will fill, along with a count of elements.
        // `resident_size` = pages currently in RAM (heap, framebuffer, etc).
        #[allow(unsafe_code, deprecated)]
        unsafe {
            let mut info: libc::mach_task_basic_info = std::mem::zeroed();
            let mut count = (std::mem::size_of::<libc::mach_task_basic_info>()
                / std::mem::size_of::<libc::natural_t>())
                as libc::mach_msg_type_number_t;
            let ret = libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                &mut info as *mut libc::mach_task_basic_info as *mut libc::integer_t,
                &mut count,
            );
            if ret == libc::KERN_SUCCESS {
                Some(info.resident_size as u64)
            } else {
                None
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // `/proc/self/status` reports VmRSS in kibibytes.
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kib * 1024);
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Log the actual resident memory (RAM) at a named milestone.
/// Cheap no-op if disabled. Note: Activity Monitor on macOS shows virtual memory
/// (VSIZE) which includes mmap'd dylibs and assets — actual RAM in use is lower.
pub(crate) fn report(milestone: &str) {
    if !enabled() {
        return;
    }
    match resident_bytes() {
        Some(bytes) => {
            let mib = bytes as f64 / (1024.0 * 1024.0);
            tracing::info!(
                milestone,
                resident_mib = format_args!("{mib:.1}"),
                "mem_report: resident memory (actual RAM in use)"
            );
        }
        None => {
            tracing::info!(milestone, "mem_report: memory unavailable on this platform");
        }
    }
}
