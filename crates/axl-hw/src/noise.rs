//! Seeded Gaussian noise (SPEC "Determinism rules": all noise comes from `ChaCha8Rng`, seeded
//! from the run seed and the device's port letter).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

/// A noise source for one device.
#[derive(Clone, Debug)]
pub struct Noise {
    rng: ChaCha8Rng,
    spare: Option<f64>,
    enabled: bool,
}

impl Noise {
    /// Seeded from the run seed, the device's port letter (`b'H'` for the hub) and a stream number
    /// that tells apart several noise sources on one device.
    pub fn new(seed: u64, port: u8, stream: u8, enabled: bool) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        bytes[8] = port;
        bytes[9] = stream;
        bytes[10..14].copy_from_slice(b"axl1");
        Self {
            rng: ChaCha8Rng::from_seed(bytes),
            spare: None,
            enabled,
        }
    }

    /// True if noise is switched on.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    fn uniform(&mut self) -> f64 {
        // 53 random bits, in (0, 1].
        ((self.rng.next_u64() >> 11) as f64 + 1.0) / 9_007_199_254_740_992.0
    }

    /// One standard normal sample (Box-Muller), or 0 when noise is off.
    pub fn standard(&mut self) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        if let Some(s) = self.spare.take() {
            return s;
        }
        let (u1, u2) = (self.uniform(), self.uniform());
        let r = (-2.0 * u1.ln()).sqrt();
        let a = std::f64::consts::TAU * u2;
        self.spare = Some(r * a.sin());
        r * a.cos()
    }

    /// A normal sample with the given standard deviation.
    pub fn gauss(&mut self, sigma: f64) -> f64 {
        sigma * self.standard()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_and_sigma_are_right_and_the_stream_is_reproducible() {
        let mut a = Noise::new(7, b'A', 0, true);
        let mut b = Noise::new(7, b'A', 0, true);
        let n = 100_000;
        let xs: Vec<f64> = (0..n).map(|_| a.gauss(2.0)).collect();
        assert!(
            xs.iter()
                .zip((0..n).map(|_| b.gauss(2.0)))
                .all(|(x, y)| *x == y)
        );
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.03, "{mean}");
        assert!((var.sqrt() - 2.0).abs() < 0.03, "{}", var.sqrt());
    }

    #[test]
    fn port_seed_and_stream_change_the_stream_and_off_gives_zero() {
        let first = |s, p, st| Noise::new(s, p, st, true).standard();
        assert_ne!(first(1, b'A', 0), first(2, b'A', 0));
        assert_ne!(first(1, b'A', 0), first(1, b'B', 0));
        assert_ne!(first(1, b'A', 0), first(1, b'A', 1));
        let mut off = Noise::new(1, b'A', 0, false);
        assert_eq!(off.gauss(5.0), 0.0);
        assert!(!off.enabled());
    }
}
