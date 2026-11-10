//! WASI shims (SPEC "Sandbox and limits"): stdout and stderr go to the log, there is no filesystem,
//! environment or network, clocks return sim time and `random_get` returns seeded bytes.
//! Only the `wasi_snapshot_preview1` functions that libc and Rust's std use at start-up and for
//! printing are real; the rest return an error code or stop the program with an explanation.

use wasmtime::{Caller, Linker, Memory};

use crate::api::{LogStream, RobotApi};
use crate::state::HostState;

const ESUCCESS: i32 = 0;
const EBADF: i32 = 8;
const EINVAL: i32 = 28;
const ENOSYS: i32 = 52;
const ESPIPE: i32 = 70;

fn memory<A: RobotApi + 'static>(c: &mut Caller<'_, HostState<A>>) -> Option<Memory> {
    c.get_export("memory").and_then(|e| e.into_memory())
}

fn write_u32<A: RobotApi + 'static>(c: &mut Caller<'_, HostState<A>>, ptr: u32, v: u32) -> bool {
    let Some(m) = memory(c) else { return false };
    m.write(c, ptr as usize, &v.to_le_bytes()).is_ok()
}

fn write_u64<A: RobotApi + 'static>(c: &mut Caller<'_, HostState<A>>, ptr: u32, v: u64) -> bool {
    let Some(m) = memory(c) else { return false };
    m.write(c, ptr as usize, &v.to_le_bytes()).is_ok()
}

const MODULE: &str = "wasi_snapshot_preview1";

