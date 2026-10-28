//! RP2350 platform support for the DHT11 sensor.
//!
//! Provides a ready-to-use adapter for the Raspberry Pi Pico 2,
//! supporting compatible RP2350 GPIO pins and TIMER0.

use embedded_hal::delay::DelayNs as SyncDelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs as AsyncDelayNs;

use rp235x_hal as hal;

use ferrum_dht11::{DhtError, DhtPin, Measurement, MicrosecondClock};

use hal::gpio::{
    Function, FunctionSioInput, FunctionSioOutput, Pin, PinId, PullNone, PullType, ValidFunction,
};

/// RP2350 TIMER0.
pub type DhtTimer = hal::Timer<hal::timer::CopyableTimer0>;

// ---- GPIO ADAPTER ----

struct PicoPin<P>
where
    P: hal::gpio::AnyPin<Function = hal::gpio::FunctionSioInput, Pull = hal::gpio::PullNone>,
{
    pin: hal::gpio::InOutPin<P>,
}

impl<P> DhtPin for PicoPin<P>
where
    P: hal::gpio::AnyPin<Function = hal::gpio::FunctionSioInput, Pull = hal::gpio::PullNone>,
{
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

/// Ready-to-use DHT11 driver for RP2350 GPIO and TIMER0.
pub struct Dht11<P>
where
    P: hal::gpio::AnyPin<Function = hal::gpio::FunctionSioInput, Pull = hal::gpio::PullNone>,
{
    inner: ferrum_dht11::Dht11<PicoPin<P>, PicoClock>,
    delay: PicoDelay,
}

// ---- CONSTRUCTOR ----

impl<I> Dht11<Pin<I, FunctionSioInput, PullNone>>
where
    I: PinId + ValidFunction<FunctionSioInput> + ValidFunction<FunctionSioOutput>,
{
    /// Create a DHT11 driver from a compatible RP2350 GPIO
    /// and an initialized TIMER0.
    ///
    /// The GPIO is automatically configured as a floating
    /// SIO input. An external pull-up is required on DATA.
    pub fn new<F, M>(pin: Pin<I, F, M>, timer: DhtTimer) -> Self
    where
        F: Function,
        M: PullType,
    {
        let pin = pin.into_floating_input();

        let pin = PicoPin {
            pin: hal::gpio::InOutPin::new(pin),
        };

        let clock = PicoClock { timer };
        let delay = PicoDelay { timer };

        Self {
            inner: ferrum_dht11::Dht11::new(pin, clock),
            delay,
        }
    }
}

// ---- SENSOR OPERATIONS ----

impl<P> Dht11<P>
where
    P: hal::gpio::AnyPin<Function = FunctionSioInput, Pull = PullNone>,
{
    /// Read and validate a temperature/humidity measurement.
    pub async fn read(&mut self) -> Result<Measurement, DhtError> {
        self.inner.read(&mut self.delay).await
    }
}
