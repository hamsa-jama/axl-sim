//! The battery (SPEC "Battery"): open-circuit voltage linear in state of charge, internal
//! resistance, idle draw and charge accounting.

use crate::params::BatteryParams;

/// A two-cell lithium-ion pack.
#[derive(Clone, Debug)]
pub struct Battery {
    p: BatteryParams,
    soc: f64,
    /// Current drawn in the last step, A (idle plus the sum of |motor currents|).
    current: f64,
    terminal: f64,
}

impl Battery {
    /// A battery at its configured starting charge.
    pub fn new(p: BatteryParams) -> Self {
        Self::with_soc(p, p.start_soc)
    }

    /// A battery at the given state of charge (0 to 1).
    pub fn with_soc(p: BatteryParams, soc: f64) -> Self {
        let soc = soc.clamp(0.0, 1.0);
        let mut b = Self {
            p,
            soc,
            current: p.idle_a,
            terminal: 0.0,
        };
        b.terminal = b.open_circuit();
        b
    }

    /// Open-circuit voltage at the present charge.
    pub fn open_circuit(&self) -> f64 {
        self.p.v_empty + (self.p.v_full - self.p.v_empty) * self.soc
    }

    /// State of charge, 0 to 1.
    pub fn soc(&self) -> f64 {
        self.soc
    }

    /// Terminal voltage computed at the start of the latest step.
    pub fn voltage(&self) -> f64 {
        self.terminal
    }

    /// Total current of the latest step, A.
    pub fn current(&self) -> f64 {
        self.current
    }

    /// The parameters.
    pub fn params(&self) -> &BatteryParams {
        &self.p
    }

    /// Start of a step: terminal voltage = open-circuit voltage - R_int x total current, where the
    /// total is the idle draw plus the absolute motor currents of the previous step.
    pub fn begin_step(&mut self, motor_currents_abs: f64) -> f64 {
        self.current = self.p.idle_a + motor_currents_abs;
        self.terminal =
            (self.open_circuit() - self.p.internal_resistance * self.current).max(0.0);
        self.terminal
    }

    /// End of a step: charge falls by current x dt.
    pub fn end_step(&mut self, dt: f64) {
        self.soc = (self.soc - self.current * dt / (self.p.capacity_ah * 3600.0)).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::HwParams;

    #[test]
    fn open_circuit_voltage_is_linear_and_sags_under_load() {
        let p = HwParams::builtin().battery;
        assert_eq!(Battery::with_soc(p, 1.0).open_circuit(), 8.4);
        assert_eq!(Battery::with_soc(p, 0.0).open_circuit(), 6.4);
        assert!((Battery::with_soc(p, 0.5).open_circuit() - 7.4).abs() < 1e-12);
        let mut b = Battery::new(p);
        assert!((b.open_circuit() - 8.2).abs() < 1e-12);
        let v = b.begin_step(2.0);
        assert!((v - (8.2 - 0.15 * 2.08)).abs() < 1e-12);
        assert_eq!(b.voltage(), v);
        assert_eq!(b.current(), 2.08);
        assert_eq!(b.params().capacity_ah, 2.0);
    }

    #[test]
    fn charge_falls_by_current_times_time() {
        let p = HwParams::builtin().battery;
        let mut b = Battery::new(p);
        b.begin_step(1.92); // 2.0 A in total
        for _ in 0..3600 {
            b.end_step(1.0);
        }
        // 2 A for an hour from a 2 Ah pack empties it, from 90% it stops at 0.
        assert_eq!(b.soc(), 0.0);
        let mut c = Battery::new(p);
        c.begin_step(1.92);
        c.end_step(360.0); // 0.2 Ah of 2 Ah
        assert!((c.soc() - 0.8).abs() < 1e-12);
    }
}
