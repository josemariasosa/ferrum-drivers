# ADR-003: Platform Resource Composition

- **Status:** Proposed
- **Date:** 2026-10-30
- **Scope:** Construction APIs for platform driver integrations

## Context

Ferrum Drivers separates portable device drivers from MCU-specific
integrations. [ADR-001](001-platform-oriented-workspaces.md) establishes
independent core and platform workspaces and calls for beginner-friendly
platform APIs. [ADR-002](002-nrf24-driver-architecture.md) describes the
portable nRF24 driver's lifecycle and plans platform integration through
platform crates.

The current DHT11 adapters illustrate a useful ready-to-use API: they configure
platform GPIO and keep hardware adaptation details out of application code.
The nRF24 examples, in contrast, configure SPI, GPIO, chip select, delays, and
an `embedded-hal-bus` `ExclusiveDevice` directly before constructing the
portable driver. The blocking nRF24 driver already accepts an
`embedded_hal::spi::SpiDevice`, so applications can supply a device assembled
from externally managed resources.

Developer-reported results now document physical nRF24 initialization on both
boards, deterministic Pico TX → ESP32 RX delivery, and sequence-correlated
FER–RUM communication in both directions. The evidence and limits are recorded
in the [nRF24 hardware audit](../audits/ferrum-nrf24.md); it was supplied for
this documentation update and was not independently reproduced here. Partial
logs do not meet the 100-exchange-per-direction validation criterion below.
These results do not imply that platform wrappers or shared-bus composition
have been implemented, and do not change this ADR's Proposed status.

## Decision drivers

- Make common beginner workflows straightforward without requiring manual
  setup of every peripheral resource.
- Preserve access to standard `embedded-hal` interfaces for advanced
  applications and ecosystem interoperability.
- Allow compatible shared-bus compositions without requiring exclusive
  ownership of an entire physical SPI bus.
- Keep MCU-specific configuration in platform crates and device behavior in
  portable drivers.
- Avoid premature abstractions, duplicated protocol logic, and unsupported
  hardware claims.

## Considered alternatives

### A — Device-specific convenience constructors

Each platform provides a simple constructor that configures the MCU resources
needed for a particular device.

**Advantages**

- Minimal implementation complexity.
- Easy to understand and maintain.
- Excellent initial developer experience.

**Limitations**

- May assume exclusive ownership of an SPI peripheral.
- Shared buses and multi-device arrangements may need additional APIs.
- A convenient integration can become restrictive if it hides resource
  assumptions or offers no composable path.

This remains appropriate for simple peripherals that do not benefit from
shared-bus composition, including the current DHT11 adapters. It may also be a
reasonable first implementation when the hardware and use case are inherently
single-device. It is not the general policy for devices where callers need to
compose SPI resources.

### B — Platform-level resource manager

Introduce a platform abstraction, such as a hypothetical `FerrumSpi`, to
construct and coordinate devices on a shared SPI bus.

**Advantages**

- Makes resource sharing a first-class concept.
- Centralizes bus configuration and coordination.
- Could be ergonomic for applications with several devices.

**Limitations**

- Adds ownership, lifetime, concurrency, and resource-policy complexity.
- Risks duplicating bus-sharing facilities already available through
  `embedded-hal-bus` and compatible HALs.
- A proprietary manager would be premature without a validated multi-device
  requirement.

Such a manager may be reconsidered if concrete applications demonstrate a
platform-wide need that standard `embedded-hal` composition cannot meet. It is
not selected as the initial policy.

### C — Progressive platform integration (selected)

When useful for a peripheral, a platform provides two complementary
construction paths:

1. A ready-to-use constructor that configures the relevant MCU peripheral,
   pins, and device-specific wiring.
2. A composable constructor accepting externally configured standard
   `embedded-hal` resources, such as a `SpiDevice`.

Both paths delegate device behavior to the same portable driver. They must
provide equivalent device functionality while allowing platform-specific
constructor signatures and resource lifetimes.

## Decision

Select **Alternative C — Progressive platform integration** as the general
direction for platform drivers where both ergonomic setup and resource
composition provide value. Beginner ergonomics and interoperability with the
Rust Embedded ecosystem are first-class goals; neither path should make the
other impossible.

The following conceptual nRF24 API shapes illustrate the two paths:

```rust,ignore
// Ready-to-use
let radio = Nrf24::new(
    spi_peripheral,
    sck,
    mosi,
    miso,
    csn,
    ce,
)?;

// Composable
let radio = Nrf24::from_spi_device(
    spi_device,
    ce,
)?;
```

These are **proposed API shapes, not currently implemented interfaces**.
They illustrate platform wrapper APIs, not a requirement to change the
portable driver's current `Nrf24::new(spi_device, ce)` API. Actual names,
signatures, and resource types must follow each HAL's conventions. A simple
device such as DHT11 is not required to expose two constructors when shared
resource composition offers no meaningful benefit.

## Rationale

Alternative A provides the clearest initial setup but, alone, can hide
ownership assumptions and make later composition awkward. Alternative B
elevates sharing but introduces framework-level policies before a demonstrated
need, despite existing ecosystem mechanisms. Alternative C retains A's
beginner-friendly path and supplies a standard-resource path where sharing
matters, without committing every driver or platform to a common manager.

The policy complements ADR-001's platform-oriented workspaces and API goal; it
does not change their dependency-isolation decisions. It complements
ADR-002's portable nRF24 behavior and platform-facing feature direction; it
does not revise its driver lifecycle or commit to any async backend. Neither
ADR requires platform constructors to own or coordinate every underlying
resource.

## Consequences and trade-offs

**Benefits**

