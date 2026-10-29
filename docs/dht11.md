# DHT11 Temperature and Humidity Sensor

`ferrum-dht11` is a `no_std` driver with a hardware-independent protocol core
and platform-specific adapters. It is developer-reported as physically
validated on the platforms listed below; see the [audit](audits/ferrum-dht11.md)
for the evidence and its limitations.

## Supported platforms

| Board | Platform workspace / crate | Example | DATA pin | Features | Target |
|---|---|---|---|---|---|
| Raspberry Pi Pico 2 (RP2350) | `platforms/rp2350` / `ferrum-rp2350` | `examples/pico2_dht11.rs` | GP15 | `dht11` | `thumbv8m.main-none-eabihf` |
| Seeed XIAO ESP32-C3 | `platforms/esp32` / `ferrum-esp32` | `examples/esp32c3_dht11.rs` | GPIO3 (board pin D1) | `esp32c3,dht11` | `riscv32imc-unknown-none-elf` |

The ESP32-C3 adapter uses `esp-hal 1.2.2`.

## Wiring

For either board, connect sensor VCC to **3.3 V** and sensor GND to board GND
(common ground). Connect DATA to the board's DATA pin in the table above.
DATA requires a pull-up to 3.3 V unless the sensor module already includes a
suitable pull-up. Do not drive an ESP32-C3 GPIO with a 5 V signal.

## Usage

The platform workspaces are independent. From the repository root, build the
examples with the commands below.

### Raspberry Pi Pico 2

```sh
cd platforms/rp2350
cargo build \
  --example pico2_dht11 \
  --features dht11 \
  --target thumbv8m.main-none-eabihf \
  --release
```

After initializing the RP2350 GPIO pins and TIMER0, use the platform adapter:

```rust
use ferrum_rp2350::dht11::Dht11;

let mut sensor = Dht11::new(pins.gpio15, timer);
let measurement = sensor.read().await?;
```

### Seeed XIAO ESP32-C3

```sh
cd platforms/esp32
cargo build \
  --example esp32c3_dht11 \
  --features esp32c3,dht11 \
  --target riscv32imc-unknown-none-elf \
  --release
```

After ESP32-C3 initialization, GPIO3 is consumed by the adapter. The example
uses `block_on` to read a measurement:

```rust
use embassy_futures::block_on;
use ferrum_esp32::dht11::Dht11;

let mut sensor = Dht11::new(peripherals.GPIO3);
let measurement = block_on(sensor.read())?;
```

The current adapters perform blocking timing operations inside the async
reading API. The DHT11 is a slow sensor; allow at least two seconds between
reads. For workspace checks, flashing, and monitoring instructions, see the
[development runbook](runbooks/development.md).

## Measurements

A successful read returns four transmitted components:

```rust
pub struct Measurement {
    pub humidity_integer: u8,
    pub humidity_decimal: u8,
    pub temperature_integer: u8,
    pub temperature_decimal: u8,
}
```

Use both integer and decimal components when displaying values. Failed reads
return a `DhtError`.
