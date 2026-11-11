#![no_std]

//! Ferrum Drivers platform adapters for Espressif microcontrollers.
//!
//! Platform-specific sensor drivers will be available through
//! Cargo features.

#[cfg(not(any(feature = "esp32c3", feature = "esp32s3")))]
compile_error!("Select an ESP32 chip feature: `esp32c3` or `esp32s3`.");

#[cfg(feature = "dht11")]
pub mod dht11;

#[cfg(feature = "nrf24")]
pub mod nrf24;
