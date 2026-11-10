//! The `axl` import module: every function of `sdk/c/axl.h` (SPEC "Functions"). The names here
//! are the `import_name`s, without the `axl_` prefix.

use wasmtime::{AsContextMut, Caller, Linker, Memory};

use crate::api::{DeviceKind, MotorCommand, MotorReading, RobotApi, StepStatus};
use crate::state::{HostState, StopKind};

/// The module name the learner's program imports from.
pub const MODULE: &str = "axl";
/// The ABI version `axl_init` must be given.
pub const ABI_VERSION: i32 = 1;

/// Every function name (without `axl_`), for the import check and the tests.
pub const FUNCTIONS: [&str; 29] = [
    "init",
    "step",
    "time",
    "device",
    "device_kind",
    "motor_power",
    "motor_speed",
    "motor_position",
    "motor_brake",
    "motor_coast",
    "motor_reset_angle",
    "motor_angle",
    "motor_rate",
    "motor_current",
    "motor_done",
    "distance",
    "color_rgb",
    "color_reflect",
    "color_name",
    "touch_pressed",
    "touch_force",
    "imu_gyro",
    "imu_accel",
    "battery_voltage",
    "plot",
    "mark_pose",
    "draw_line",
    "draw_clear",
    "report",
];

/// The device kind codes returned by `axl_device_kind` (the `AXL_KIND_*` enum in `axl.h`).
pub fn kind_code(k: DeviceKind) -> i32 {
    match k {
        DeviceKind::Hub => 0,
        DeviceKind::Motor => 1,
        DeviceKind::Distance => 2,
        DeviceKind::Color => 3,
        DeviceKind::Touch => 4,
    }
}

fn memory<A: RobotApi + 'static>(c: &mut Caller<'_, HostState<A>>) -> Option<Memory> {
    c.get_export("memory").and_then(|e| e.into_memory())
}

fn read_bytes<A: RobotApi + 'static>(
    c: &mut Caller<'_, HostState<A>>,
    func: &str,
    what: &str,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<Vec<u8>> {
    let ok = ptr >= 0 && (0..=4096).contains(&len);
    let mut buf = vec![0u8; if ok { len as usize } else { 0 }];
    let read = ok && memory(c).is_some_and(|m| m.read(&*c, ptr as usize, &mut buf).is_ok());
    if !read {
        return c.data_mut().stop(
            StopKind::BadArgument,
            format!("axl_{func}: {what} points outside the program's memory or is longer than 4096 bytes."),
        );
    }
    Ok(buf)
}

fn read_str<A: RobotApi + 'static>(
    c: &mut Caller<'_, HostState<A>>,
    func: &str,
    what: &str,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<String> {
    read_bytes(c, func, what, ptr, len).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn write_bytes<A: RobotApi + 'static>(
    c: &mut Caller<'_, HostState<A>>,
    func: &str,
    ptr: i32,
    data: &[u8],
) -> wasmtime::Result<()> {
    let ok = ptr >= 0 && memory(c).is_some_and(|m| m.write(&mut *c, ptr as usize, data).is_ok());
    if ok {
        Ok(())
    } else {
        c.data_mut().stop(
            StopKind::BadArgument,
            format!("axl_{func}: the output pointer points outside the program's memory."),
        )
    }
}

fn pet_watchdog<A: RobotApi + 'static>(c: &mut Caller<'_, HostState<A>>) {
    let ticks = c.data().watchdog_ticks;
    c.as_context_mut().set_epoch_deadline(ticks);
}

fn finite<A: RobotApi + 'static>(
    c: &mut Caller<'_, HostState<A>>,
    func: &str,
    what: &str,
    v: f64,
) -> wasmtime::Result<()> {
    if v.is_finite() {
        Ok(())
    } else {
        c.data_mut().stop(
            StopKind::BadArgument,
            format!("axl_{func}: {what} must be a number, not {v}."),
        )
    }
}

macro_rules! motor_fn {
    ($l:expr, $name:literal, |$c:ident, $dev:ident| $body:expr) => {
        $l.func_wrap(
            MODULE,
            $name,
            |mut $c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<()> {
                $c.data_mut().enter($name)?;
                let $dev = $c.data_mut().device($name, dev, DeviceKind::Motor)?;
                $body;
                Ok(())
            },
        )?;
    };
}

macro_rules! motor_read {
    ($l:expr, $name:literal, $what:expr) => {
        $l.func_wrap(
            MODULE,
            $name,
            |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<f64> {
                c.data_mut().enter($name)?;
                let d = c.data_mut().device($name, dev, DeviceKind::Motor)?;
                Ok(c.data().api.motor_reading(d, $what))
            },
        )?;
    };
}

