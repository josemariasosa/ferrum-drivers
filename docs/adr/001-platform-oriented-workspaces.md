# ADR-001: Platform-Oriented Workspaces

- **Status:** Accepted
- **Date:** 2026-10-28
- **Scope:** Ferrum Drivers repository architecture

## Context

Ferrum Drivers is a collection of reusable Rust embedded drivers.

Our goal is to make embedded development approachable for beginners while keeping protocol implementations portable across microcontrollers.

Initially, `ferrum-dht11` provided platform support through Cargo features:

```toml
ferrum-dht11 = { features = ["rp2350"] }
```

This approach worked for Raspberry Pi Pico 2.

However, adding support for ESP32-C3 introduced a dependency resolution conflict between `rp235x-hal` and `esp-hal`. Both dependency graphs require incompatible versions of `riscv-rt` that declare the same Cargo `links` value.

Optional dependencies and target-specific compilation did not eliminate this conflict within the shared workspace dependency resolution.

We therefore need an architecture that allows multiple MCU families to coexist without coupling their HAL dependencies.

## Decision

We organize Ferrum Drivers into two layers:

1. **Hardware-independent drivers**, containing protocol logic, public interfaces, error types, and tests.
2. **Platform libraries**, containing hardware adapters, firmware examples, and MCU-specific dependencies.

Each platform maintains an independent Cargo workspace and lockfile.

### Repository structure

```text
ferrum-drivers/
├── Cargo.toml                  # Core workspace
├── crates/
│   ├── ferrum-dht11/
│   └── ferrum-nrf24/
├── platforms/
│   ├── rp2350/
│   │   ├── Cargo.toml          # ferrum-rp2350
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── dht11.rs
│   │   └── examples/
│   │       └── pico2_dht11.rs
│   └── esp32/
│       ├── Cargo.toml          # ferrum-esp32
│       ├── src/
│       │   ├── lib.rs
│       │   └── dht11.rs
│       └── examples/
│           └── esp32c3_dht11.rs
└── docs/
```

The root workspace includes the hardware-independent crates. Platform workspaces depend on these crates through local path dependencies.

### Cargo features

Platform crates select supported drivers through features.

For RP2350:

```toml
[features]
default = []
dht11 = ["dep:ferrum-dht11"]
```

For ESP32, features select both the target chip and the required drivers:

```toml
[features]
default = []
esp32c3 = ["esp-hal/esp32c3"]
dht11 = ["dep:ferrum-dht11"]
```

The ESP32-C3 DHT11 adapter and firmware example are implemented in the
independent `platforms/esp32` workspace. They use the shared
hardware-independent driver and `esp-hal 1.2.2`.

A platform can eventually support multiple sensors without requiring a separate crate for every sensor–MCU combination.

## Public API

Platform-specific complexity should remain inside the platform library.

For example, the RP2350 integration allows:

```rust
use ferrum_rp2350::dht11::Dht11;

let mut sensor = Dht11::new(pins.gpio15, timer);
let measurement = sensor.read().await?;
```

Users do not need to implement GPIO traits or manually decode the DHT11 protocol.

Developers supporting new hardware may use the generic driver interfaces directly.

## Consequences

### Benefits

- Protocol logic is shared across platforms.
- MCU-specific HAL dependencies remain isolated.
- Core drivers can be tested on a development computer.
- Platform crates can provide beginner-friendly APIs.
- Each platform can integrate multiple drivers.
- Individual crates remain independently publishable to crates.io.

### Trade-offs

- Multiple Cargo workspaces and lockfiles must be maintained.
- Checks and formatting must be executed separately for each workspace.
- Shared dependency versions may require coordination.
- Platform packages must explicitly declare their own metadata and dependencies.
- Platform features and supported chips must be documented and tested independently.

## Alternatives considered

**Platform features inside each driver**

Simple for users, but resulted in incompatible HAL dependency graphs during Cargo resolution.

**One adapter crate per sensor and MCU**

Provides dependency isolation, but scales poorly as more drivers and platforms are introduced.

**One workspace for all platforms**

Simplifies repository commands, but reintroduces the dependency-resolution conflict.

We chose platform-oriented workspaces because they balance dependency isolation, reuse, and user-facing simplicity.

## Implementation rules

The following rules guide future development:

1. Core crates must not depend on MCU-specific HAL implementations.
2. Platform-specific GPIO, clock, and delay implementations belong in `platforms/`.
3. Platform libraries may depend on multiple core drivers.
4. Driver integrations should be enabled through optional Cargo features.
5. Platform examples should demonstrate real hardware usage.
6. Changes to a platform adapter should not require modifications to the corresponding protocol solely to accommodate that MCU.
7. Each workspace must be compiled and tested independently.
8. Prefer small, understandable public APIs over additional abstractions.

## Validation

As of October 28, 2026:

- `ferrum-dht11` has passing host tests.
- The RP2350 DHT11 adapter and firmware compile successfully.
- DHT11 measurements have been observed on a physical Raspberry Pi Pico 2.
- The ESP32-C3 DHT11 adapter and firmware example compile successfully in
  release mode in the independent `ferrum-esp32` workspace.
- The developer reports flashing and executing the ESP32-C3 firmware on a
  Seeed XIAO ESP32-C3 using GPIO3 (board pin D1), with six consecutive
  successful readings. This is not a long-duration stability test, precision
  calibration, or proof of timing robustness.

## Future work

Extend platform integrations to other drivers, including nRF24, as their hardware adapters become available.

Revisit this ADR if a materially simpler dependency-isolation strategy becomes practical.