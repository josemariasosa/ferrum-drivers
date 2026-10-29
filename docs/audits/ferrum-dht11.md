# Ferrum DHT11 Driver Audit

**Audit date:** 2026-10-28

## 1. Executive Summary

`ferrum-dht11` is a compact, `no_std` protocol driver with hardware-independent
core logic and working RP2350 and ESP32-C3 adapters. Code inspection supports
the start signal, response handshake, 40-bit MSB-first frame assembly,
wrapping checksum, GPIO release, and error propagation. Both platform examples
compiled in release mode during this audit. Developer-reported physical tests
demonstrate successful readings on a Raspberry Pi Pico 2 and six consecutive
readings on a Seeed XIAO ESP32-C3 using GPIO3.

There are no Critical or High findings. The main technical risk is permissive
data-bit pulse validation; a completed but out-of-spec pulse can be classified
as data. The public async API also blocks during its delay and pulse polling.
Test coverage is useful for nominal protocol paths but does not yet verify
timing boundaries, GPIO failures, or a full transaction on a shared simulated
timeline. Two documentation findings from this audit were corrected during
follow-up maintenance; the remaining technical findings are unchanged.

Physically validated platforms, based on developer reports, are Raspberry Pi
Pico 2 (RP2350) and Seeed XIAO ESP32-C3 (GPIO3). These results demonstrate
successful operation, not calibration, long-duration stability, or timing
robustness.

## 2. Scope and Methodology

### Code and documentation inspected

- Core protocol, interfaces, and tests: `crates/ferrum-dht11/src/`,
  `crates/ferrum-dht11/tests/dht11_integration.rs`, and
  `crates/ferrum-dht11/Cargo.toml`.
- RP2350 adapter and example: `platforms/rp2350/src/dht11.rs` and
  `platforms/rp2350/examples/pico2_dht11.rs`.
- ESP32-C3 adapter and example: `platforms/esp32/src/dht11.rs` and
  `platforms/esp32/examples/esp32c3_dht11.rs`.
- Related documentation: `docs/dht11.md` and
  `docs/adr/001-platform-oriented-workspaces.md`.
- GPIO, timer, and clock behavior was checked against the locally resolved
  `rp235x-hal 0.4.0` and `esp-hal 1.2.2` sources.

### Automated verification executed

- Results of the current maintenance verification are recorded in Section 6
  and distinguish the remaining RP2350 linker warning from the corrected
  ESP32 example warning.

The builds verify compilation, not firmware correctness on target. No
independent hardware measurement or electrical/timing instrumentation was
performed for this audit.

### Physical evidence

The RP2350 and ESP32-C3 results below were reported by the developer. The
ESP32-C3 firmware was reported as release-compiled, flashed, and executed.
Physical evidence is kept separate from code inspection and automated test
results in the validation matrix.

## 3. Architecture Assessment

### Core protocol

`Dht11::start_signal()` in `crates/ferrum-dht11/src/driver/mod.rs` drives DATA
low for 18 ms, awaits the supplied delay, then releases the pin. The core waits
up to 120 us for the sensor's initial LOW and measures response LOW and HIGH
pulses with 150 us deadlines, accepting response durations of 50–110 us.
`read_frame()` reads five bytes, MSB first, and
`Measurement::from_frame()` checks the wrapping sum of the first four bytes
against the checksum byte.

For data bits, `read_bit()` measures the LOW pulse but discards its duration.
It treats any completed HIGH pulse greater than 45 us as `1`; the 100 us
measurement timeout is not a valid-width check. This leaves the data-bit
validation less strict than the response handshake. GPIO-operation failures
are mapped to `DhtError::PinError`, although the concrete adapter error is
discarded.

### Hardware traits and electrical assumptions

`DhtPin` requires an operation to drive DATA low, release it without actively
driving HIGH, and sample the line. `MicrosecondClock` must be monotonic,
advance during synchronous polling, and return consistent microsecond units.
The core uses wrapping subtraction for elapsed durations; it cannot enforce
the clock's units or resolution. The asynchronous `DelayNs` starts the host
signal, but the trait's async signature does not itself guarantee that an
implementation yields.

