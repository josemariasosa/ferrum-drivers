# DHT11 Temperature & Humidity Sensor

A lightweight, `no_std` Rust driver for reading temperature and relative humidity from a DHT11 sensor.

`ferrum-dht11` separates the sensor protocol from hardware-specific implementations, making it easy to use on supported microcontrollers.

## Supported hardware

| Platform                     | Status             | Interface     |
|------------------------------|--------------------|---------------|
| Raspberry Pi Pico 2 (RP2350) | Tested on hardware | GPIO + TIMER0 |

The RP2350 implementation supports compatible GPIO pins. GPIO15 is used in the example below.

## Wiring

Connect the DHT11 sensor to your Raspberry Pi Pico 2:

| DHT11 | Pico 2 |
|---|---|
| VCC | 3.3V |
| DATA | GP15 (physical pin 20) |
| GND | GND |

The DATA line requires a pull-up resistor, typically 4.7–10 kΩ, to 3.3V. Many DHT11 modules already include one.

## Usage

Enable the RP2350 platform feature in your project's `Cargo.toml`:

```toml
[dependencies]
ferrum-dht11 = { path = "../ferrum-dht11", features = ["rp2350"] }
```

The path above is an example for a local checkout. Adjust it to match your project structure.

Once the RP2350 clocks, GPIO pins, and TIMER0 have been initialized:

```rust
use ferrum_dht11::rp2350::Dht11;

// Select any compatible GPIO pin.
let mut sensor = Dht11::new(pins.gpio15, timer);

// Read temperature and humidity.
let measurement = sensor.read().await?;

let temperature = measurement.temperature_integer;
let humidity = measurement.humidity_integer;
```

The driver configures the GPIO internally, handles the DHT11 communication sequence, and validates the received checksum.

No custom GPIO traits or manual pulse decoding are required.

## Measurements

A successful read returns a `Measurement`:

```rust
pub struct Measurement {
    pub humidity_integer: u8,
    pub humidity_decimal: u8,
    pub temperature_integer: u8,
    pub temperature_decimal: u8,
}
```

For example:

```text
Humidity: 69.0 % | Temperature: 24.5 C
```

Use both integer and decimal fields when displaying values. On failure, `read()` returns a `DhtError` rather than an invalid measurement.

The DHT11 is a relatively slow sensor. Allow at least two seconds between readings.

## Raspberry Pi Pico 2 example

A complete working example is available at:

`crates/ferrum-dht11/examples/pico2_dht11.rs`

From the workspace root, compile it with:

```bash
cargo build -p ferrum-dht11 \
  --example pico2_dht11 \
  --features rp2350 \
  --target thumbv8m.main-none-eabihf \
  --release
```

Flash the firmware using your RP2350 flashing tool, then open the USB serial port.

Example output:

```text
Ferrum DHT11 - RP2350
Sensor: GPIO15
Reading every 2 seconds...
Humidity: 69.0 % | Temperature: 24.5 C
Humidity: 69.0 % | Temperature: 24.5 C
Humidity: 69.0 % | Temperature: 24.5 C
```

The example uses USB CDC to print measurements every two seconds and reports failed readings as warnings.

## Notes

- The library is `no_std` compatible.
- The generic protocol implementation is hardware-independent.
- The RP2350 adapter handles GPIO configuration and timer integration.
- The current RP2350 implementation uses blocking timing operations internally, even though its public reading method is asynchronous.
- Other microcontrollers can be supported by implementing additional platform adapters.