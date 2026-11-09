//! axl-host: the wasmtime host that runs a learner's program (SPEC "Controller runtime and robot
//! API"). It owns the sandbox (64 MB of memory, an epoch-based step watchdog, WASI shims with sim
//! time and seeded random), the `axl` ABI and the plain-language errors with source lines.
//!
//! The robot itself is behind the [`RobotApi`] trait, so the same host runs against the real
//! simulation (`axl-sim`) and against recorders in tests.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod abi;
pub mod api;
mod state;
pub mod toolchain;
mod wasi;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use wasmtime::{
    Config, Engine, Instance, Linker, Module, Store, StoreLimitsBuilder, Trap, WasmBacktrace,
    WasmBacktraceDetails,
};

pub use api::{ColorName, DeviceKind, LogStream, MotorCommand, MotorReading, RobotApi, StepStatus};
pub use state::{HostState, ProgramError, SourceLocation, StopKind};

/// The message of the step watchdog (SPEC "Sandbox and limits").
pub const WATCHDOG_MESSAGE: &str =
    "Your program ran for 2 seconds without calling axl_step. A loop may be missing a step.";

/// Limits and options of a run.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Functions the mission switches off, without the `axl_` prefix (for example `motor_speed`).
    pub disabled_api: Vec<String>,
    /// Seed of `random_get`.
    pub seed: u64,
    /// Most linear memory the program may have, bytes (64 MB).
    pub max_memory: usize,
    /// Wall-clock length of one epoch tick (100 ms).
    pub tick: Duration,
    /// Ticks the program may run without calling `axl_step`. 21 ticks of 100 ms trip between
    /// 2.0 and 2.1 s after the last call, because the first tick can come at any moment.
    pub watchdog_ticks: u64,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            disabled_api: Vec::new(),
            seed: 1,
            max_memory: 64 * 1024 * 1024,
            tick: Duration::from_millis(100),
            watchdog_ticks: 21,
        }
    }
}

/// Why a program could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The bytes are not a valid WebAssembly module.
    #[error("This is not a valid WebAssembly program: {0}")]
    Invalid(String),
    /// The program imports something AXL does not provide.
    #[error("This program uses {module}.{name}, which this version of AXL does not have.")]
    UnknownImport {
        /// Import module.
        module: String,
        /// Import name.
        name: String,
    },
    /// The program needs more memory than the sandbox allows, or could not start.
    #[error("{0}")]
    Instantiate(String),
    /// The program has no `_start`.
    #[error(
        "This program has no entry point. Build it as a command for wasm32-wasip1 (a normal main)."
    )]
    NoStart,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// `main` returned (or the program called `exit(0)`).
    Finished,
    /// The program was stopped.
    Stopped(ProgramError),
}

/// Frames from these places are not the learner's code: errors point at the first frame outside.
const SDK_MARKERS: [&str; 7] = [
    "axl.h",
    "axl-robot",
    "axl_robot",
    "wasi-sysroot",
    "/rustc/",
    "rustlib",
    ".cargo/registry",
];

/// A loaded program with its robot.
pub struct Controller<A: RobotApi + 'static> {
    store: Store<HostState<A>>,
    instance: Instance,
    ticker_stop: Arc<AtomicBool>,
}

