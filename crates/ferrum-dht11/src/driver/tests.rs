extern crate std;

use std::vec::Vec;

use super::*;

use core::cell::Cell;

use embedded_hal_mock::eh1::delay::{CheckedDelay, Transaction};

use futures::executor::block_on;

struct SharedClock<'a> {
    time: &'a Cell<u64>,
    step_us: u64,
}

impl MicrosecondClock for SharedClock<'_> {
    fn now_us(&self) -> u64 {
        let now = self.time.get();

        self.time.set(now.wrapping_add(self.step_us));

        now
    }
}

struct TimedPin<'a> {
    time: &'a Cell<u64>,
    initial_high: bool,
    transitions: &'a [(u64, bool)],
}

impl DhtPin for TimedPin<'_> {
    type Error = ();

    fn drive_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn release(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_high(&mut self) -> Result<bool, Self::Error> {
        let now = self.time.get();

        let level = self
            .transitions
            .iter()
            .rev()
            .find(|(at_us, _)| *at_us <= now)
            .map(|(_, high)| *high)
            .unwrap_or(self.initial_high);

        Ok(level)
    }
}

struct TransitionPin {
    reads: usize,
    high_reads: usize,
}

impl TransitionPin {
    fn new(high_reads: usize) -> Self {
        Self {
            reads: 0,
            high_reads,
        }
    }
}

impl DhtPin for TransitionPin {
    type Error = ();

    fn drive_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn release(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_high(&mut self) -> Result<bool, Self::Error> {
        self.reads += 1;

        Ok(self.reads <= self.high_reads)
    }
}

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
    let pin = TransitionPin::new(3);
    let clock = AdvancingClock::new(0, 10);

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.wait_for_initial_low(), Ok(()));

    let (pin, _) = sensor.into_parts();

    assert_eq!(pin.reads, 4);
}

#[test]
fn initial_low_times_out() {
    let pin = TransitionPin::new(usize::MAX);
    let clock = AdvancingClock::new(0, 10);

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.wait_for_initial_low(), Err(DhtError::NoInitialLow));

    let (pin, _) = sensor.into_parts();

    assert_eq!(pin.reads, 12);
}

#[test]
fn initial_low_at_timeout_boundary() {
    let pin = TransitionPin::new(12);
    let clock = AdvancingClock::new(0, 10);

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.wait_for_initial_low(), Err(DhtError::NoInitialLow));

    let (pin, _) = sensor.into_parts();

    assert_eq!(pin.reads, 12);
}

#[test]
fn measures_high_pulse_duration() {
    let time = Cell::new(0);
    let transitions = [(80, false)];

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(
        sensor.measure_pulse(true, 150, DhtError::HighDidNotEnd,),
        Ok(80)
    );
}

#[test]
fn high_pulse_times_out() {
    let time = Cell::new(0);

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &[],
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(
        sensor.measure_pulse(true, 150, DhtError::HighDidNotEnd,),
        Err(DhtError::HighDidNotEnd)
    );
}

#[test]
fn validates_complete_handshake() {
    let time = Cell::new(0);

    let transitions = [
        (20, false),  // Sensor begins LOW response
        (100, true),  // LOW pulse ends after 80 us
        (180, false), // HIGH pulse ends after 80 us
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.validate_response(), Ok(()));
}

#[test]
fn rejects_short_low_response() {
    let time = Cell::new(0);

    let transitions = [
        (20, false),
        (50, true), // LOW lasted only 30 us
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(
        sensor.validate_response(),
        Err(DhtError::LowInvalidDuration)
    );
}

#[test]
fn rejects_short_high_response() {
    let time = Cell::new(0);

    let transitions = [
        (20, false),
        (100, true),
        (130, false), // HIGH lasted only 30 us
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(
        sensor.validate_response(),
        Err(DhtError::HighInvalidDuration)
    );
}

#[test]
fn low_response_times_out() {
    let time = Cell::new(0);

    let transitions = [
        (20, false), // Sensor begins LOW and stays LOW
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.validate_response(), Err(DhtError::LowDidNotEnd));
}

#[test]
fn high_response_times_out() {
    let time = Cell::new(0);

    let transitions = [
        (20, false),
        (100, true), // Sensor begins HIGH and stays HIGH
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: true,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.validate_response(), Err(DhtError::HighDidNotEnd));
}

#[test]
fn reads_zero_bit() {
    let time = Cell::new(0);

    let transitions = [
        (50, true),  // LOW ends after 50 us
        (78, false), // HIGH ends after 28 us
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: false,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.read_bit(), Ok(false));
}

#[test]
fn reads_one_bit() {
    let time = Cell::new(0);

    let transitions = [
        (50, true),   // LOW ends after 50 us
        (120, false), // HIGH ends after 70 us
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: false,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.read_bit(), Ok(true));
}

#[test]
fn bit_low_times_out() {
    let time = Cell::new(0);

    let pin = TimedPin {
        time: &time,
        initial_high: false,
        transitions: &[],
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.read_bit(), Err(DhtError::BitLowDidNotEnd));
}

#[test]
fn bit_high_times_out() {
    let time = Cell::new(0);

    let transitions = [
        (50, true), // HIGH begins but never ends
    ];

    let pin = TimedPin {
        time: &time,
        initial_high: false,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.read_bit(), Err(DhtError::BitHighDidNotEnd));
}

fn data_transitions(bytes: &[u8], start_us: u64) -> Vec<(u64, bool)> {
    let mut transitions = Vec::new();
    let mut time = start_us;

    for &byte in bytes {
        for bit_index in (0..8).rev() {
            let bit = (byte >> bit_index) & 1;

            // Each bit starts with 50 us LOW.
            time += 50;
            transitions.push((time, true));

            // HIGH pulse determines the bit value.
            time += if bit == 0 { 28 } else { 70 };
            transitions.push((time, false));
        }
    }

    transitions
}

#[test]
fn reads_complete_byte() {
    let time = Cell::new(0);
    let transitions = data_transitions(&[0b1010_0101], 0);

    let pin = TimedPin {
        time: &time,
        initial_high: false,
        transitions: &transitions,
    };

    let clock = SharedClock {
        time: &time,
        step_us: 1,
    };

    let mut sensor = Dht11::new(pin, clock);

    assert_eq!(sensor.read_byte(), Ok(0b1010_0101));
}
