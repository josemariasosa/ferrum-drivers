
//! RP2350 platform support for the DHT11 sensor.
//!
//! Provides a ready-to-use adapter for the Raspberry Pi Pico 2,
//! initially supporting GPIO15 and TIMER0.

use embedded_hal::delay::DelayNs as SyncDelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs as AsyncDelayNs;

use rp235x_hal as hal;

use crate::hardware::{DhtPin, MicrosecondClock};
use crate::{DhtError, Measurement};

/// GPIO15 configured as a floating input.
pub type DhtInputPin = hal::gpio::Pin<
    hal::gpio::bank0::Gpio15,
    hal::gpio::FunctionSio<hal::gpio::SioInput>,
    hal::gpio::PullNone,
>;

/// RP2350 TIMER0.
pub type DhtTimer = hal::Timer<hal::timer::CopyableTimer0>;

// ---- GPIO ADAPTER ----

struct PicoPin {
    pin: hal::gpio::InOutPin<DhtInputPin>,
}

impl DhtPin for PicoPin {
    type Error = ();

    fn drive_low(&mut self) -> Result<(), Self::Error> {
        self.pin.set_low().map_err(|_| ())
    }

    fn release(&mut self) -> Result<(), Self::Error> {
        // Open-drain HIGH releases the data line.
        self.pin.set_high().map_err(|_| ())
    }

    fn is_high(&mut self) -> Result<bool, Self::Error> {
        self.pin.is_high().map_err(|_| ())
    }
}

// ---- CLOCK ADAPTER ----

struct PicoClock {
    timer: DhtTimer,
}

impl MicrosecondClock for PicoClock {
    fn now_us(&self) -> u64 {
        self.timer.get_counter().ticks()
    }
}

// ---- DELAY ADAPTER ----

struct PicoDelay {
    timer: DhtTimer,
}

impl AsyncDelayNs for PicoDelay {
    async fn delay_ns(&mut self, ns: u32) {
        SyncDelayNs::delay_ns(&mut self.timer, ns);
    }
}

// ---- PUBLIC DRIVER ----

/// Ready-to-use DHT11 driver for RP2350 TIMER0 and GPIO15.
pub struct Dht11 {
    inner: crate::Dht11<PicoPin, PicoClock>,
    delay: PicoDelay,
}

impl Dht11 {
    /// Create a DHT11 driver from GPIO15 and an initialized TIMER0.
    pub fn new(pin: DhtInputPin, timer: DhtTimer) -> Self {
        let pin = PicoPin {
            pin: hal::gpio::InOutPin::new(pin),
        };

        let clock = PicoClock { timer };
        let delay = PicoDelay { timer };

        Self {
            inner: crate::Dht11::new(pin, clock),
            delay,
        }
    }

    /// Read and validate a temperature/humidity measurement.
    pub async fn read(&mut self) -> Result<Measurement, DhtError> {
        self.inner.read(&mut self.delay).await
    }
}
