# `ferrum-dht11` audit

## Executive summary

The crate has a small, hardware-independent `no_std` API and the main DHT11
transaction shape is recognizable: hold DATA low for 18 ms, release it, check
the response handshake, sample 40 MSB-first bits, and validate the wrapping
checksum. The byte reconstruction and checksum arithmetic are correct for the
frame layout represented by the driver.

The current implementation is **not yet recommended for a trustworthy first
hardware test**. GPIO failures during pulse observation are reported as
protocol timeouts, and timeout checks can accept an edge observed after its
deadline. Both can hide wiring/timing faults or allow malformed pulses to be
decoded as data. These are behavior-changing fixes and were not applied during
this documentation-and-audit task. Existing unit tests pass/fail results are
recorded below after verification.

The `async` API only awaits the start delay. Pulse acquisition is synchronous
busy polling, so it blocks the Embassy executor during the response and data
frame. This may be acceptable for an isolated early test, but applications
must account for it.

## Findings

### High — GPIO failures during polling are misreported as protocol timeouts

**References:** [`driver/mod.rs`](../crates/ferrum-dht11/src/driver/mod.rs),
`measure_pulse` (lines 77–98), `wait_for_initial_low` (lines 118–130), and
`DhtError::PinError` (in
[`protocol.rs`](../crates/ferrum-dht11/src/protocol.rs)).

`measure_pulse` converts an `is_high()` failure to whichever timeout variant
was passed by its caller. `wait_for_initial_low` similarly converts a GPIO
failure to `NoInitialLow`. `PinError` is used for GPIO errors only while
driving/releasing the start signal. Consequently an application cannot
distinguish a failed GPIO read from a sensor that did not respond or whose
pulse did not end.

**Recommendation:** Map GPIO read failures to `DhtError::PinError` throughout
the polling helpers, reserving timeout variants for elapsed-time expiration.
Add tests that inject read failures during initial response polling, response
pulse measurement, and bit acquisition. This changes observable error behavior
and should be applied as a separate functional fix.

### Medium — Pulse/response timeout checks can accept late edges

**References:** [`driver/mod.rs`](../crates/ferrum-dht11/src/driver/mod.rs),
`measure_pulse` (lines 77–98), `wait_for_initial_low` (lines 118–130), and
`read_bit` (lines 132–142).

Both polling loops inspect the pin before checking elapsed time. If an edge is
first observed after the timeout, the helper returns success before checking
the deadline. In particular, a late data HIGH-to-LOW edge can produce a
duration greater than the 100 µs bit timeout and still be decoded as a `1`.
The initial response wait can likewise accept a low level first observed after
its 120 µs timeout. GPIO sampling latency makes the exact edge time unknowable,
but the deadline should still be enforced consistently against the sampled
time.

**Recommendation:** Define whether the timeout boundary is inclusive, then
validate elapsed time along with each sampled level so an observation after
the deadline cannot succeed. Add below/at/above-boundary tests for both
initial-low waiting and pulse measurement. This changes edge cases in current
behavior and should be made separately from the documentation changes.

### Medium — `read()` busy-polls synchronously after its async delay

**References:** [`driver/mod.rs`](../crates/ferrum-dht11/src/driver/mod.rs),
`read` (lines 36–62), `measure_pulse` (lines 77–98), and bit/frame reading
(lines 132–164).

After awaiting the 18 ms start delay, `read()` has no further await points.
Response and 40-bit frame acquisition repeatedly call the GPIO and clock in a
synchronous loop. The async signature therefore does not make pulse
observation cooperative: it can occupy the executor for several milliseconds
and delay other Embassy tasks. This is not inherently incorrect for a
timing-critical protocol, but is a relevant integration constraint.

**Recommendation:** Keep the current design for minimal hardware bring-up only
if this blocking interval is acceptable for the application. Document it (now
done in the source API docs), and measure actual transaction duration and
task impact on target hardware. If cooperative scheduling becomes a
requirement, evaluate a hardware edge-capture/timer-backed implementation
separately rather than adding awaits inside the existing pulse loop.

### Low — The full-transaction test does not couple the start signal to the simulated waveform

