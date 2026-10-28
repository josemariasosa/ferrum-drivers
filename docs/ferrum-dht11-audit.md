# `ferrum-dht11` audit

## Executive summary

**Re-audit date: October 28, 2026.** The current implementation is a compact
`no_std` driver with a workable RP2350 adapter and a straightforward public
constructor. Its start pulse, response handshake, MSB-first 40-bit assembly,
wrapping checksum, and RP2350 open-drain release behavior are represented
correctly. The confirmed concerns are limited: data-bit pulse validation is
permissive, the `async` API blocks during the start delay and pulse polling,
and several tests do not exercise timing boundaries or injected GPIO failures.
There are no Critical or High findings in this re-audit.

The earlier report's polling-error and late-timeout findings do not describe
the current runtime code: GPIO read errors now map to `PinError`, and sampled
edges after the timeout are rejected. The related Rustdoc still contains the
old GPIO-error description and should be corrected.

The developer reports successful repeated measurements on a Pico 2 / DHT11
connected to GP15 (approximately 24.5 °C and 69% RH). This report records that
as developer-reported validation only; it was not independently reproduced
during this audit.

## Historical findings status

These are the findings from the previous audit, classified against the
implementation inspected on October 28, 2026:

| Previous finding | Status | Current evidence / note |
|---|---|---|
| GPIO read errors during polling were reported as protocol timeouts (previous High) | **Partially resolved** | `measure_pulse` and `wait_for_initial_low` now map GPIO read failures to `DhtError::PinError` (`src/driver/mod.rs:77-99,122-140`). The old claim remains in Rustdoc (`src/driver/mod.rs:44-48`, `src/protocol.rs:22-25`); see DHT-04. |
| An edge observed after a timeout could be accepted (previous Medium) | **Resolved** | Both helpers calculate elapsed time and return on `elapsed > timeout_us` / `elapsed > 120` before accepting the sampled edge (`src/driver/mod.rs:86-99,126-140`). The exact deadline is inclusive: an edge sampled at equality can succeed. Boundary tests do not fully establish this behavior; see DHT-03. |
| Async `read()` busy-polls synchronously after its start delay (previous Medium) | **Still open** | This remains the implementation behavior. Driver Rustdoc acknowledges synchronous pulse polling, but the RP2350 adapter also implements its async delay using a synchronous HAL delay. See DHT-02. |
| Full-transaction test does not couple start signal to the simulated waveform (previous Low) | **Still open** | The fake delay and `TimedPin` do not share elapsed time or start/release state (`src/driver/tests.rs:13-58,227-237,607-638`). See DHT-03. |

The earlier report's statements that there was no reported physical validation
and that polling GPIO failures became timeouts are superseded by this dated
re-audit. The hardware result remains developer-reported rather than
independently verified.

## Re-audit — October 28, 2026

### Findings

#### DHT-01 — Permissive data-bit pulse validation

- **Severity:** Medium
- **Type:** Confirmed protocol-validation weakness
- **Affected files:** `crates/ferrum-dht11/src/driver/mod.rs:144-154`
- **Evidence:** `read_bit()` calls `measure_pulse(false, 100, ...)` but discards
  the measured LOW duration. It then accepts any HIGH duration that ends by
  the 100 µs timeout and classifies it solely as `high_us > 45`. Thus a
  malformed but completed 46–100 µs HIGH pulse is accepted as `1`; there is
  no explicit accepted LOW-width range or distinct valid HIGH-width bands.
- **Technical impact:** Noise, timing distortion, or malformed transmissions
  can be interpreted as data rather than rejected. The 8-bit checksum catches
  many corrupted frames, but is not a guarantee against a checksum collision.
- **Recommended minimal action:** Validate data LOW and HIGH durations against
  explicitly chosen DHT11 timing tolerances, preserving a documented
  threshold/deadline policy. Add tests at each accepted boundary and just
  outside it. Confirm the selected tolerances against the exact sensor and
  target before tightening them.
- **Current status:** Open.

#### DHT-02 — Async API does not yield during RP2350 delay or frame polling

- **Severity:** Medium
- **Type:** Confirmed scheduling limitation; acceptable for the current
  single-threaded example
- **Affected files:** `crates/ferrum-dht11/src/driver/mod.rs:36-75,77-169`;
  `crates/ferrum-dht11/src/platforms/rp2350.rs:66-73,123-127`
- **Evidence:** The generic `read()` awaits the start delay, then performs
  response detection and all 40-bit reads in synchronous loops with no further
  await points. RP2350 `PicoDelay::delay_ns()` is declared async but directly
  calls `embedded_hal::delay::DelayNs::delay_ns()` on the timer without
  awaiting or yielding. The RP2350 read therefore blocks while the 18 ms
  start delay is performed and while the response/frame is polled.
- **Technical impact:** A cooperative executor cannot run other tasks during
  those intervals; in the example, `block_on(sensor.read())` also pauses USB
  polling for the duration. This does not invalidate the protocol and is
  likely acceptable for the example's two-second sampling interval, but it is
  not cooperative async behavior.