- Common setup can be encapsulated in a platform wrapper.
- Applications can supply standard resources they have configured or
  composed themselves.
- The portable driver remains the single implementation of device protocol
  behavior.
- Platform APIs can evolve according to actual peripheral and HAL needs.

**Trade-offs**

- Where both paths are provided, they add API and testing surface.
- Platform constructors may differ because HAL ownership models differ.
- Supporting a composable path does not guarantee every possible bus-sharing
  arrangement, timing requirement, or peripheral combination.
- Hardware-specific constructors can still impose resource constraints that
  must be documented clearly.

## Ownership and resource-sharing constraints

Portable drivers own the device-specific logic: register access, protocol
behavior, state machines, and device error semantics. They remain independent
of concrete MCU HAL types and do not implement general MCU resource
management.

Platform crates may configure MCU peripherals and pins when doing so provides
meaningful ergonomic value. They adapt and pass resources to the portable
driver; they must not reimplement its register logic or state machine.

In the ready-to-use path, the wrapper may take ownership of an SPI controller
peripheral and pins to configure a bus, then own the resulting logical
`SpiDevice`, chip-select resource, CE pin, and any delay resource it needs.
The exact ownership and whether delay is stored internally or supplied by the
caller depend on the HAL and wrapper API.

In the composable path, the caller configures and owns the SPI bus and gives
the wrapper a logical `SpiDevice` plus device-specific resources such as CE.
Owning a `SpiDevice` is not the same as exclusively owning the physical SPI
controller: a `SpiDevice` can be a transaction-scoped view that arbitrates
access to a bus shared with other devices. The chosen implementation and
ownership types determine whether that composition is possible.

Shared-SPI support has distinct concerns:

- **Bus arbitration and chip select:** a sharing implementation must coordinate
  access to the physical bus and manage each device's chip select as required.
  `SpiDevice` expresses device transactions; it does not by itself promise a
  particular arbitration mechanism.
- **Mode and frequency:** these are bus configuration requirements. Devices
  with compatible requirements may share a bus configured to that SPI mode
  and frequency.
- **Per-device configuration switching:** `embedded-hal-bus` does not
  automatically switch SPI mode or frequency for each device. Different
  requirements may need HAL-specific support or dedicated SPI controllers.
- **Concurrency and transaction atomicity:** a sharing implementation must
  provide the synchronization appropriate to its execution environment.
  Individual `SpiDevice` transactions are the unit of device-level bus
  interaction; a sequence of separate driver operations is not necessarily
  atomic with respect to other devices.
- **Latency and real time:** arbitration, other devices' transactions, and
  interrupt or critical-section policies can add latency. Applications must
  evaluate whether that latency meets device timing and real-time constraints.

The portable nRF24 driver already has a `release()` method returning its SPI
device and CE pin. This does not establish a resource-recovery contract for
platform wrappers. Do not promise wrapper extraction, recovery, or peripheral
reinitialization APIs until ownership behavior is implemented and tested;
releasing a wrapper may not reset hardware or restore a safe device state.

An internally owned platform delay may simplify calls, but it must preserve
the portable driver's observable lifecycle, error propagation, and fault-state
semantics. A wrapper must not conceal failures with retries or alter behavior
without an explicit, separately reviewed decision.

## Implementation strategy

1. Complete bidirectional FER–RUM communication using the existing portable
   nRF24 API.
2. Establish physical regression evidence for both communication directions
   using deterministic payloads and documented radio configuration.
3. Implement an ESP32 nRF24 platform wrapper, beginning with the composable
   `SpiDevice` constructor.
4. Add its ready-to-use hardware constructor.
5. Implement the corresponding RP2350 integration using its HAL conventions.
6. Migrate the existing RP2350 and ESP32 nRF24 examples to the platform
   wrappers.
7. Re-run software and hardware validation and document the results and
   limitations.

Architectural acceptance of this ADR does not mean these integrations or
validation steps are complete. This ADR remains Proposed until reviewed;
implementation progress and hardware evidence do not change its status
automatically.

## Validation criteria

Before describing the migration as complete:

- Both platform wrappers compile in their independent workspaces for their
  documented targets and feature selections.
- The composable and ready-to-use paths on each platform exercise the same
  portable driver operations and produce the same documented lifecycle and
  error behavior.
- The existing nRF24 host tests continue to pass; add focused tests for
  wrapper construction, resource wiring, and error propagation where
  testable without hardware.
- Physical RP2350 ↔ ESP32-C3 tests deliver at least 100 deterministic payloads
  in **each** direction, with all 100 received byte-for-byte and no receive
  errors or mismatches in each direction. Record board, wiring, channel,
  address, payload, and observed counts. Compilation or radio initialization
  alone is not evidence of bidirectional communication.
- If shared-bus compatibility is claimed, validate it with a compatible
  sharing implementation and another bus device, confirming chip-select
  behavior and transaction integrity. Do not claim automatic mode or
  frequency switching unless the selected HAL explicitly implements and
  tests it.
- Update the relevant hardware-validation documentation with evidence and
  limitations; do not treat the existing contradictory status statements as
  resolved by this ADR alone.

## Deferred decisions

- Exact platform module, wrapper type, constructor names, signatures, and
  supported pin types.
- Whether each ready-to-use wrapper owns a delay implementation or accepts
  delay from its caller.
- Resource extraction and recovery semantics for platform wrappers.
- Which `embedded-hal-bus` or HAL-specific sharing implementations to document
  or support for each platform.
- Requirements for validating a multi-device shared bus beyond the
  conditional validation above.
- Whether future peripherals justify a platform-level resource manager.
- Async, DMA, interrupt, executor, retry, recovery, allocation, or proprietary
  resource abstractions; none are required by this initial architecture.
