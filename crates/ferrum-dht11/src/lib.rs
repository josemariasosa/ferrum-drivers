//! A hardware-independent, `no_std` driver for the DHT11 temperature and
//! humidity sensor.
//!
//! The application supplies a [`DhtPin`] for the bidirectional DATA line, a
//! [`MicrosecondClock`] for pulse timing, and an asynchronous delay for the
//! host start signal. The driver reads the sensor's 40-bit frame and validates
//! its checksum; it does not configure a board's GPIO, pull-up, or timer.
//!
//! # Usage
//!
//! This conceptual example uses board-specific values implementing the
//! required traits. It is ignored because those implementations depend on the
//! application's HAL and hardware configuration.
//!
//! ```ignore
//! let mut sensor = Dht11::new(data_pin, clock);
//! let reading = sensor.read(&mut delay).await?;
//! ```
//!
//! DATA requires an external pull-up to a voltage compatible with both the
//! sensor and MCU. Do not pull a 3.3 V MCU input up to 5 V unless the input is
//! explicitly 5 V tolerant; use a suitable level shifter when needed.
//!
//! The driver has not been verified on physical hardware. Pulse timing depends
//! on the supplied clock and GPIO implementation, and pulse observation is a
//! synchronous busy-polling operation.
#![no_std]

mod driver;
mod hardware;
mod protocol;

pub use driver::Dht11;
pub use hardware::{DhtPin, MicrosecondClock};
pub use protocol::{DhtError, Measurement};