**References:** [`tests.rs`](../crates/ferrum-dht11/src/driver/tests.rs),
`TimedPin` and `SharedClock` (lines 13–58), `full_transaction` (lines 227–237),
and `reads_complete_measurement` (lines 607–638).

The complete `read()` test checks that the 18 ms async delay is requested and
then decodes a simulated response and frame. However, the fake delay does not
advance the shared timestamp, and the timed pin's `drive_low`/`release`
operations do not affect its waveform. Thus the test verifies orchestration
and decoding, but not that the response timing is relative to release of the
start signal or that the pin is released before sampling. The other mocks are
useful deterministic control-flow models: `SharedClock` advances when queried,
while `AdvancingClock` advances by a fixed amount per query and
`TransitionPin` changes by read count. None models real GPIO call latency,
timer granularity, interrupt jitter, or executor behavior.

**Recommendation:** Before relying on the integration test as transaction
coverage, connect simulated delay, pin state changes, and clock time in one
model. Also add injected GPIO failures and timeout-boundary cases. Existing
unit tests do cover valid/invalid handshake widths, zero/one decoding,
byte/frame assembly, and checksum validation, but they do not establish
hardware timing fidelity.

## Design and protocol assessment

- **Protocol flow:** An 18 ms host-low start signal and release are represented.
  The response waits for initial LOW, measures response LOW and HIGH, and
  accepts each in the 50–110 µs range. Bit bytes are assembled MSB first, with
  a 45 µs HIGH threshold separating short and long pulses. The first four
  bytes are summed with `u8` wrapping arithmetic and compared to byte five.
- **Timing limitations:** The response timeout is 120 µs; response pulse
  measurement uses 150 µs; bit LOW and HIGH timeouts are 100 µs. A polling
  implementation needs a clock that continues to advance during synchronous
  polling and has useful microsecond-scale resolution. GPIO call latency,
  timer quantization, interrupts, sensor tolerance, and electrical noise can
  affect measured durations. Data LOW duration has no explicit minimum-width
  check, and data HIGH is classified by one threshold rather than validated
  against separate accepted width bands. Confirm these tolerances against the
  exact sensor and target setup before relying on marginal signals.
- **Clock contract:** The implementation uses wrapping subtraction for
  elapsed time. A `u64` wrap is therefore supported for these short intervals;
  the more important requirement is that the clock advance while the
  synchronous loops execute. The source trait docs now state these actual
  requirements.
- **Hardware abstraction and ownership:** `DhtPin` is small and board-neutral;
  `Dht11` owns the pin and clock and `into_parts()` returns them. The caller
  supplies `embedded_hal_async::delay::DelayNs` to `read()`. The crate uses
  `#![no_std]` and has no platform HAL dependency. The public surface is
  appropriate for an early driver; no additional abstraction is needed for
  this audit.
- **Measurement API:** The result preserves the four transmitted integer and
  decimal bytes rather than scaling or combining them. Checksum validation is
  performed before returning a measurement.
- **Electrical setup:** DATA needs an external pull-up (or equivalent
  board-level pull-up) to a voltage safe for both devices. A 5 V pull-up must
  not be connected directly to a non-5-V-tolerant 3.3 V MCU input; use a
  compatible supply or level shifting. The HAL implementation must truly
  release DATA rather than drive it high against the sensor.
- **Hardware status:** No real-hardware verification is claimed. Pico 2
  (RP2350) and ESP32-C3 GPIO modes, timer sources, voltage compatibility,
  timing jitter, and pull-up choices remain to be checked on the actual
  boards and sensor modules.

## Rustdoc changes

Added crate-level purpose and conceptual usage documentation, hardware and
voltage requirements, and public API docs for `Dht11`, its methods, `DhtPin`,
`MicrosecondClock`, `Measurement`, and `DhtError`. The example is marked
`ignore` because it requires board-specific implementations of the hardware
traits and delay.

## Verification

Verification results:

- `cargo fmt --all --check` — passed.
- `cargo test -p ferrum-dht11` — passed: 25 unit tests passed; the Rustdoc
  example is ignored as intended.
- `cargo clippy -p ferrum-dht11 --all-targets -- -D warnings` — passed with no
  warnings.
- `cargo doc -p ferrum-dht11 --no-deps` — passed; documentation generated.
