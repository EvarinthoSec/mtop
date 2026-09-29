//! Process signal delivery, behind a testable trait.

use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Term,
    Kill,
    Int,
    Hup,
    /// Any other POSIX signal number (1..=64), used by the signal picker.
    Raw(i32),
}

impl Signal {
    pub fn number(self) -> i32 {
        match self {
            Signal::Hup => 1,
            Signal::Int => 2,
            Signal::Kill => 9,
            Signal::Term => 15,
            Signal::Raw(n) => n,
        }
    }
    pub fn label(self) -> String {
        match self {
            Signal::Term => "SIGTERM".to_owned(),
            Signal::Kill => "SIGKILL".to_owned(),
            Signal::Int => "SIGINT".to_owned(),
            Signal::Hup => "SIGHUP".to_owned(),
            Signal::Raw(n) => format!("SIG{n}"),
        }
    }
    /// Build a Signal from a POSIX number, folding known numbers into their
    /// named variant. Valid range is 1..=64; anything else returns None.
    pub fn from_number(n: i32) -> Option<Signal> {
        if !(1..=64).contains(&n) {
            return None;
        }
        Some(match n {
            1 => Signal::Hup,
            2 => Signal::Int,
            9 => Signal::Kill,
            15 => Signal::Term,
            other => Signal::Raw(other),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalError;

pub trait ProcessController: Send {
    fn send(&self, pid: u32, sig: Signal) -> Result<(), SignalError>;
    /// Set the scheduling nice value (-20 highest .. 19 lowest priority).
    /// Lowering nice below the current value needs root on macOS/Linux.
    fn renice(&self, pid: u32, nice: i32) -> Result<(), SignalError>;
}

/// Test controller — records requests, never touches the OS.
#[derive(Default)]
pub struct RecordingController {
    sent: Mutex<Vec<(u32, Signal)>>,
    reniced: Mutex<Vec<(u32, i32)>>,
}

impl RecordingController {
    pub fn sent(&self) -> Vec<(u32, Signal)> {
        self.sent.lock().unwrap().clone()
    }
    pub fn reniced(&self) -> Vec<(u32, i32)> {
        self.reniced.lock().unwrap().clone()
    }
}

impl ProcessController for RecordingController {
    fn send(&self, pid: u32, sig: Signal) -> Result<(), SignalError> {
        self.sent.lock().unwrap().push((pid, sig));
        Ok(())
    }
    fn renice(&self, pid: u32, nice: i32) -> Result<(), SignalError> {
        self.reniced.lock().unwrap().push((pid, nice));
        Ok(())
    }
}