Both adapters use open-drain/release behavior and enable input sampling while
communicating. DATA requires a suitable external or board-level pull-up to a
voltage safe for both sensor and MCU. Wiring voltage, resistor sizing, and
board-level electrical integrity are not established by the code or readings.

### RP2350 adapter

`platforms/rp2350/src/dht11.rs` converts the supplied compatible pin to a
floating SIO input and wraps it in `hal::gpio::InOutPin`. In the resolved HAL,
`set_low()` enables the low output and `set_high()` disables output drive,
releasing the line. TIMER0 supplies the clock in ticks; the example initializes
the HAL timer after clock setup, and the adapter uses the same timer for its
delay. The adapter's `PicoDelay::delay_ns()` calls the synchronous HAL delay
inside an async method. GPIO failures are converted to `()`, then to the
driver's `PinError`.

### ESP32-C3 adapter

`platforms/esp32/src/dht11.rs` owns the input pin through `Flex<'d>`, configures
open-drain output with no internal pull, enables input sampling, and sets HIGH
to release DATA. The pin lifetime is tied to the adapter; construction
consumes the pin, avoiding a second owner. The `esp-hal 1.2.2` `Instant`
documentation specifies a 1 us timer resolution and a monotonic time since
boot while awake; it does not advance in sleep mode, which is not used during
the driver's active polling. `EspDelay::delay_ns()` wraps the synchronous
`DelayNs` implementation in an async method. The ESP GPIO operations are
infallible in this adapter and are represented as `Result<_, ()>` to satisfy
the shared trait.

Both adapters compile against their platform HALs and reuse the same
hardware-independent protocol. No adapter defect was confirmed by inspection;
the shared data-pulse validation and blocking behavior apply to both.

## 4. Findings Summary

Priority is independent of severity: **P1** means address in the next focused
work; **P2** means plan after the P1 items; **P3** means defer unless product
requirements bring it into scope.

| ID | Severity | Affected components | Status | Recommended priority |
|---|---|---|---|---|
| DHT-01 | Medium | Core protocol; both platforms | Open | P1 |
| DHT-02 | Medium | Core async API; both adapters/examples | Open | P2 |
| DHT-03 | Low | Core unit and integration tests | Open | P2 |
| DHT-04 | Low | Core Rustdoc | Resolved | — |
| DHT-05 | Low | Generic core API; adapters with yielding delays | Open, conditional | P3 |
| DHT-06 | Low | DHT11 guide and workspace ADR | Resolved | — |

Earlier observations that GPIO polling errors became protocol timeouts and
that a sampled edge after a timeout could be accepted are resolved in the
current implementation. Polling GPIO errors map to `PinError`; the timeout
check precedes acceptance of a sampled edge after the deadline. The exact
deadline remains inclusive. The stale GPIO-error Rustdoc is tracked separately
as DHT-04. DHT-04 and DHT-06 are resolved by the documentation changes
summarized under Resolved findings. Earlier statements that no physical
validation had been reported are superseded by the developer evidence
recorded here.

## 5. Detailed Findings

### Resolved findings

#### DHT-04 — GPIO-error Rustdoc contradicted runtime behavior

- **Original issue:** Rustdoc stated that GPIO read failures during polling
  became timeout/no-response errors, while the implementation returns
  `DhtError::PinError`.
- **Corrective action:** Updated the `read()` and `PinError` documentation to
  state that all GPIO-operation failures map to `PinError` and the generic
  driver does not retain the underlying platform error.
- **Files changed:** `crates/ferrum-dht11/src/driver/mod.rs` and
  `crates/ferrum-dht11/src/protocol.rs`.
- **Verification:** Inspected the actual GPIO error mapping and reran core
  formatting, tests, and platform compilation checks; results are in Section
  6.

#### DHT-06 — User documentation omitted implemented ESP32-C3 support

- **Original issue:** The guide omitted ESP32-C3 and pointed to an obsolete
  RP2350 example path; ADR-001 described ESP32 integration and validation as
  pending.
- **Corrective action:** Added current board, workspace, crate, features,
  targets, pin, wiring, usage, and runbook link to the guide. Updated ADR-001
  implementation and developer-reported validation status while preserving
  its architectural rationale.
- **Files changed:** `docs/dht11.md` and
  `docs/adr/001-platform-oriented-workspaces.md`.
