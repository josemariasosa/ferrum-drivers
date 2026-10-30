//! DHT11 hardware adapter for Espressif microcontrollers.

use embedded_hal::delay::DelayNs as SyncDelayNs;
use embedded_hal_async::delay::DelayNs as AsyncDelayNs;

use esp_hal::delay::Delay;
use esp_hal::time::Instant;

use ferrum_dht11::{DhtPin, MicrosecondClock};

use esp_hal::gpio::{DriveMode, Flex, InputConfig, OutputConfig, Pull};

// ---- CLOCK ADAPTER ----

struct EspClock;

impl MicrosecondClock for EspClock {
    fn now_us(&self) -> u64 {
        Instant::now().duration_since_epoch().as_micros()
    }
}

// ---- DELAY ADAPTER ----

struct EspDelay {
    delay: Delay,
}

impl EspDelay {
    fn new() -> Self {
        Self {
            delay: Delay::new(),
        }
    }
}

impl AsyncDelayNs for EspDelay {
    async fn delay_ns(&mut self, ns: u32) {
        SyncDelayNs::delay_ns(&mut self.delay, ns);
    }
}

// ---- GPIO ADAPTER ----

struct EspPin<'d> {
    pin: Flex<'d>,
}

impl<'d> EspPin<'d> {
    fn new(pin: impl esp_hal::gpio::Pin + 'd) -> Self {
        let mut pin = Flex::new(pin);

        // Configure open-drain output.
        pin.apply_output_config(&OutputConfig::default().with_drive_mode(DriveMode::OpenDrain));

        // Enable input sampling while using the output driver.
        pin.apply_input_config(&InputConfig::default().with_pull(Pull::None));
        pin.set_input_enable(true);

        // HIGH releases DATA in open-drain mode.
        pin.set_high();
        pin.set_output_enable(true);

        Self { pin }
    }
}

impl DhtPin for EspPin<'_> {
    type Error = ();

    fn drive_low(&mut self) -> Result<(), Self::Error> {
        self.pin.set_low();
        Ok(())
    }

    fn release(&mut self) -> Result<(), Self::Error> {
        // HIGH in open-drain mode releases the line.
        self.pin.set_high();
        Ok(())
    }

    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.pin.is_high())
    }
}

// ---- PUBLIC DRIVER ----

/// Ready-to-use DHT11 driver for ESP32 microcontrollers.
///
/// Configures the GPIO for open-drain communication.
pub struct Dht11<'d> {
    inner: ferrum_dht11::Dht11<EspPin<'d>, EspClock>,
    delay: EspDelay,
}

impl<'d> Dht11<'d> {
    /// Create a DHT11 driver using a compatible GPIO.
    pub fn new(pin: impl esp_hal::gpio::Pin + 'd) -> Self {
        let pin = EspPin::new(pin);

        Self {
            inner: ferrum_dht11::Dht11::new(pin, EspClock),
            delay: EspDelay::new(),
        }
    }

    /// Read temperature and humidity.
    ///
    /// Note: The current implementation uses blocking
    /// delays and active polling internally.
    pub async fn read(&mut self) -> Result<ferrum_dht11::Measurement, ferrum_dht11::DhtError> {
        self.inner.read(&mut self.delay).await
    }
}
