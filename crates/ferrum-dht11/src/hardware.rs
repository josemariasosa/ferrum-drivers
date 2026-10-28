/// A bidirectional data line for the DHT11 protocol.
///
/// The host drives DATA low to initiate communication,
/// then releases the line so the sensor can respond.
pub trait DhtPin {
    type Error;

    /// Configure DATA as output and drive it LOW.
    fn drive_low(&mut self) -> Result<(), Self::Error>;

    /// Release DATA by configuring the pin as input.
    fn release(&mut self) -> Result<(), Self::Error>;

    /// Read the current logic level.
    fn is_high(&mut self) -> Result<bool, Self::Error>;

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        self.is_high().map(|high| !high)
    }
}

/// A monotonic clock with microsecond resolution.
///
/// Implementations must advance independently of the
/// driver and must not wrap during a DHT11 transaction.
pub trait MicrosecondClock {
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
