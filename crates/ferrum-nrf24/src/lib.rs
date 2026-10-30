//! A hardware-independent `no_std` blocking driver for the nRF24L01+
//! transceiver, built on [`embedded_hal`] 1.0 SPI-device, GPIO output,
//! and delay traits.
//!
//! [`Nrf24::initialize`] configures RX pipe 0 and enters
//! [`RadioState::Standby`] with CE low.
//!
//! [`Nrf24::start_listening`] explicitly enables reception.
//! [`Nrf24::standby`] stops listening.
//!
//! [`Nrf24::transmit`] sends one payload from Standby or Listening
//! and restores the previous stable state on success.
//!
//! [`Nrf24::receive`] polls the RX FIFO without waiting for a packet
//! and copies at most one dynamic-length payload into a caller-owned
//! buffer.
//!
//! [`Nrf24::power_down`] and [`Nrf24::power_up`] provide explicit
//! power-management transitions between Standby and PowerDown.
//!
//! The driver supports channels 0–125, five-byte addresses, 250 kbit/s,
//! 1 Mbit/s or 2 Mbit/s, four nominal TX power levels, one-byte CRC, and
//! dynamic payload lengths of 1–32 bytes on pipe 0. TX uses selective NO_ACK:
//! it does not request an acknowledgment or automatically retransmit. The
//! receiver still enables pipe-0 auto-ack as required by the hardware's
//! dynamic-payload configuration; NO_ACK frames themselves do not request an
//! acknowledgment. Other acknowledgment policies are not configurable.
//!
//! Calls perform synchronous SPI transactions. Receive polling does not
//! allocate, change CE, or wait for a packet, although an individual SPI call
//! can block according to the supplied HAL. Configuration is read back during
//! initialization. Critical SPI, GPIO, and uncertain-operation failures put
//! the driver into a fail-stop [`RadioState::Faulted`] state; callers must
//! release the resources and manage hardware recovery themselves. Invalid
//! arguments, an undersized receive buffer, and a successfully recovered
//! invalid payload width do not by themselves fault the driver.
//!
//! The crate does not implement interrupt-driven or asynchronous operation,
//! additional RX pipes, payload encoding, or automatic recovery. Correct
//! operation assumes compatible radios, matching RF settings and addresses,
//! appropriate power/reset sequencing, and correctly configured SPI Mode 0,
//! MSB-first. Unit tests use mocked HAL interactions; they do not establish
//! physical-radio compatibility or over-the-air delivery.
//!
//! # Example
//!
//! The concrete SPI device, CE output pin, and delay provider come from the
//! application HAL. This example shows the driver calls, not a board pin
//! assignment or proof of hardware operation:
//!
//! ```
//! use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiDevice};
//! use ferrum_nrf24::{Nrf24, RadioConfig};
//!
//! fn exchange_example<SPI, CE, D>(spi: SPI, ce: CE, delay: &mut D)
//! where
//!     SPI: SpiDevice,
//!     CE: OutputPin,
//!     D: DelayNs,
//! {
//!     let config = RadioConfig::new(50, [0xE7; 5]).expect("valid channel");
//!     let mut radio = Nrf24::new(spi, ce);
//!     radio.initialize(&config, delay).expect("radio initialization");
//!
//!     radio.transmit(&[0x01, 0x02], delay).expect("transmit");
//!
//!     let mut buffer = [0u8; 32];
//!     if let Some(length) = radio.receive(&mut buffer).expect("receive") {
//!         let received_payload = &buffer[..length];
//!         let _ = received_payload;
//!     }
//! }
//! ```

#![no_std]

mod config;
mod driver;
mod error;
mod registers;

pub use config::{DataRate, RadioConfig, TxPower};
pub use driver::{Nrf24, RadioState};
pub use error::{
    ConfigError, ConfigureError, InitError, PayloadError, RegisterError, RxError, TxError,
};
