//! Sample, hold and delay (SPEC "Sample, hold and delay"): each sensor keeps a short queue of
//! timestamped readings, and a read at time t returns the newest reading taken at or before
//! t minus the delay. The queue is a fixed array, so pushing and reading never allocate.

/// Values a reading can hold (the colour sensor needs five).
pub const VALUES: usize = 6;
const DEPTH: usize = 8;

/// One timestamped reading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Time the sample was taken, ms of simulated time.
    pub t_ms: u64,
    /// The values (unused entries are 0).
    pub v: [f64; VALUES],
}

/// A ring of the latest readings.
#[derive(Clone, Debug)]
pub struct DelayQueue {
    buf: [Sample; DEPTH],
    head: usize,
    len: usize,
    delay_ms: u64,
}

impl DelayQueue {
    /// An empty queue for a sensor with this delay.
    pub fn new(delay_ms: u64) -> Self {
        Self {
            buf: [Sample {
                t_ms: 0,
                v: [0.0; VALUES],
            }; DEPTH],
            head: 0,
            len: 0,
            delay_ms,
        }
    }

    /// The delay, ms.
    pub fn delay_ms(&self) -> u64 {
        self.delay_ms
    }

    /// Add a reading taken at `t_ms`.
    pub fn push(&mut self, t_ms: u64, v: [f64; VALUES]) {
        self.buf[self.head] = Sample { t_ms, v };
        self.head = (self.head + 1) % DEPTH;
        self.len = (self.len + 1).min(DEPTH);
    }

    /// The newest reading taken at or before `now_ms - delay`, or `None` if there is none yet.
    pub fn read(&self, now_ms: u64) -> Option<Sample> {
        let limit = now_ms.checked_sub(self.delay_ms)?;
        (0..self.len)
            .map(|k| self.buf[(self.head + DEPTH - 1 - k) % DEPTH])
            .find(|s| s.t_ms <= limit)
    }

    /// Forget everything (after a reset).
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64) -> [f64; VALUES] {
        [x, 0.0, 0.0, 0.0, 0.0, 0.0]
    }

    #[test]
    fn a_read_returns_the_newest_sample_old_enough() {
        let mut q = DelayQueue::new(30);
        assert!(q.read(100).is_none());
        q.push(50, v(1.0));
        q.push(100, v(2.0));
        assert!(q.read(79).is_none());
        assert_eq!(q.read(80).unwrap().v[0], 1.0);
        assert_eq!(q.read(129).unwrap().v[0], 1.0);
        assert_eq!(q.read(130).unwrap().v[0], 2.0);
        assert_eq!(q.read(10_000).unwrap().v[0], 2.0);
        assert_eq!(q.delay_ms(), 30);
    }

    #[test]
    fn the_ring_overwrites_the_oldest_and_clear_empties_it() {
        let mut q = DelayQueue::new(0);
        for i in 0..20u64 {
            q.push(i, v(i as f64));
        }
        assert_eq!(q.read(19).unwrap().v[0], 19.0);
        assert_eq!(q.read(12).unwrap().v[0], 12.0);
        assert!(q.read(11).is_none(), "only the last 8 are kept");
        q.clear();
        assert!(q.read(100).is_none());
    }
}
