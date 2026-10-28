/// A bidirectional data line for the DHT11 protocol.
///
/// The host drives DATA low to initiate communication, then releases the line
/// so the sensor can respond. The released line must be pulled high externally
/// (or by a suitable board-level pull-up); implementations should configure
/// the pin so it does not actively drive high against the sensor.
pub trait DhtPin {
    /// The error type returned by GPIO operations.
    type Error;

    /// Configure DATA as an output and drive it low.
    fn drive_low(&mut self) -> Result<(), Self::Error>;

    /// Release DATA, typically by configuring the pin as an input.
    fn release(&mut self) -> Result<(), Self::Error>;

    /// Read whether DATA is at a high logic level.
    fn is_high(&mut self) -> Result<bool, Self::Error>;

    /// Read whether DATA is at a low logic level.
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        self.is_high().map(|high| !high)
    }
}

/// A monotonic clock with microsecond resolution.
///
/// The clock must continue advancing while the driver synchronously polls,
/// provide useful resolution around microsecond-scale pulses, and use a
/// consistent microsecond unit. Elapsed durations are computed with wrapping
/// subtraction, so a `u64` rollover is supported provided a measured interval
/// is shorter than the full counter period.
pub trait MicrosecondClock {
    /// Return the current monotonic time in microseconds.
    fn now_us(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::cell::Cell;

    struct MockClock {
        time: Cell<u64>,
    }

    impl MockClock {
        fn new() -> Self {
            Self { time: Cell::new(0) }
        }
    }

    impl MicrosecondClock for MockClock {
        fn now_us(&self) -> u64 {
            let now = self.time.get();

            self.time.set(now + 1);

            now
        }
    }

    #[test]
    fn virtual_clock_advances() {
        let clock = MockClock::new();

        assert_eq!(clock.now_us(), 0);
        assert_eq!(clock.now_us(), 1);
        assert_eq!(clock.now_us(), 2);
    }

    #[test]
    fn virtual_clock_measures_elapsed_time() {
        let clock = MockClock::new();

        let start = clock.now_us();

        for _ in 0..100 {
            clock.now_us();
        }

        let elapsed = clock.now_us() - start;

        assert_eq!(elapsed, 101);
    }
}
