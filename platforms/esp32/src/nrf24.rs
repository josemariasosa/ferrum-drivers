//! ESP32 blocking integration for the portable nRF24L01+ driver.
//!
//! Accepts an externally configured SPI device and CE pin.
//! Owns the delay provider required by the portable driver.
//!
//! This module does not configure SPI pins or chip select.
//! Those responsibilities belong to the caller when using
//! `from_spi_device`.

use embedded_hal::{digital::OutputPin, spi::SpiDevice};

use esp_hal::delay::Delay;

pub use ferrum_nrf24::{DataRate, InitError, RadioConfig, RadioState, RxError, TxError, TxPower};

use ferrum_nrf24::Nrf24 as PortableNrf24;

/// Blocking nRF24 integration for ESP32.
///
/// The SPI device may use an exclusive or shared bus.
/// The caller is responsible for supplying a valid SPI
/// configuration and a correctly configured CE output.
pub struct Nrf24<SPI, CE> {
    inner: PortableNrf24<SPI, CE>,
    delay: Delay,
}

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Construct using externally configured hardware.
    ///
    /// SPI must be Mode 0, MSB-first, at no more than 10 MHz.
    /// The SPI device must manage CSN.
    ///
    /// This function performs no radio I/O.
    pub fn from_spi_device(spi: SPI, ce: CE) -> Self {
        Self {
            inner: PortableNrf24::new(spi, ce),
            delay: Delay::new(),
        }
    }

    /// Initialize and verify the radio.
    pub fn initialize(
        &mut self,
        config: &RadioConfig,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.inner.initialize(config, &mut self.delay)
    }

    /// Enter reception mode.
    pub fn start_listening(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.inner.start_listening(&mut self.delay)
    }

    /// Stop reception and enter Standby.
    pub fn standby(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.inner.standby()
    }

    /// Transmit one dynamic-length payload.
    pub fn transmit(&mut self, payload: &[u8]) -> Result<(), TxError<SPI::Error, CE::Error>> {
        self.inner.transmit(payload, &mut self.delay)
    }

    /// Poll the receive FIFO.
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, RxError<SPI::Error>> {
        self.inner.receive(buffer)
    }

    /// Enter PowerDown from Standby.
    pub fn power_down(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.inner.power_down()
    }

    /// Return from PowerDown to Standby.
    pub fn power_up(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.inner.power_up(&mut self.delay)
    }

    /// Return the portable driver's logical state.
    pub fn state(&self) -> RadioState {
        self.inner.state()
    }

    /// Consume the wrapper and return SPI and CE.
    ///
    /// Does not reset or power down the physical radio.
    pub fn release(self) -> (SPI, CE) {
        self.inner.release()
    }
}
