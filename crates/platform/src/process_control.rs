//! Native process-control adapters.

pub use mtop_core::process_control::{ProcessController, RecordingController, Signal, SignalError};

/// Native controller retained under the historical `LibcController` name.
pub struct NativeProcessController;

/// Backwards-compatible name for the platform-native controller.
pub use self::NativeProcessController as LibcController;

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsPriorityBand {
    AboveNormal,
    Normal,
    BelowNormal,
    Idle,
}

#[cfg(any(windows, test))]
fn windows_priority_band(nice: i32) -> Option<WindowsPriorityBand> {
    Some(match nice {
        // Avoid Windows HIGH_PRIORITY_CLASS: it can starve system work.
        -20..=-1 => WindowsPriorityBand::AboveNormal,
        0..=5 => WindowsPriorityBand::Normal,
        6..=14 => WindowsPriorityBand::BelowNormal,
        15..=19 => WindowsPriorityBand::Idle,
        _ => return None,
    })
}

#[cfg(unix)]
impl ProcessController for NativeProcessController {
    fn send(&self, pid: u32, sig: Signal) -> Result<(), SignalError> {
        // SAFETY: kill is async-signal-safe; pid and signal are plain integers.
        let rc = unsafe { libc::kill(pid as libc::pid_t, sig.number()) };
        if rc == 0 { Ok(()) } else { Err(SignalError) }
    }

    fn renice(&self, pid: u32, nice: i32) -> Result<(), SignalError> {
        // SAFETY: setpriority takes plain integers and touches no memory.
        let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
        if rc == 0 { Ok(()) } else { Err(SignalError) }
    }
}

#[cfg(windows)]
impl ProcessController for NativeProcessController {
    fn send(&self, pid: u32, sig: Signal) -> Result<(), SignalError> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_TERMINATE, TerminateProcess,
        };

        // Windows has no POSIX signal delivery. Map TERM/KILL to process
        // termination and reject signals that have no safe equivalent.
        if !matches!(sig, Signal::Term | Signal::Kill) {
            return Err(SignalError);
        }
        // SAFETY: the handle is checked and closed after the operation.
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if process.is_null() {
            return Err(SignalError);
        }
        // SAFETY: process is a valid handle opened with PROCESS_TERMINATE.
        let terminated = unsafe { TerminateProcess(process, sig.number() as u32) != 0 };
        // SAFETY: process is the handle returned by OpenProcess above.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(process) };
        if terminated { Ok(()) } else { Err(SignalError) }
    }

    fn renice(&self, pid: u32, nice: i32) -> Result<(), SignalError> {
        use windows_sys::Win32::System::Threading::{
            ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
            NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_SET_INFORMATION, SetPriorityClass,
        };

        let band = windows_priority_band(nice).ok_or(SignalError)?;
        let priority_class = match band {
            WindowsPriorityBand::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            WindowsPriorityBand::Normal => NORMAL_PRIORITY_CLASS,
            WindowsPriorityBand::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            WindowsPriorityBand::Idle => IDLE_PRIORITY_CLASS,
        };
        // SAFETY: the handle is checked and closed after the operation.
        let process = unsafe { OpenProcess(PROCESS_SET_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(SignalError);
        }
        // SAFETY: process is a valid handle opened with PROCESS_SET_INFORMATION.
        let changed = unsafe { SetPriorityClass(process, priority_class) != 0 };
        // SAFETY: process is the handle returned by OpenProcess above.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(process) };
        if changed { Ok(()) } else { Err(SignalError) }
    }
}

#[cfg(not(any(unix, windows)))]
impl ProcessController for NativeProcessController {
    fn send(&self, _pid: u32, _sig: Signal) -> Result<(), SignalError> {
        Err(SignalError)
    }

    fn renice(&self, _pid: u32, _nice: i32) -> Result<(), SignalError> {
        Err(SignalError)
    }
}

#[cfg(test)]
mod windows_priority_tests {
    use super::{WindowsPriorityBand, windows_priority_band};

    #[test]
    fn maps_nice_range_to_windows_priority_bands() {
        assert_eq!(
            windows_priority_band(-20),
            Some(WindowsPriorityBand::AboveNormal)
        );
        assert_eq!(
            windows_priority_band(-1),
            Some(WindowsPriorityBand::AboveNormal)
        );
        assert_eq!(windows_priority_band(0), Some(WindowsPriorityBand::Normal));
        assert_eq!(
            windows_priority_band(8),
            Some(WindowsPriorityBand::BelowNormal)
        );
        assert_eq!(windows_priority_band(19), Some(WindowsPriorityBand::Idle));
    }

    #[test]
    fn rejects_nice_values_outside_posix_range() {
        assert_eq!(windows_priority_band(-21), None);
        assert_eq!(windows_priority_band(20), None);
    }
}