- **Verification:** Checked API snippets and paths against both platform
  examples and library exports; both platform example builds completed in the
  current validation (Section 6). No hardware validation was repeated.

### Active findings

### DHT-01 — Permissive data-bit pulse validation

- **Severity:** Medium
- **Priority:** P1
- **Status:** Open
- **Affected platforms:** RP2350 and ESP32-C3, through the shared core.
- **Evidence:** `read_bit()` in
  `crates/ferrum-dht11/src/driver/mod.rs` ignores the measured data LOW
  duration. It classifies a completed HIGH pulse only by `high_us > 45`, with
  a 100 us timeout rather than an accepted-width range. Consequently, a
  completed 46–100 us HIGH pulse is accepted as `1`, and a short HIGH pulse
  can be accepted as `0`. The response handshake uses explicit duration
  bounds, but data bits do not.
- **Impact:** Malformed or timing-distorted pulses can be decoded as payload
  bits. The checksum detects many corrupt frames, but is not a guarantee
  against all corruption.
- **Recommendation:** Select and document tolerances appropriate to the
  supported DHT11 and target timing, validate both data LOW and HIGH widths,
  and test each accepted boundary plus values immediately outside it. Confirm
  tolerance choices on hardware rather than treating the reported readings as
  timing characterization.

### DHT-02 — Async read blocks the executor during the transaction

- **Severity:** Medium
- **Priority:** P2
- **Status:** Open
- **Affected platforms:** RP2350 and ESP32-C3; generic core API.
- **Evidence:** After awaiting the start delay, `read()` in
  `crates/ferrum-dht11/src/driver/mod.rs` performs response and 40-bit polling
  in synchronous loops. `PicoDelay::delay_ns()` and `EspDelay::delay_ns()` in
  the platform adapters call synchronous HAL delays directly, so their
  futures do not yield during the 18 ms start pulse either. Both examples
  invoke the API with `block_on()`.
- **Impact:** An executor cannot schedule other work during the delay or
  frame polling. In the RP2350 example, USB polling pauses for the duration of
  the sensor read. This is a scheduling limitation, not a protocol error, and
  is acceptable for the current two-second sampling examples.
- **Recommendation:** Keep the simple polling implementation for current
  use. Document its blocking behavior consistently and pursue a yielding
  timer or hardware edge capture only if an application requires cooperative
  scheduling.

### DHT-03 — Timing and fault-injection test gaps

- **Severity:** Low
- **Priority:** P2
- **Status:** Open
- **Affected components:** Core tests; both platform adapters lack dedicated
  adapter tests.
- **Evidence:** `crates/ferrum-dht11/src/driver/tests.rs` and
  `crates/ferrum-dht11/tests/dht11_integration.rs` cover nominal handshakes,
  timeouts, bit decoding, frame assembly, and checksum behavior. The
  full-transaction test models the delay separately from the pin transition
  timeline, so it does not verify that start-pulse duration and waveform
  timing share a clock. There are no injected GPIO-error cases, malformed
  data-pulse width cases, or direct tests of pulse edges exactly at and just
  beyond their deadlines. The initial-low boundary test verifies a timeout
  with no LOW observed through the boundary, not a LOW edge at that boundary.
- **Impact:** The tests do not catch regressions in GPIO-error propagation or
  data-width acceptance, and do not establish end-to-end timing behavior of
  the start signal and sensor waveform.
- **Recommendation:** Extend the existing test model to share virtual time
  across delay, pin transitions, and clock. Add GPIO-error and below/at/above
  deadline cases, including data-bit width boundaries. Keep those test claims
  distinct from physical timing validation.

### DHT-05 — Cancellation can leave DATA low with a yielding delay

- **Severity:** Low
- **Priority:** P3
- **Status:** Open, conditional on a yielding delay implementation.
- **Affected components:** Generic core API; no current platform adapter
  exposes a cancellation point during the start delay.
- **Evidence:** `start_signal()` in
  `crates/ferrum-dht11/src/driver/mod.rs` drives DATA low before awaiting the
  caller-supplied `DelayNs`; it releases the line only after that await
  completes. Dropping a pending read during a genuinely yielding delay skips
  release cleanup. The supplied RP2350 and ESP32-C3 delay adapters perform
  their delays synchronously in one poll, so this consequence is not
  demonstrated on those adapters.
