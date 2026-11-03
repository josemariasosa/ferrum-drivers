//! ESP32 blocking integration for the portable nRF24L01+ driver.
//!
//! Supports both externally configured SPI devices and a ready-to-use SPI
//! master configuration. The wrapper owns the delay provider required by the
//! portable driver; the ready-to-use path also owns its configured SPI device,
//! CSN, and CE pins.

use embedded_hal::{digital::OutputPin, spi::SpiDevice};

use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{
        Level, Output, OutputConfig, OutputPin as GpioOutputPin,
        interconnect::{PeripheralInput, PeripheralOutput},
    },
    spi::{
        Mode,
        master::{Config as SpiConfig, ConfigError, Instance, Spi},
    },
    time::Rate,
};

pub use ferrum_nrf24::{DataRate, InitError, RadioConfig, RadioState, RxError, TxError, TxPower};

use embedded_hal_bus::spi::ExclusiveDevice;
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

impl<'d> Nrf24<ExclusiveDevice<Spi<'d, Blocking>, Output<'d>, Delay>, Output<'d>> {
    /// Configure an SPI master and GPIO pins for an nRF24 radio.
    ///
    /// SPI runs at 1 MHz in Mode 0. CSN starts high and CE starts low.
    /// This configures the hardware but does not initialize the radio.
    ///
    /// The SPI peripheral and pin parameters follow ESP-HAL's peripheral
    /// capabilities, so compatible SPI peripherals and GPIO pins can be used.
    pub fn new(
        spi: impl Instance + 'd,
        sck: impl PeripheralOutput<'d>,
        mosi: impl PeripheralOutput<'d>,
        miso: impl PeripheralInput<'d>,
        csn: impl GpioOutputPin + 'd,
        ce: impl GpioOutputPin + 'd,
    ) -> Result<Self, ConfigError> {
        let spi_config = SpiConfig::default()
            .with_frequency(Rate::from_mhz(1))
            .with_mode(Mode::_0);

        let spi_bus = Spi::new(spi, spi_config)?
            .with_sck(sck)
            .with_mosi(mosi)
            .with_miso(miso);

        let csn = Output::new(csn, Level::High, OutputConfig::default());
        let ce = Output::new(ce, Level::Low, OutputConfig::default());

        let spi_device = match ExclusiveDevice::new(spi_bus, csn, Delay::new()) {
            Ok(device) => device,
            Err(error) => match error {},
        };

        Ok(Self::from_spi_device(spi_device, ce))
    }
}