/// Define every `axl.*` function on a linker.
pub fn add_to_linker<A: RobotApi + 'static>(l: &mut Linker<HostState<A>>) -> wasmtime::Result<()> {
    // ----- lifecycle -----
    l.func_wrap(MODULE, "init", |mut c: Caller<'_, HostState<A>>, abi: i32| -> wasmtime::Result<i32> {
        if abi != ABI_VERSION {
            return c.data_mut().stop(
                StopKind::AbiVersion,
                format!(
                    "This program was built for AXL ABI version {abi}, but this AXL speaks version {ABI_VERSION}. Rebuild it with the SDK that came with this AXL."
                ),
            );
        }
        c.data_mut().inited = true;
        Ok(0)
    })?;
    l.func_wrap(
        MODULE,
        "step",
        |mut c: Caller<'_, HostState<A>>, ms: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("step")?;
            if !(1..=1000).contains(&ms) {
                return c.data_mut().stop(
                    StopKind::BadArgument,
                    format!("axl_step: ms must be between 1 and 1000, not {ms}."),
                );
            }
            if c.data().mission_over {
                pet_watchdog(&mut c);
                return Ok(1);
            }
            pet_watchdog(&mut c);
            let r = c.data_mut().api.step(ms as u32);
            pet_watchdog(&mut c);
            match r {
                Ok(StepStatus::Running) => Ok(0),
                Ok(StepStatus::MissionOver) => {
                    c.data_mut().mission_over = true;
                    Ok(1)
                }
                Err(m) => c.data_mut().stop(StopKind::Simulation, m),
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "time",
        |mut c: Caller<'_, HostState<A>>| -> wasmtime::Result<f64> {
            c.data_mut().enter("time")?;
            Ok(c.data().api.time())
        },
    )?;

    // ----- devices -----
    l.func_wrap(
        MODULE,
        "device",
        |mut c: Caller<'_, HostState<A>>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("device")?;
            let name = read_str(&mut c, "device", "the name", ptr, len)?;
            Ok(c.data().api.find_device(&name).map_or(-1, |d| d as i32))
        },
    )?;
    l.func_wrap(
        MODULE,
        "device_kind",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("device_kind")?;
            let info = if dev >= 0 {
                c.data().api.device_info(dev as usize)
            } else {
                None
            };
            Ok(info.map_or(-1, |(_, k)| kind_code(k)))
        },
    )?;

    // ----- motor commands -----
    l.func_wrap(
        MODULE,
        "motor_power",
        |mut c: Caller<'_, HostState<A>>, dev: i32, p: f64| -> wasmtime::Result<()> {
            c.data_mut().enter("motor_power")?;
            let d = c.data_mut().device("motor_power", dev, DeviceKind::Motor)?;
            finite(&mut c, "motor_power", "power", p)?;
            c.data_mut()
                .api
                .motor_command(d, MotorCommand::Power(p.clamp(-1.0, 1.0)));
            Ok(())
        },
    )?;
    l.func_wrap(
        MODULE,
        "motor_speed",
        |mut c: Caller<'_, HostState<A>>, dev: i32, w: f64| -> wasmtime::Result<()> {
            c.data_mut().enter("motor_speed")?;
            let d = c.data_mut().device("motor_speed", dev, DeviceKind::Motor)?;
            finite(&mut c, "motor_speed", "speed", w)?;
            c.data_mut().api.motor_command(d, MotorCommand::Speed(w));
            Ok(())
        },
    )?;
    l.func_wrap(
        MODULE,
        "motor_position",
        |mut c: Caller<'_, HostState<A>>, dev: i32, rad: f64, max: f64| -> wasmtime::Result<()> {
            c.data_mut().enter("motor_position")?;
            let d = c.data_mut().device("motor_position", dev, DeviceKind::Motor)?;
            finite(&mut c, "motor_position", "the angle", rad)?;
            finite(&mut c, "motor_position", "max_rad_s", max)?;
            if max <= 0.0 {
                return c.data_mut().stop(
                    StopKind::BadArgument,
                    "axl_motor_position: max_rad_s must be greater than zero, otherwise the motor can never move."
                        .into(),
                );
            }
            c.data_mut()
                .api
                .motor_command(d, MotorCommand::Position { target: rad, max_speed: max });
            Ok(())
        },
    )?;
    motor_fn!(l, "motor_brake", |c, d| c
        .data_mut()
        .api
        .motor_command(d, MotorCommand::Brake));
    motor_fn!(l, "motor_coast", |c, d| c
        .data_mut()
        .api
        .motor_command(d, MotorCommand::Coast));
    motor_fn!(l, "motor_reset_angle", |c, d| c
        .data_mut()
        .api
        .motor_reset_angle(d));

    // ----- motor readings -----
    motor_read!(l, "motor_angle", MotorReading::Angle);
    motor_read!(l, "motor_rate", MotorReading::Rate);
    motor_read!(l, "motor_current", MotorReading::Current);
    l.func_wrap(
        MODULE,
        "motor_done",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("motor_done")?;
            let d = c.data_mut().device("motor_done", dev, DeviceKind::Motor)?;
            Ok(i32::from(
                c.data().api.motor_reading(d, MotorReading::Done) > 0.5,
            ))
        },
    )?;

    // ----- sensors -----
    l.func_wrap(
        MODULE,
        "distance",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<f64> {
            c.data_mut().enter("distance")?;
            let d = c.data_mut().device("distance", dev, DeviceKind::Distance)?;
            Ok(c.data().api.distance(d))
        },
    )?;
    l.func_wrap(
        MODULE,
        "color_rgb",
        |mut c: Caller<'_, HostState<A>>, dev: i32, out: i32| -> wasmtime::Result<()> {
            c.data_mut().enter("color_rgb")?;
            let d = c.data_mut().device("color_rgb", dev, DeviceKind::Color)?;
            let (rgb, _, _) = c.data().api.color(d);
            let mut bytes = [0u8; 12];
            for (i, v) in rgb.iter().enumerate() {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            write_bytes(&mut c, "color_rgb", out, &bytes)
        },
    )?;
    l.func_wrap(
        MODULE,
        "color_reflect",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("color_reflect")?;
            let d = c
                .data_mut()
                .device("color_reflect", dev, DeviceKind::Color)?;
            Ok(c.data().api.color(d).1)
        },
    )?;
    l.func_wrap(
        MODULE,
        "color_name",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("color_name")?;
            let d = c.data_mut().device("color_name", dev, DeviceKind::Color)?;
            Ok(c.data().api.color(d).2 as i32)
        },
    )?;
    l.func_wrap(
        MODULE,
        "touch_pressed",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<i32> {
            c.data_mut().enter("touch_pressed")?;
            let d = c
                .data_mut()
                .device("touch_pressed", dev, DeviceKind::Touch)?;
            Ok(i32::from(c.data().api.touch(d).0))
        },
    )?;
    l.func_wrap(
        MODULE,
        "touch_force",
        |mut c: Caller<'_, HostState<A>>, dev: i32| -> wasmtime::Result<f64> {
            c.data_mut().enter("touch_force")?;
            let d = c.data_mut().device("touch_force", dev, DeviceKind::Touch)?;
            Ok(c.data().api.touch(d).1)
        },
    )?;

    // ----- hub -----
    l.func_wrap(
        MODULE,
        "imu_gyro",
        |mut c: Caller<'_, HostState<A>>, out: i32| -> wasmtime::Result<()> {
            c.data_mut().enter("imu_gyro")?;
            let v = c.data().api.imu_gyro();
            write_bytes(&mut c, "imu_gyro", out, &f64_bytes(v))
        },
    )?;
    l.func_wrap(
        MODULE,
        "imu_accel",
        |mut c: Caller<'_, HostState<A>>, out: i32| -> wasmtime::Result<()> {
            c.data_mut().enter("imu_accel")?;
            let v = c.data().api.imu_accel();
            write_bytes(&mut c, "imu_accel", out, &f64_bytes(v))
        },
    )?;
    l.func_wrap(
        MODULE,
        "battery_voltage",
        |mut c: Caller<'_, HostState<A>>| -> wasmtime::Result<f64> {
            c.data_mut().enter("battery_voltage")?;
            Ok(c.data().api.battery_voltage())
        },
    )?;

    // ----- teaching and missions -----
    l.func_wrap(
        MODULE,
        "plot",
        |mut c: Caller<'_, HostState<A>>, ptr: i32, len: i32, v: f64| -> wasmtime::Result<()> {
            c.data_mut().enter("plot")?;
            let ch = read_str(&mut c, "plot", "the channel name", ptr, len)?;
            match c.data_mut().api.plot(&ch, v) {
                Ok(()) => Ok(()),
                Err(m) => c.data_mut().stop(StopKind::Refused, m),
            }
        },
    )?;
    l.func_wrap(
        MODULE,
        "mark_pose",
        |mut c: Caller<'_, HostState<A>>, x: f64, y: f64, yaw: f64| -> wasmtime::Result<()> {
            c.data_mut().enter("mark_pose")?;
            c.data_mut().api.mark_pose(x, y, yaw);
            Ok(())
        },
    )?;
    l.func_wrap(
        MODULE,
        "draw_line",
        |mut c: Caller<'_, HostState<A>>,
         x0: f64,
         y0: f64,
         z0: f64,
         x1: f64,
         y1: f64,
         z1: f64,
         rgba: i32|
         -> wasmtime::Result<()> {
            c.data_mut().enter("draw_line")?;
            c.data_mut()
                .api
                .draw_line([x0, y0, z0], [x1, y1, z1], rgba as u32);
            Ok(())
        },
    )?;
    l.func_wrap(
        MODULE,
        "draw_clear",
        |mut c: Caller<'_, HostState<A>>| -> wasmtime::Result<()> {
            c.data_mut().enter("draw_clear")?;
            c.data_mut().api.draw_clear();
            Ok(())
        },
    )?;
    l.func_wrap(
        MODULE,
        "report",
        |mut c: Caller<'_, HostState<A>>, ptr: i32, len: i32, v: f64| -> wasmtime::Result<()> {
            c.data_mut().enter("report")?;
            let key = read_str(&mut c, "report", "the key", ptr, len)?;
            c.data_mut().api.report(&key, v);
            Ok(())
        },
    )?;
    Ok(())
}

fn f64_bytes(v: [f64; 3]) -> [u8; 24] {
    let mut b = [0u8; 24];
    for (i, x) in v.iter().enumerate() {
        b[i * 8..i * 8 + 8].copy_from_slice(&x.to_le_bytes());
    }
    b
}