- **Impact:** A generic adapter using a yielding delay could leave the shared
  line driven low after cancellation, requiring recovery before another
  transaction.
- **Recommendation:** Define the generic cancellation contract. If yielding
  delays are supported with cancellation, release the pin on cancellation
  through an appropriate design and test it; otherwise document the
  limitation. Do not treat this as a current platform failure.

## 6. Test and Hardware Validation Matrix

| Platform / evidence type | Result | What it establishes | Limitations |
|---|---|---|---|
| Host automated tests | **Verified by execution:** 24 unit tests and 1 integration test passed; 1 Rustdoc test ignored. | Nominal core decoding, selected timeouts, checksum, and simulated complete measurement. | Not real GPIO timing or electrical behavior; test gaps are DHT-03. |
| Formatting checks | **Verified by execution:** core and RP2350 workspace checks passed; the edited ESP32 example passes direct `rustfmt --check`. | Changed Rust files are formatted. | ESP32 workspace-wide check still reports a pre-existing formatting difference (leading blank line) in `platforms/esp32/src/dht11.rs`; it was left unchanged as outside scope. |
| RP2350 release compilation | **Verified by execution:** library check and release example build completed for `thumbv8m.main-none-eabihf`. | Adapter and example compile for the requested target. | Linker warns that `.text` address `0x10000124` is not aligned to 8 bytes; `proc-macro-error2 v2.0.1` reports a future-incompatibility warning. No independent flash/run was done in this audit. |
| RP2350 physical hardware | **Developer-reported:** successful physical DHT11 readings on Raspberry Pi Pico 2. | The reported setup produced readings. | No sequence, duration, instrumentation, calibration, or timing characterization supplied. |
| ESP32-C3 release compilation | **Verified by execution:** release library check and example build completed against `esp-hal 1.2.2`. | Adapter and example compile for `riscv32imc-unknown-none-elf`. | No compiler warning after removing the unnecessary `mut`; compilation alone does not establish runtime behavior. |
| ESP32-C3 physical hardware | **Developer-reported:** Seeed XIAO ESP32-C3, GPIO3; firmware release-compiled, flashed, and executed; six consecutive successful readings. | Successful reported operation on that board and pin. | This is not a long-duration stability test, precision calibration, or proof of timing robustness. |

Reported ESP32-C3 readings, in order:

1. 68% RH / 23.8 C
2. 68% RH / 23.8 C
3. 67% RH / 23.8 C
4. 76% RH / 24.1 C
5. 80% RH / 24.1 C
6. 83% RH / 24.1 C

### Explicit coverage gaps

- Valid and invalid data-bit LOW/HIGH widths at exact boundaries.
- GPIO-operation error propagation through start, handshake, and frame reads.
- A single simulated timeline coupling start delay, pin state, and clock.
- Hardware pulse capture or other independent timing characterization across
  both targets.
- Long-duration reliability, sensor-to-sensor variation, accuracy/calibration,
  voltage/pull-up variants, and task-scheduling behavior under load.
- Cancellation behavior for a generic adapter with a genuinely yielding
  delay.

## 7. Recommended Remediation Order

### Medium-term corrections

1. Add data-pulse width and deadline-boundary tests, then choose and enforce
   supported timing tolerances based on sensor requirements and measured
   target behavior (DHT-01, DHT-03).
2. Improve the virtual test fixture so delay, pin transitions, and clock share
   one timeline; add GPIO failure injection (DHT-03).

### Deferred engineering work

1. Replace or supplement synchronous polling with yielding timing or hardware
   edge capture only if cooperative scheduling is a demonstrated requirement
   (DHT-02).
2. Decide whether cancellation safety is part of the generic API contract;
   implement and test cleanup only if yielding delays are supported
   (DHT-05).

## 8. Conclusion

The driver has demonstrated useful early maturity: the shared `no_std` core is
separated from platform HALs, host protocol tests pass, both platform examples
compile in release mode, and developers report successful physical readings
on RP2350 and ESP32-C3. The implementation is not yet fully characterized.
Data-bit timing acceptance is permissive, its async interfaces block during
the transaction, and current tests do not cover important timing and fault
boundaries. The physical reports establish successful readings only; timing
robustness, long-term stability, calibration, and broader board compatibility
remain unverified.
