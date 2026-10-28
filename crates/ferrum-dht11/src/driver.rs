use embedded_hal_async::delay::DelayNs;

use crate::hardware::{DhtPin, MicrosecondClock};
use crate::protocol::DhtError;

pub struct Dht11<P, C> {
    pin: P,
    clock: C,
}

impl<P, C> Dht11<P, C>
where
    P: DhtPin,
    C: MicrosecondClock,
{
    pub fn new(pin: P, clock: C) -> Self {
        Self { pin, clock }
    }

    pub fn into_parts(self) -> (P, C) {
        (self.pin, self.clock)
    }

    async fn start_signal<D>(&mut self, delay: &mut D) -> Result<(), P::Error>
    where
        D: DelayNs,
    {
        self.pin.drive_low()?;

        delay.delay_us(18_000).await;

        self.pin.release()?;

        Ok(())
    }

    fn wait_for_initial_low(&mut self) -> Result<(), DhtError> {
        let start = self.clock.now_us();

        loop {
            if self.pin.is_low().map_err(|_| DhtError::NoInitialLow)? {
                return Ok(());
            }

            if self.clock.now_us().wrapping_sub(start) >= 120 {
                return Err(DhtError::NoInitialLow);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::cell::Cell;

    use embedded_hal_mock::eh1::delay::{CheckedDelay, Transaction};

    use futures::executor::block_on;

    struct FakePin;

    impl DhtPin for FakePin {
        type Error = ();

        fn drive_low(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn release(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    struct FakeClock;

    impl MicrosecondClock for FakeClock {
        fn now_us(&self) -> u64 {
            42
        }
    }

    #[test]
    fn creates_driver_with_generic_hardware() {
        let pin = FakePin;
        let clock = FakeClock;

        let sensor = Dht11::new(pin, clock);

        let (_pin, clock) = sensor.into_parts();

        assert_eq!(clock.now_us(), 42);
    }

    struct RecordingPin {
        driven_low: bool,
        released: bool,
    }

    impl DhtPin for RecordingPin {
        type Error = ();

        fn drive_low(&mut self) -> Result<(), Self::Error> {
            self.driven_low = true;
            Ok(())
        }

        fn release(&mut self) -> Result<(), Self::Error> {
            self.released = true;
            Ok(())
        }

        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[test]
    fn start_signal_drives_low_and_releases_pin() {
        let pin = RecordingPin {
            driven_low: false,
            released: false,
        };

        let clock = FakeClock;

        let mut sensor = Dht11::new(pin, clock);

        let expectations = [Transaction::async_delay_us(18_000)];

        let mut delay = CheckedDelay::new(&expectations);

        block_on(sensor.start_signal(&mut delay)).unwrap();

        delay.done();

        let (pin, _) = sensor.into_parts();

        assert!(pin.driven_low);
        assert!(pin.released);
    }

    #[test]
    fn detects_initial_low() {
        struct LowPin;

        impl DhtPin for LowPin {
            type Error = ();

            fn drive_low(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }

            fn release(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }

            fn is_high(&mut self) -> Result<bool, Self::Error> {
                Ok(false)
            }
        }

        let mut sensor = Dht11::new(LowPin, FakeClock);

        assert_eq!(sensor.wait_for_initial_low(), Ok(()));
    }

    struct AdvancingClock {
        now: Cell<u64>,
        step_us: u64,
    }

    impl AdvancingClock {
        fn new(start_us: u64, step_us: u64) -> Self {
            Self {
                now: Cell::new(start_us),
                step_us,
            }
        }
    }

    impl MicrosecondClock for AdvancingClock {
        fn now_us(&self) -> u64 {
            let current = self.now.get();

            self.now.set(current.wrapping_add(self.step_us));

            current
        }
    }

    #[test]
    fn times_out_when_sensor_never_goes_low() {
        let pin = FakePin; // Always HIGH

        let clock = AdvancingClock::new(0, 10);

        let mut sensor = Dht11::new(pin, clock);

        assert_eq!(sensor.wait_for_initial_low(), Err(DhtError::NoInitialLow));
    }

    #[test]
    fn initial_low_arrives_before_timeout() {
        let clock = AdvancingClock::new(0, 10);
        let mut pin = FakePin::new(3);

        let start = clock.now_us();

        let result = loop {
            if pin.read_low() {
                break Ok(());
            }

            if clock.now_us().wrapping_sub(start) >= 120 {
                break Err(DhtError::NoInitialLow);
            }
        };

        assert!(result.is_ok());
        assert_eq!(pin.reads, 4);
    }

    #[test]
    fn initial_low_times_out() {
        let clock = AdvancingClock::new(0, 10);
        let mut pin = MockPin::new(usize::MAX);

        let start = clock.now_us();

        let result = loop {
            if pin.read_low() {
                break Ok(());
            }

            if clock.now_us().wrapping_sub(start) >= 120 {
                break Err(DhtError::NoInitialLow);
            }
        };

        assert!(matches!(result, Err(DhtError::NoInitialLow)));
        assert_eq!(pin.reads, 12);
    }
}
