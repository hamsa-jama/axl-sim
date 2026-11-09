//! The state a controller run keeps in the wasmtime `Store`, and the errors that stop a program.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use wasmtime::StoreLimits;

use crate::api::{DeviceKind, LogStream, RobotApi};

/// Why a program was stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKind {
    /// An AXL call came before `axl_init`.
    NotInitialised,
    /// `axl_init` was given an ABI version this AXL does not speak.
    AbiVersion,
    /// A handle that does not exist, or a device of the wrong kind.
    BadDevice,
    /// A bad argument (for example `axl_step(0)`, or a pointer outside memory).
    BadArgument,
    /// The mission has switched this function off.
    DisabledApi,
    /// 2 s without a call to `axl_step`.
    Watchdog,
    /// The simulation could not go on.
    Simulation,
    /// The program called `exit` with a non-zero code.
    Exit,
    /// The program tried to sleep or raise a signal.
    Unsupported,
    /// A WebAssembly trap (out of bounds, unreachable, stack overflow, ...).
    Trap,
    /// Too many plot channels or another refusal from the robot.
    Refused,
}

/// A position in the learner's source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    /// File as recorded in the debug info.
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, if known.
    pub column: Option<u32>,
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.column {
            Some(c) => write!(f, "{}:{}:{}", self.file, self.line, c),
            None => write!(f, "{}:{}", self.file, self.line),
        }
    }
}

/// What stopped the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramError {
    /// Which kind of stop.
    pub kind: StopKind,
    /// The plain message for the learner.
    pub message: String,
    /// Where in the learner's code it happened, if the program has debug info.
    pub location: Option<SourceLocation>,
}

impl std::fmt::Display for ProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(l) = &self.location {
            write!(f, " ({l})")?;
        }
        Ok(())
    }
}

/// The store data of a run.
pub struct HostState<A> {
    /// The robot.
    pub api: A,
    pub(crate) limits: StoreLimits,
    pub(crate) inited: bool,
    pub(crate) disabled: Vec<String>,
    pub(crate) stop: Option<(StopKind, String)>,
    pub(crate) mission_over: bool,
    pub(crate) exited: Option<u32>,
    pub(crate) watchdog_ticks: u64,
    rng: ChaCha8Rng,
    out: Vec<u8>,
    err: Vec<u8>,
}

impl<A: RobotApi> HostState<A> {
    pub(crate) fn new(
        api: A,
        limits: StoreLimits,
        disabled: Vec<String>,
        seed: u64,
        watchdog_ticks: u64,
    ) -> Self {
        Self {
            api,
            limits,
            inited: false,
            disabled,
            stop: None,
            mission_over: false,
            exited: None,
            watchdog_ticks,
            rng: ChaCha8Rng::seed_from_u64(seed),
            out: Vec::new(),
            err: Vec::new(),
        }
    }

    /// Stop the program with a message; the returned error unwinds the guest.
    pub(crate) fn stop<R>(&mut self, kind: StopKind, message: String) -> wasmtime::Result<R> {
        self.stop = Some((kind, message.clone()));
        Err(wasmtime::Error::msg(message))
    }

    /// Entry check of every `axl_*` function except `axl_init`.
    pub(crate) fn enter(&mut self, name: &str) -> wasmtime::Result<()> {
        if !self.inited {
            return self.stop(
                StopKind::NotInitialised,
                "Call axl_init before anything else.".into(),
            );
        }
        if self.disabled.iter().any(|d| d == name) {
            return self.stop(
                StopKind::DisabledApi,
                format!("This mission switches off {name} so you can build your own controller."),
            );
        }
        Ok(())
    }

    /// Check that `dev` exists and is of kind `want`; returns its index.
    pub(crate) fn device(
        &mut self,
        func: &str,
        dev: i32,
        want: DeviceKind,
    ) -> wasmtime::Result<usize> {
        let info = if dev >= 0 {
            self.api.device_info(dev as usize)
        } else {
            None
        };
        let Some((name, kind)) = info else {
            return self.stop(
                StopKind::BadDevice,
                format!(
                    "axl_{func}: device {dev} does not exist. axl_device returns -1 when a name is not found; check for it."
                ),
            );
        };
        if kind != want {
            return self.stop(
                StopKind::BadDevice,
                format!(
                    "axl_{func}: device {dev} ({name}) is {}, not {}.",
                    kind.describe(),
                    want.describe()
                ),
            );
        }
        Ok(dev as usize)
    }

    pub(crate) fn fill_random(&mut self, buf: &mut [u8]) {
        self.rng.fill_bytes(buf);
    }

    /// Collect bytes written to stdout or stderr; whole lines go to the log.
    pub(crate) fn write_log(&mut self, stream: LogStream, data: &[u8]) {
        let buf = match stream {
            LogStream::Stdout => &mut self.out,
            LogStream::Stderr => &mut self.err,
        };
        buf.extend_from_slice(data);
        while let Some(i) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=i).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
            self.api.log(stream, &text);
        }
    }

    /// Send any unfinished lines to the log (at the end of a run).
    pub(crate) fn flush_log(&mut self) {
        for stream in [LogStream::Stdout, LogStream::Stderr] {
            let buf = match stream {
                LogStream::Stdout => std::mem::take(&mut self.out),
                LogStream::Stderr => std::mem::take(&mut self.err),
            };
            if !buf.is_empty() {
                let text = String::from_utf8_lossy(&buf).into_owned();
                self.api.log(stream, &text);
            }
        }
    }

    pub(crate) fn stop_sleep(&mut self) -> wasmtime::Result<i32> {
        self.stop(
            StopKind::Unsupported,
            "Your program tried to sleep. Time only passes when you call axl_step(ms).".into(),
        )
    }

    pub(crate) fn stop_signal(&mut self) -> wasmtime::Result<i32> {
        self.stop(
            StopKind::Unsupported,
            "Your program raised a signal and was stopped.".into(),
        )
    }

    pub(crate) fn exit(&mut self, code: u32) -> wasmtime::Result<()> {
        self.exited = Some(code);
        if code == 0 {
            // A clean exit is the same as `main` returning.
            return Err(wasmtime::Error::msg("exit"));
        }
        self.stop(
            StopKind::Exit,
            format!("Your program exited with code {code}."),
        )
    }
}
