mod init;
mod rx;
mod spi;
mod tx;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::{Operation, SpiDevice};

use crate::config::RadioConfig;
use crate::error::{ConfigureError, InitError, PayloadError, RegisterError, RxError, TxError};
use crate::registers::*;

/// nRF24L01+ driver owning the SPI device and CE output pin.
///
/// Starts in [`RadioState::Uninitialized`]. Call [`Self::initialize`]
/// before attempting to transmit or receive.
///
/// SPI chip-select (CSN) is managed by the supplied `SpiDevice`.
pub struct Nrf24<SPI, CE> {
    spi: SPI,
    ce: CE,
    state: RadioState,
}

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Creates a driver without accessing the hardware.
    ///
    /// Takes ownership of an `embedded-hal` 1.0 [`SpiDevice`] (including its
    /// CSN management) and a separate CE [`OutputPin`]. Configure SPI Mode 0,
    /// MSB-first, at no more than 10 MHz before calling. No pin level is set.
    /// The initial state is [`RadioState::Uninitialized`], with no known mode.
    pub fn new(spi: SPI, ce: CE) -> Self {
        Self {
            spi,
            ce,
            state: RadioState::Uninitialized,
        }
    }

    /// Consumes the driver and returns ownership of SPI and CE.
    ///
    /// Valid in every state, including [`RadioState::Faulted`]. Performs no
    /// hardware I/O: CE, power, FIFOs, and configuration are left as they were.
    /// The caller must establish safe hardware conditions before reuse; creating
    /// a new driver around these resources does not reset the transceiver.
    pub fn release(self) -> (SPI, CE) {
        (self.spi, self.ce)
    }

    /// Returns the software state without inspecting hardware or changing CE.
    pub fn state(&self) -> RadioState {
        self.state
    }
}

/// Logical operating state of the radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    /// The radio has not been initialized.
    Uninitialized,

    /// Powered and configured, with CE low.
    Standby,

    /// Powered and actively receiving packets.
    Listening,

    /// Radio powered down while retaining register configuration.
    PowerDown,

    /// The hardware state cannot be reliably established.
    Faulted,
}