impl<A: RobotApi + 'static> Controller<A> {
    /// Compile and instantiate a program (`.wasm` bytes, or WebAssembly text) against a robot.
    pub fn new(wasm: &[u8], api: A, cfg: &HostConfig) -> Result<Self, LoadError> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        // DWARF line tables in the learner's module give every error a source file and line.
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
        let engine = Engine::new(&config).map_err(|e| LoadError::Instantiate(e.to_string()))?;
        let module =
            Module::new(&engine, wasm).map_err(|e| LoadError::Invalid(format!("{e:#}")))?;

        let mut linker: Linker<HostState<A>> = Linker::new(&engine);
        abi::add_to_linker(&mut linker).map_err(|e| LoadError::Instantiate(e.to_string()))?;
        wasi::add_to_linker(&mut linker).map_err(|e| LoadError::Instantiate(e.to_string()))?;
        for import in module.imports() {
            let known = match import.module() {
                abi::MODULE => abi::FUNCTIONS.contains(&import.name()),
                // Unknown WASI functions become traps that name themselves.
                "wasi_snapshot_preview1" => true,
                _ => false,
            };
            if !known {
                return Err(LoadError::UnknownImport {
                    module: import.module().to_owned(),
                    name: import.name().to_owned(),
                });
            }
        }
        linker
            .define_unknown_imports_as_traps(&module)
            .map_err(|e| LoadError::Instantiate(e.to_string()))?;

        let limits = StoreLimitsBuilder::new()
            .memory_size(cfg.max_memory)
            .instances(1)
            .build();
        let state = HostState::new(
            api,
            limits,
            cfg.disabled_api.clone(),
            cfg.seed,
            cfg.watchdog_ticks,
        );
        let mut store = Store::new(&engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_epoch_deadline(cfg.watchdog_ticks);
        let instance = linker.instantiate(&mut store, &module).map_err(|e| {
            let text = format!("{e:#}");
            if text.contains("memory") {
                LoadError::Instantiate(
                    "This program asks for more than 64 MB of memory, which is the most AXL allows.".into(),
                )
            } else {
                LoadError::Instantiate(text)
            }
        })?;
        if instance.get_func(&mut store, "_start").is_none() {
            return Err(LoadError::NoStart);
        }

        // The epoch ticker: one tick every 100 ms of wall-clock time while the controller lives.
        let ticker_stop = Arc::new(AtomicBool::new(false));
        {
            let (engine, stop, tick) = (engine.clone(), ticker_stop.clone(), cfg.tick);
            std::thread::Builder::new()
                .name("axl-watchdog".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(tick);
                        engine.increment_epoch();
                    }
                })
                .map_err(|e| LoadError::Instantiate(e.to_string()))?;
        }
        Ok(Self {
            store,
            instance,
            ticker_stop,
        })
    }

    /// The robot.
    pub fn api(&self) -> &A {
        &self.store.data().api
    }

    /// The robot, mutable.
    pub fn api_mut(&mut self) -> &mut A {
        &mut self.store.data_mut().api
    }

    /// Run the program from `_start` until `main` returns or it is stopped. Then the robot's
    /// `program_ended` runs (every motor brakes and the simulation carries on).
    pub fn run(&mut self) -> RunEnd {
        let ticks = self.store.data().watchdog_ticks;
        self.store.set_epoch_deadline(ticks);
        let result = match self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "_start")
        {
            Ok(f) => f.call(&mut self.store, ()),
            Err(e) => Err(e),
        };
        self.store.data_mut().flush_log();
        let end = match result {
            Ok(()) => RunEnd::Finished,
            Err(e) => self.classify(e),
        };
        self.store.data_mut().api.program_ended();
        end
    }

    fn classify(&mut self, e: wasmtime::Error) -> RunEnd {
        let location = source_location(&e);
        let st = self.store.data();
        if st.exited == Some(0) {
            return RunEnd::Finished;
        }
        if let Some((kind, message)) = st.stop.clone() {
            return RunEnd::Stopped(ProgramError {
                kind,
                message,
                location,
            });
        }
        let (kind, message) = match e.downcast_ref::<Trap>() {
            Some(Trap::Interrupt) => (StopKind::Watchdog, WATCHDOG_MESSAGE.to_owned()),
            Some(Trap::StackOverflow) => (
                StopKind::Trap,
                "Your program used up its stack. A function may be calling itself forever.".to_owned(),
            ),
            Some(Trap::MemoryOutOfBounds) => (
                StopKind::Trap,
                "Your program read or wrote outside its memory (a bad pointer or an array index out of range).".to_owned(),
            ),
            Some(Trap::UnreachableCodeReached) => (
                StopKind::Trap,
                "Your program stopped itself (a panic, an assertion or abort).".to_owned(),
            ),
            Some(Trap::IntegerDivisionByZero) => {
                (StopKind::Trap, "Your program divided an integer by zero.".to_owned())
            }
            Some(t) => (StopKind::Trap, format!("Your program crashed: {t}.")),
            None => (StopKind::Trap, format!("Your program could not run: {e:#}")),
        };
        RunEnd::Stopped(ProgramError {
            kind,
            message,
            location,
        })
    }
}

impl<A: RobotApi + 'static> Drop for Controller<A> {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
    }
}

/// The first frame of the backtrace that is the learner's code and has a source line.
fn source_location(e: &wasmtime::Error) -> Option<SourceLocation> {
    let bt = e.downcast_ref::<WasmBacktrace>()?;
    let mut fallback = None;
    for frame in bt.frames() {
        for s in frame.symbols() {
            let (Some(file), Some(line)) = (s.file(), s.line()) else {
                continue;
            };
            let loc = SourceLocation {
                file: file.to_owned(),
                line,
                column: s.column(),
            };
            if SDK_MARKERS.iter().any(|m| file.contains(m)) {
                fallback.get_or_insert(loc);
            } else {
                return Some(loc);
            }
        }
    }
    fallback
}
