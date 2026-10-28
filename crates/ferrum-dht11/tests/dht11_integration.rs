use ferrum_dht11::*;

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

fn full_transaction(frame: [u8; 5]) -> Vec<(u64, bool)> {
    let mut transitions = std::vec![
        (20, false),  // Sensor starts responding
        (100, true),  // 80 us LOW
        (180, false), // 80 us HIGH
    ];

    transitions.extend(data_transitions(&frame, 180));

    transitions
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
fn reads_complete_measurement() {
    let time = Cell::new(0);

    let transitions = full_transaction([55, 0, 24, 0, 79]);

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

    let expectations = [Transaction::async_delay_us(18_000)];

    let mut delay = CheckedDelay::new(&expectations);

    let measurement = block_on(sensor.read(&mut delay)).unwrap();

    delay.done();

    assert_eq!(measurement.humidity_integer, 55);
    assert_eq!(measurement.humidity_decimal, 0);
    assert_eq!(measurement.temperature_integer, 24);
    assert_eq!(measurement.temperature_decimal, 0);
}