- **Recommended minimal action:** Keep the simple design for the current
  bare-metal example and make the RP2350 API documentation explicit that the
  delay and full transaction are blocking. Only pursue a yielding timer or
  hardware edge-capture implementation if an application has a demonstrated
  scheduling requirement.
- **Current status:** Open; a known design tradeoff, not a protocol defect.

#### DHT-03 — Timing tests leave important integration and boundary gaps

- **Severity:** Low
- **Type:** Confirmed test-coverage gap
- **Affected files:** `crates/ferrum-dht11/src/driver/tests.rs:13-58,227-237,
  278-290,293-338,607-638`
- **Evidence:** The fake clock advances only when queried, while `TimedPin`
  uses a separate transition table; `drive_low()`, `release()`, and the fake
  delay do not establish the waveform start time. Existing tests cover normal
  response widths, short response pulses, timeouts, bit zero/one, byte/frame
  decoding, and checksum unit behavior. They do not inject GPIO read errors,
  exercise malformed data LOW/HIGH widths, or directly test a pulse edge
  sampled exactly at and just beyond its timeout. The test named
  `initial_low_at_timeout_boundary` checks timeout when no LOW has been seen
  through the boundary, rather than a LOW transition exactly at the boundary.
- **Technical impact:** These deterministic tests provide useful control-flow
  coverage but cannot validate the coupling of the 18 ms start signal, pin
  release, GPIO sampling latency, and timer time. They also would not detect a
  regression in GPIO-error propagation or the permissive bit-duration checks.
- **Recommended minimal action:** Extend the existing test model so delay,
  pin transitions, and clock share one simulated timeline; add focused
  injected-error and below/at/above-boundary cases. Keep hardware timing
  validation separate from claims made by these unit tests.
- **Current status:** Open.

#### DHT-04 — GPIO-error Rustdoc contradicts current error propagation

- **Severity:** Low
- **Type:** Confirmed documentation defect
- **Affected files:** `crates/ferrum-dht11/src/driver/mod.rs:44-48`;
  `crates/ferrum-dht11/src/protocol.rs:22-25`
- **Evidence:** Both docs say GPIO failures during polling are reported as
  timeout/no-response errors. Current implementations instead use
  `.map_err(|_| DhtError::PinError)?` in both polling helpers
  (`src/driver/mod.rs:88,126`). Start-signal GPIO failures also map to
  `PinError`.
- **Technical impact:** Users reading API docs can incorrectly diagnose a
  GPIO failure as a sensor timeout. The runtime error behavior itself is
  correct here, although `PinError` intentionally does not retain the
  underlying platform error.
- **Recommended minimal action:** Update the two stale Rustdoc comments to
  state that all GPIO-operation failures map to `PinError`, and note that
  the platform-specific error detail is erased.
- **Current status:** Open; the runtime portion of the previous High finding
  is fixed.

#### DHT-05 — Cancellation during a yielding start delay can leave DATA low

- **Severity:** Low
- **Type:** Potential risk for generic adapters; not reproduced with the RP2350
  adapter
- **Affected files:** `crates/ferrum-dht11/src/driver/mod.rs:64-75`
- **Evidence:** `start_signal()` calls `drive_low()` and then awaits the
  caller-supplied `DelayNs` before calling `release()`. If that delay future
  yields and the caller drops the enclosing `read()` future while it is
  pending, there is no cancellation cleanup to release the pin.
- **Technical impact:** A cancelled read can leave a generic adapter holding
  the shared DATA line low, preventing subsequent communication until the pin
  is manually recovered. The RP2350 `PicoDelay` currently performs its delay
  synchronously within one poll, so this cancellation point is not exposed
  during that delay in the supplied adapter.
- **Recommended minimal action:** Document that callers must not cancel while
  the start signal is active, or add cancellation-safe pin release if the
  generic driver is intended to support yielding delays and cancellation.
- **Current status:** Open potential risk; not a confirmed RP2350 defect.

### Protocol and implementation assessment

- **Start signal and release:** `start_signal()` drives DATA low, awaits
  `delay_us(18_000)`, then calls `release()` (`src/driver/mod.rs:64-75`).
  There is no cleanup path if the async delay future is cancelled while DATA
  is low; the current RP2350 API exposes no cancellation point during its
  synchronous delay, but a generic caller can supply a genuinely yielding
  delay. Consider pin-state cleanup only if cancellation is in scope for the
  generic API.
- **Handshake and timeouts:** The initial response LOW must be observed by
  120 µs. Response LOW and HIGH pulse measurements each have 150 µs timeouts
  and are accepted at 50–110 µs (`src/driver/mod.rs:104-117,122-140`).
  Deadline checks reject observations after the timeout and accept an edge at
  equality. GPIO read errors propagate as `PinError`.
- **Frame acquisition:** The driver reads five bytes, eight bits per byte,
  MSB first, then validates the wrapping sum of the first four bytes against
  byte five (`src/driver/mod.rs:144-169`, `src/protocol.rs:45-64`). An
  incomplete pulse that never ends times out; there is no separate trailing
  post-frame pulse check. The checksum is the protocol's only payload
  integrity check.
