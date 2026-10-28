#![no_std]

//! Ferrum Drivers platform adapters for Espressif microcontrollers.
//!
//! Platform-specific sensor drivers will be available through
//! Cargo features.

#[cfg(not(feature = "esp32c3"))]
compile_error!("Select an ESP32 chip feature, e.g. `esp32c3`.");