/// Define the WASI functions on a linker.
pub fn add_to_linker<A: RobotApi + 'static>(l: &mut Linker<HostState<A>>) -> wasmtime::Result<()> {
    // Arguments and environment: none.
    l.func_wrap(
        MODULE,
        "args_sizes_get",
        |mut c: Caller<'_, HostState<A>>, argc: u32, size: u32| -> i32 {
            if write_u32(&mut c, argc, 0) && write_u32(&mut c, size, 0) {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "args_get",
        |_: Caller<'_, HostState<A>>, _a: u32, _b: u32| -> i32 { ESUCCESS },
    )?;
    l.func_wrap(
        MODULE,
        "environ_sizes_get",
        |mut c: Caller<'_, HostState<A>>, n: u32, size: u32| -> i32 {
            if write_u32(&mut c, n, 0) && write_u32(&mut c, size, 0) {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "environ_get",
        |_: Caller<'_, HostState<A>>, _a: u32, _b: u32| -> i32 { ESUCCESS },
    )?;

    // Clocks return simulated time. The resolution is 1 ms, the physics step.
    l.func_wrap(
        MODULE,
        "clock_res_get",
        |mut c: Caller<'_, HostState<A>>, _id: u32, out: u32| -> i32 {
            if write_u64(&mut c, out, 1_000_000) {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "clock_time_get",
        |mut c: Caller<'_, HostState<A>>, id: u32, _precision: u64, out: u32| -> i32 {
            if id > 3 {
                return EINVAL;
            }
            let ns = (c.data().api.time() * 1e9).round() as u64;
            if write_u64(&mut c, out, ns) {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;

    // Seeded random bytes.
    l.func_wrap(
        MODULE,
        "random_get",
        |mut c: Caller<'_, HostState<A>>, ptr: u32, len: u32| -> i32 {
            let Some(m) = memory(&mut c) else {
                return EINVAL;
            };
            let mut buf = vec![0u8; len as usize];
            c.data_mut().fill_random(&mut buf);
            if m.write(&mut c, ptr as usize, &buf).is_ok() {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;

    // Standard streams.
    l.func_wrap(
        MODULE,
        "fd_write",
        |mut c: Caller<'_, HostState<A>>, fd: u32, iovs: u32, n: u32, written: u32| -> i32 {
            let stream = match fd {
                1 => LogStream::Stdout,
                2 => LogStream::Stderr,
                _ => return EBADF,
            };
            let Some(m) = memory(&mut c) else {
                return EINVAL;
            };
            let mut total = 0u32;
            let mut data = Vec::new();
            for i in 0..n {
                let mut iov = [0u8; 8];
                if m.read(&c, (iovs + i * 8) as usize, &mut iov).is_err() {
                    return EINVAL;
                }
                let ptr = u32::from_le_bytes([iov[0], iov[1], iov[2], iov[3]]);
                let len = u32::from_le_bytes([iov[4], iov[5], iov[6], iov[7]]);
                let start = data.len();
                data.resize(start + len as usize, 0);
                if m.read(&c, ptr as usize, &mut data[start..]).is_err() {
                    return EINVAL;
                }
                total += len;
            }
            c.data_mut().write_log(stream, &data);
            if write_u32(&mut c, written, total) {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "fd_read",
        |mut c: Caller<'_, HostState<A>>, fd: u32, _iovs: u32, _n: u32, nread: u32| -> i32 {
            // Standard input is empty.
            if fd != 0 {
                return EBADF;
            }
            if write_u32(&mut c, nread, 0) {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "fd_close",
        |_: Caller<'_, HostState<A>>, fd: u32| -> i32 { if fd <= 2 { ESUCCESS } else { EBADF } },
    )?;
    l.func_wrap(
        MODULE,
        "fd_fdstat_get",
        |mut c: Caller<'_, HostState<A>>, fd: u32, out: u32| -> i32 {
            if fd > 2 {
                return EBADF;
            }
            // filetype character_device (2), flags 0, rights: read/write (fd_read bit 1, fd_write bit 6).
            let Some(m) = memory(&mut c) else {
                return EINVAL;
            };
            let mut buf = [0u8; 24];
            buf[0] = 2;
            buf[8..16].copy_from_slice(&((1u64 << 1) | (1u64 << 6)).to_le_bytes());
            if m.write(&mut c, out as usize, &buf).is_ok() {
                ESUCCESS
            } else {
                EINVAL
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "fd_fdstat_set_flags",
        |_: Caller<'_, HostState<A>>, _fd: u32, _f: u32| -> i32 { ESUCCESS },
    )?;
    l.func_wrap(
        MODULE,
        "fd_seek",
        |_: Caller<'_, HostState<A>>, _fd: u32, _off: i64, _w: u32, _out: u32| -> i32 { ESPIPE },
    )?;
    l.func_wrap(
        MODULE,
        "fd_tell",
        |_: Caller<'_, HostState<A>>, _fd: u32, _out: u32| -> i32 { ESPIPE },
    )?;
    l.func_wrap(
        MODULE,
        "fd_sync",
        |_: Caller<'_, HostState<A>>, _fd: u32| -> i32 { ESUCCESS },
    )?;
    // No pre-opened directories: libc stops asking after the first EBADF.
    l.func_wrap(
        MODULE,
        "fd_prestat_get",
        |_: Caller<'_, HostState<A>>, _fd: u32, _out: u32| -> i32 { EBADF },
    )?;
    l.func_wrap(
        MODULE,
        "fd_prestat_dir_name",
        |_: Caller<'_, HostState<A>>, _fd: u32, _p: u32, _l: u32| -> i32 { EBADF },
    )?;
    // No filesystem.
    l.func_wrap(
        MODULE,
        "path_open",
        |_: Caller<'_, HostState<A>>,
         _fd: u32,
         _df: u32,
         _p: u32,
         _pl: u32,
         _of: u32,
         _rb: u64,
         _ri: u64,
         _ff: u32,
         _out: u32|
         -> i32 { ENOSYS },
    )?;
    l.func_wrap(
        MODULE,
        "sched_yield",
        |_: Caller<'_, HostState<A>>| -> i32 { ESUCCESS },
    )?;

    // Sleeping would wait for wall-clock time that never passes in the simulation.
    l.func_wrap(
        MODULE,
        "poll_oneoff",
        |mut c: Caller<'_, HostState<A>>,
         _i: u32,
         _o: u32,
         _n: u32,
         _out: u32|
         -> wasmtime::Result<i32> { c.data_mut().stop_sleep() },
    )?;
    l.func_wrap(
        MODULE,
        "proc_exit",
        |mut c: Caller<'_, HostState<A>>, code: u32| -> wasmtime::Result<()> {
            c.data_mut().exit(code)
        },
    )?;
    l.func_wrap(
        MODULE,
        "proc_raise",
        |mut c: Caller<'_, HostState<A>>, _sig: u32| -> wasmtime::Result<i32> {
            c.data_mut().stop_signal().map(|_| ENOSYS)
        },
    )?;
    Ok(())
}