- **Error diagnostics:** `DhtError` distinguishes missing/invalid response
  pulses, bit pulse timeouts, checksum mismatch, and GPIO failure
  (`src/protocol.rs:3-25`). The generic driver discards the concrete GPIO
  error value when mapping to `PinError`; the RP2350 HAL adapter's
  `InOutPin` operations currently return `Ok` for its set/read operations.
- **Clock contract and overflow:** The core computes elapsed time with
  `u64::wrapping_sub` (`src/driver/mod.rs:86,126`), suitable for these short
  intervals as long as the provided monotonic microsecond counter advances
  during polling. The RP2350 adapter returns `Timer::get_counter().ticks()`
  without conversion (`src/platforms/rp2350.rs:54-60`); `rp235x-hal 0.4.0`
  documents a 1 MHz timer tick requirement for `new_timer0`. The example
  initializes TIMER0 after configuring clocks and watchdog. Applications
  must preserve that HAL clock setup; the type alone does not encode or check
  the timer frequency.

### RP2350 integration and API assessment

- The public API permits the intended beginner flow,
  `rp2350::Dht11::new(pins.gpio15, timer)`, followed by `sensor.read().await`.
  The adapter owns the pin and timer handles, and does not require the user
  to implement `DhtPin` or understand protocol timing.
- The constructor accepts a `Pin<I, F, M>` where the pin ID supports SIO input
  and output, converts it to a floating SIO input, then wraps it in
  `hal::gpio::InOutPin` (`src/platforms/rp2350.rs:77-109`). Its generic
  constraints are not hardcoded to GP15; GP15 is simply the example's choice.
- Inspection of the resolved `rp235x-hal 0.4.0` implementation confirms
  `InOutPin::new` disables output enable before configuring its internal
  output-low state; `set_low()` enables the low output and `set_high()`
  disables the output driver. Thus release is high impedance and does not
  actively drive HIGH against the sensor. This is HAL behavior verified in
  the resolved dependency source, not an assumption about the trait name.
- The RP2350 adapter requires a floating-input pin and external/board-level
  DATA pull-up. Electrical voltage compatibility, resistor sizing, cable
  effects, and behavior on alternate compatible pins remain board-level
  concerns; generic type support does not independently prove a safe
  electrical connection. Do not pull a non-5-V-tolerant MCU input to 5 V.
- `DhtTimer` is `Timer<CopyableTimer0>`. The example copies the timer handle
  for scheduling while the adapter holds copies for its clock and delay
  (`examples/pico2_dht11.rs:48-63`). `rp235x-hal` documents this timer handle
  as copyable; the adapter only reads the counter and uses the same TIMER0
  for its synchronous delay. No overflow or unit conversion defect was
  identified for the configured 1 MHz timer.
- The `rp2350` feature gates the optional HAL dependency, while
  firmware/example dependencies are target-specific
  (`crates/ferrum-dht11/Cargo.toml:10-39`; root `Cargo.toml:1-18`).
  The configuration keeps the core crate's normal dependency surface
  hardware-independent. An ESP32-C3 adapter can implement the existing
  `DhtPin`, `MicrosecondClock`, and `DelayNs` boundaries without changing
  protocol code; no additional abstraction is justified now.

### Verification

Executed on October 28, 2026, with `rustc 1.98.1` and `cargo 1.98.1`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed; no formatting differences. |
| `cargo test -p ferrum-dht11` | Passed; 25 unit tests passed, 0 failed. Rustdoc test: 1 ignored, as intended. |
| `cargo check -p ferrum-dht11 --features rp2350 --target thumbv8m.main-none-eabihf` | Passed. Warning: `proc-macro-error2 v2.0.1` contains code that may be rejected by a future Rust version. |
| `cargo build -p ferrum-dht11 --example pico2_dht11 --features rp2350 --target thumbv8m.main-none-eabihf --release` | Passed. Warnings: linker reports `.text` address `0x10000124` is not a multiple of alignment 8; `proc-macro-error2 v2.0.1` future-incompatibility warning. |

The target and required toolchain were available. The release warning did not
fail the build, but should be confirmed against the intended firmware image
and linker layout before treating it as benign. No physical hardware test was
performed as part of this audit.

## Prioritized remediation plan

1. **DHT-01 (Medium):** Define and enforce acceptable data pulse widths;
   include values at and immediately outside each boundary in tests.
2. **DHT-02 (Medium):** Clarify in RP2350 public Rustdoc that its async
   operation blocks; retain the current simple implementation unless actual
   task-scheduling requirements justify a different timing mechanism.
3. **DHT-03 (Low):** Couple fake delay, GPIO transitions, and time; inject
   GPIO failures and exercise exact deadline behavior.
4. **DHT-04 (Low):** Correct stale error-propagation Rustdoc.
5. **DHT-05 (Low):** Document cancellation constraints or make the start
   sequence cancellation-safe if generic yielding delays are supported.
6. Investigate the release linker's alignment warning in the context of the
   final firmware image; it is a build warning, not a confirmed driver defect.
