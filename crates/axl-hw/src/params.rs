//! Hardware constants from `assets/physics/hardware.ron` (all `placeholder: calibrate`).
//! Motor parameters are not here: the compiler puts them in the device map.

use serde::Deserialize;

/// Battery pack (SPEC "Battery").
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct BatteryParams {
    /// Open-circuit voltage when full, V.
    pub v_full: f64,
    /// Nominal voltage, V (informational: it is the open-circuit voltage at 50% charge).
    pub v_nom: f64,
    /// Open-circuit voltage when empty, V.
    pub v_empty: f64,
    /// Internal resistance, ohm.
    pub internal_resistance: f64,
    /// Capacity, ampere hours.
    pub capacity_ah: f64,
    /// Hub idle draw, A.
    pub idle_a: f64,
    /// State of charge at the start of a run, 0 to 1.
    pub start_soc: f64,
}

/// Built-in motor loops and the "done" criteria.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct ControlParams {
    /// Speed loop proportional gain, V s/rad.
    pub speed_kp: f64,
    /// Speed loop integral gain, V/rad.
    pub speed_ki: f64,
    /// Position loop gain (angle error to speed target), 1/s. Not in the spec (decision 0012).
    pub position_kp: f64,
    /// Position "done": error below this, degrees.
    pub done_error_deg: f64,
    /// Position "done": speed below this, rad/s.
    pub done_speed: f64,
    /// Encoder speed window, ms.
    pub speed_window_ms: usize,
}

/// Encoder resolution.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct EncoderParams {
    /// Counts per revolution of a continuous motor.
    pub cpr: f64,
    /// Servo counts over its span.
    pub servo_counts: f64,
    /// Servo span, degrees.
    pub servo_span_deg: f64,
}

/// Ultrasonic distance sensor.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct UltrasonicParams {
    /// Shortest range, m.
    pub min_m: f64,
    /// Longest range, m.
    pub max_m: f64,
    /// Samples per second.
    pub rate_hz: f64,
    /// Delay, ms.
    pub delay_ms: u64,
    /// Cone half angles of the two rings, degrees.
    pub ring_deg: (f64, f64),
    /// Rays in each ring.
    pub ring_rays: usize,
    /// Surfaces tilted more than this away from facing the sensor are missed, degrees.
    pub max_incidence_deg: f64,
    /// Noise sigma, constant part, m.
    pub sigma_abs_m: f64,
    /// Noise sigma, part proportional to distance.
    pub sigma_rel: f64,
    /// Output rounding, m.
    pub resolution_m: f64,
}

/// Time-of-flight distance sensor.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct TofParams {
    /// Shortest range, m.
    pub min_m: f64,
    /// Longest range, m.
    pub max_m: f64,
    /// Samples per second.
    pub rate_hz: f64,
    /// Delay, ms.
    pub delay_ms: u64,
    /// Tilt of the four outer rays, degrees.
    pub ring_deg: f64,
    /// Number of outer rays.
    pub ring_rays: usize,
    /// Works up to this angle between the surface normal and the sensor axis, degrees.
    pub max_incidence_deg: f64,
    /// Noise sigma, constant part, m.
    pub sigma_abs_m: f64,
    /// Noise sigma, part proportional to distance.
    pub sigma_rel: f64,
}

/// Colour sensor.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct ColorParams {
    /// Nearest surface it reads, m.
    pub min_m: f64,
    /// Farthest surface it reads, m.
    pub max_m: f64,
    /// Samples per second.
    pub rate_hz: f64,
    /// Delay, ms.
    pub delay_ms: u64,
    /// Radius of the sampled spot, m.
    pub spot_radius_m: f64,
    /// Distance of peak intensity, m.
    pub peak_m: f64,
    /// Width of the intensity falloff, m.
    pub width_m: f64,
    /// Full-scale RGB value.
    pub full_scale: f64,
    /// Noise sigma as a fraction of full scale.
    pub sigma_frac: f64,
}

/// Touch sensor.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct TouchParams {
    /// Largest force reported, N.
    pub max_n: f64,
    /// Samples per second.
    pub rate_hz: f64,
    /// Delay, ms.
    pub delay_ms: u64,
    /// Pressed above this force, N.
    pub threshold_n: f64,
    /// Debounce time, ms.
    pub debounce_ms: u32,
    /// Force noise sigma, N.
    pub sigma_n: f64,
}

/// Gyroscope.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct GyroParams {
    /// Range, rad/s.
    pub range: f64,
    /// Samples per second.
    pub rate_hz: f64,
    /// Delay, ms.
    pub delay_ms: u64,
    /// Per-sample noise sigma, rad/s.
    pub sigma: f64,
    /// Initial bias sigma per axis, rad/s.
    pub bias_sigma: f64,
    /// Random-walk step of the bias per sample, rad/s.
    pub drift_per_sample: f64,
    /// Quantisation bits.
    pub bits: u32,
}

/// Accelerometer.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct AccelParams {
    /// Range, m/s2.
    pub range: f64,
    /// Samples per second.
    pub rate_hz: f64,
    /// Delay, ms.
    pub delay_ms: u64,
    /// Per-sample noise sigma, m/s2.
    pub sigma: f64,
    /// Initial bias sigma per axis, m/s2.
    pub bias_sigma: f64,
    /// Quantisation bits.
    pub bits: u32,
}

/// All sensors.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct SensorParams {
    /// Ultrasonic.
    pub ultrasonic: UltrasonicParams,
    /// Time of flight.
    pub tof: TofParams,
    /// Colour.
    pub color: ColorParams,
    /// Touch.
    pub touch: TouchParams,
    /// Gyro.
    pub gyro: GyroParams,
    /// Accelerometer.
    pub accel: AccelParams,
}

/// Reference colours the colour sensor can name.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct ColorNames {
    /// Black.
    pub black: [f64; 3],
    /// White.
    pub white: [f64; 3],
    /// Red.
    pub red: [f64; 3],
    /// Green.
    pub green: [f64; 3],
    /// Blue.
    pub blue: [f64; 3],
    /// Yellow.
    pub yellow: [f64; 3],
}

/// Everything in `hardware.ron` that `axl-hw` needs.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct HwParams {
    /// Battery.
    pub battery: BatteryParams,
    /// Motor loops.
    pub control: ControlParams,
    /// Encoders.
    pub encoders: EncoderParams,
    /// Sensors.
    pub sensors: SensorParams,
    /// Colour names.
    pub colors: ColorNames,
}

impl HwParams {
    /// The file shipped with the program.
    pub fn builtin() -> Self {
        let text = include_str!("../../../assets/physics/hardware.ron");
        Self::parse(text).unwrap_or_else(|e| panic!("assets/physics/hardware.ron: {e}"))
    }

    /// Parse RON text.
    pub fn parse(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_file_parses_with_the_spec_numbers() {
        let p = HwParams::builtin();
        assert_eq!(p.battery.v_full, 8.4);
        assert_eq!(p.battery.internal_resistance, 0.15);
        assert_eq!(p.control.speed_kp, 0.6);
        assert_eq!(p.sensors.ultrasonic.delay_ms, 30);
        assert_eq!(p.sensors.gyro.bits, 16);
        // Nominal voltage is the open-circuit voltage at half charge.
        let mid = (p.battery.v_full + p.battery.v_empty) / 2.0;
        assert!((mid - p.battery.v_nom).abs() < 1e-12);
    }

    #[test]
    fn the_compiler_still_reads_the_same_file() {
        axl_compile::PhysicsParams::builtin();
    }
}
