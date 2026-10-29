# Ferrum Drivers — Development Runbook

Practical commands for building, testing, flashing, and debugging Ferrum Drivers.

This runbook documents verified development workflows and serves as a reference for future hardware experiments.

## 1. Repository layout

```text
ferrum-drivers/
├── Cargo.toml                # Core workspace
├── crates/
│   ├── ferrum-dht11/
│   └── ferrum-nrf24/
├── platforms/
│   ├── rp2350/               # Independent RP2350 workspace
│   └── esp32/                # Independent ESP32 workspace
├── docs/
│   ├── adr/
│   ├── audits/
│   └── runbooks/
└── firmware/                 # Generated firmware artifacts
```

**Important:** Core, RP2350, and ESP32 are independent Cargo workspaces. Run commands from the indicated directory.

See [ADR-001](../adr/001-platform-oriented-workspaces.md) for the architectural rationale.

## 2. Host development — Core workspace

Working directory: repository root.

### Formatting

```bash
cargo fmt --all
cargo fmt --all -- --check
```

### Unit and integration tests

```bash
cargo test --workspace
```

Run only the DHT11 tests:

```bash
cargo test -p ferrum-dht11
```

### Check the core driver

```bash
cargo check -p ferrum-dht11 --lib
```

The core driver is hardware-independent. Do not use platform-specific features such as `rp2350` when building `ferrum-dht11`.

## 3. Raspberry Pi Pico 2 — RP2350

Hardware:

- Raspberry Pi Pico 2
- DHT11 temperature/humidity sensor
- DHT11 DATA connected to GP15

Target:

`thumbv8m.main-none-eabihf`

### 3.1 Check the platform library

```bash
cd platforms/rp2350

cargo check \
  --lib \
  --features dht11 \
  --target thumbv8m.main-none-eabihf
```

### 3.2 Build the DHT11 example

```bash
cargo build \
  --example pico2_dht11 \
  --features dht11 \
  --target thumbv8m.main-none-eabihf \
  --release
```

### 3.3 Convert ELF to UF2

Use `picotool`:

```bash
picotool uf2 convert \
  target/thumbv8m.main-none-eabihf/release/examples/pico2_dht11 \
  -t elf \
  firmware/pico2_dht11.uf2
```

Ensure that the `firmware/` directory exists beforehand.

**Note:** `elf2uf2-rs` was previously attempted, but `picotool` is the preferred workflow for Pico 2 / RP2350.

### 3.4 Flash the Pico 2

1. Disconnect the Pico 2.
2. Hold BOOTSEL while reconnecting USB.
3. Wait for the `RP2350` mass-storage device to appear.
4. Copy the generated UF2 file to the mounted device.
5. The Pico 2 should automatically reboot into the application.

### 3.5 Observe measurements

Open the USB CDC serial port with a compatible serial monitor.

The example reports temperature and relative humidity at approximately two-second intervals.

## 4. Seeed XIAO ESP32-C3

Hardware:

- Seeed XIAO ESP32-C3
- DHT11 sensor
- DHT11 DATA connected to GPIO3 (board pin D1)
- 3.3 V supply and common ground
- External pull-up on DATA if not already present on the sensor module

Target:

`riscv32imc-unknown-none-elf`

### 4.1 Check the platform library

```bash
cd platforms/esp32

cargo check \
  --lib \
  --features esp32c3,dht11 \
  --target riscv32imc-unknown-none-elf \
  --release
```

### 4.2 Build the DHT11 example

```bash
cargo build \
  --example esp32c3_dht11 \
  --features esp32c3,dht11 \
  --target riscv32imc-unknown-none-elf \
  --release
```

### 4.3 Flash and monitor

```bash
cargo run \
  --example esp32c3_dht11 \
  --features esp32c3,dht11 \
  --target riscv32imc-unknown-none-elf \
  --release
```

The Cargo runner uses:

```toml
[target.riscv32imc-unknown-none-elf]
runner = "espflash flash --monitor --chip esp32c3"
```

The monitor displays readings through the ESP32-C3 USB Serial/JTAG interface.

### 4.4 Expected serial output

Example of successful physical measurements:

```text
Ferrum DHT11 - ESP32-C3
Sensor: GPIO3 (XIAO D1)
Reading every 2 seconds...
Humidity: 68.0 % | Temperature: 23.8 C
Humidity: 67.0 % | Temperature: 23.8 C
```

### 4.5 ESP32 troubleshooting

**Error: `esp_hal::main` or `esp_hal::init` unavailable**

The firmware requires the `esp-hal/rt` feature.

**Error: undefined symbols such as `WIFI_MAC`, `GPIO`, or `SPI2`**

Check that the platform's build script includes:

```rust
println!("cargo:rustc-link-arg=-Tlinkall.x");
```

**Error: ESP-IDF App Descriptor missing**

Ensure the example includes:

```rust
esp_bootloader_esp_idf::esp_app_desc!();
```

**Error: conflicting `esp-println` features**

Select exactly one transport feature, such as `jtag-serial`, and disable incompatible default features.

**Error: `esp-backtrace` missing output backend**

Enable `println` alongside `panic-handler`.

## 5. Full validation checklist

Run from the repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

Run from `platforms/rp2350`:

```bash
cargo fmt --all -- --check

cargo check --lib \
  --features dht11 \
  --target thumbv8m.main-none-eabihf

cargo build \
  --example pico2_dht11 \
  --features dht11 \
  --target thumbv8m.main-none-eabihf \
  --release
```

Run from `platforms/esp32`:

```bash
cargo fmt --all -- --check

cargo check --lib \
  --features esp32c3,dht11 \
  --target riscv32imc-unknown-none-elf \
  --release

cargo build \
  --example esp32c3_dht11 \
  --features esp32c3,dht11 \
  --target riscv32imc-unknown-none-elf \
  --release
```

## 6. Development notes

- Prefer `--release` for timing-sensitive ESP32 examples.
- A successful build does not imply successful physical hardware validation.
- The current DHT11 adapters use blocking operations within the async interface.
- Do not use `cargo test --workspace` at the repository root to assume that the independent platform workspaces have been tested.
- Check the DHT11 audit before changing timing thresholds or GPIO behavior.

## 7. References

- [DHT11 documentation](../dht11.md)
- [ADR-001: Platform-Oriented Workspaces](../adr/001-platform-oriented-workspaces.md)
- [DHT11 audit](../audits/ferrum-dht11.md)
