# ferrum-dht11

Portable, hardware-independent `no_std` driver for DHT11 temperature and
humidity sensors. The application supplies GPIO, timing, and async delay
implementations through the driver's hardware traits.

This crate is experimental. Its async API currently performs blocking pulse
polling, and timing behavior depends on the supplied hardware implementations.

Licensed under `MIT OR Apache-2.0`; see `LICENSE-MIT` and `LICENSE-APACHE`.
