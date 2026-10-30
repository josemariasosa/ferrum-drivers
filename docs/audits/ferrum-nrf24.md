# Ferrum nRF24 Blocking Driver Audit

**Audit date:** 2026-10-30

## 1. Executive Summary

The blocking driver probes FEATURE-register access, uses ACTIVATE only when
necessary, and verifies the configured FEATURE/DYNPD values. Tests model
feature access as disabled until ACTIVATE, and cover SPI failures during
initialization and both power transitions. The supplied hardware results
establish initialization on the documented RP2350 and ESP32-C3 setup,
Pico-to-ESP32 payload delivery, and sequence-correlated FER–RUM communication
in both directions. These are bounded, developer-reported results, not a
long-term reliability or broad compatibility assessment.

## 2. Scope and Methodology

- Inspected the blocking implementation, unit and integration tests,
  `crates/ferrum-nrf24/README.md`, `docs/adr/002-nrf24-driver-architecture.md`,
  and workspace/package Cargo manifests.
- Recorded the physical test results and partial serial logs supplied for this
  documentation update. No firmware was flashed and no hardware test was
  independently reproduced during this update.
- Reviewed ACTIVATE semantics in the nRF24L01+ Product Specification v1.0:
  the command is executed with argument `0x73` and toggles access to FEATURE
  through the register read/write commands; it is not an idempotent enable.
  The Nordic PDF endpoint returned 403 and the previously used public mirror
  is currently unavailable. This is specification review, not validation
  against physical silicon.
- Ran the requested formatting, test, Clippy, and Rustdoc checks; results are
  in Section 6.

## 3. Architecture Assessment

The public state transitions match the README and ADR-002's intended
Standby-I/Listening/Power Down model. TX clears only TX status flags, flushes
only the TX FIFO, and restores the prior state on success; RX checks FIFO
contents, validates dynamic payload widths, and preserves undersized-buffer
payloads. The 20 us CE pulse, 130 us RX settling delay, and 2 ms power-up wait
are consistent with the reviewed specification's stated minimums/maximums
under its documented crystal assumption.

The blocking backend's software tests are mocked; they do not establish
silicon compatibility or over-the-air behavior. Separately, the supplied
physical results establish operation only for the specific boards, wiring,
and configuration in Section 7. They do not prove reliability over time or
compatibility with other nRF24 variants. The initialization mock models
FEATURE-register access being disabled until ACTIVATE is issued.

## 4. Findings Summary

Priority is independent of severity: **P1** means address before declaring the
blocking backend software-complete; **P2** means plan after P1.

| ID | Severity | Affected components | Status | Recommended priority |
|---|---|---|---|---|
| NRF24-01 | High | Initialization; FEATURE/DYNPD configuration | Resolved | P1 |
| NRF24-02 | Low | Initialization and power-transition tests | Resolved | P2 |
| NRF24-03 | Low | ADR-002 implementation-status description | Resolved | P2 |

## 5. Detailed Findings

### NRF24-01 — Feature registers are accessed without ACTIVATE

- **Severity:** High
- **Priority:** P1
- **Status:** Resolved
- **Affected files:** `crates/ferrum-nrf24/src/driver/init/mod.rs`;
  `crates/ferrum-nrf24/src/driver/init/tests.rs`;
  `crates/ferrum-nrf24/tests/radio_lifecycle.rs`.
- **Correction:** Initialization writes the required FEATURE value and reads
  it back. If it does not match, the driver issues exactly one ACTIVATE
  (`0x50, 0x73`) and retries the FEATURE write. This avoids toggling access
  that is already enabled. Final initialization readback verifies
  `FEATURE.EN_DPL`, `FEATURE.EN_DYN_ACK`, and `DYNPD.DPL_P0`; a failed SPI
  operation propagates its error and a readback mismatch returns
  `VerificationFailed`, with the public initializer setting Faulted. The
  `W_TX_PAYLOAD_NOACK` path remains unchanged and is supported by
  `FEATURE.EN_DYN_ACK`.
- **Tests:** `initialize_with_feature_access_already_enabled_does_not_activate`
  verifies no ACTIVATE is sent; `initialize_activates_feature_access_once_when_initially_disabled`
  verifies command order and configured values with a hardware-semantic
  FEATURE mock; `initialize_faults_when_activate_spi_transaction_fails` and
  `initialize_faults_when_feature_access_remains_disabled_after_activate`
  cover activation and final-readback failure.
- **Residual limitations:** The mock models the specified FEATURE access
  toggle but cannot establish silicon or clone compatibility. Physical
  operation has been reported for the specific nRF24L01+ modules in Section 7;
  other radios and clones remain untested.

### NRF24-02 — Fail-stop transition tests omit partial SPI failures

- **Severity:** Low
- **Priority:** P2
- **Status:** Resolved
- **Affected files:** `crates/ferrum-nrf24/src/driver/init/tests.rs`;
  `crates/ferrum-nrf24/tests/radio_lifecycle.rs`.
- **Correction:** Added targeted SPI error injection without changing
  production transition behavior.
- **Tests:** `initialize_faults_when_activate_spi_transaction_fails` fails
  the ACTIVATE transaction; `power_down_spi_write_failure_faults_without_entering_power_down`
  fails the CONFIG write after a successful read; and
  `power_up_spi_read_failure_faults_without_writing_or_delay` fails the
  initial CONFIG read. Each checks error propagation, Faulted state, no false
  stable-state transition, and that no later SPI/GPIO/delay step occurs.

### NRF24-03 — ADR-002 describes the crate as an unimplemented scaffold

- **Severity:** Low
- **Priority:** P2
- **Status:** Resolved
- **Affected file:** `docs/adr/002-nrf24-driver-architecture.md`.
- **Correction:** ADR-002 lists the blocking SPI lifecycle, dynamic RX,
  NO_ACK transmission, and successful TX restoration as implemented. It
  distinguishes the now-documented, bounded physical evidence from pending
  async backends, platform integrations, and broader hardware validation. It
  also documents conditional ACTIVATE and the fail-stop TX error policy.

## 6. Test and Hardware Validation Matrix

| Check / evidence type | Result | What it establishes | Limitations |
|---|---|---|---|
| `cargo fmt --all -- --check` | **Passed** | Workspace Rust formatting is clean. | Does not validate protocol behavior. |
| `cargo test -p ferrum-nrf24` | **Passed:** 64 unit tests, 1 integration test, and 1 Rustdoc test. | Mocked state, SPI, TX/RX, boundary, activation, and error paths exercised by the suite. | Mocks do not establish silicon register semantics; see NRF24-01. |
| `cargo clippy -p ferrum-nrf24 --all-targets -- -D warnings` | **Passed** | No Clippy warnings for the crate's targets. | Static analysis only. |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p ferrum-nrf24 --no-deps` | **Passed** | Crate documentation builds without warnings. | Does not establish documentation accuracy beyond compilation. |
| `cargo check -p ferrum-nrf24 --target thumbv8m.main-none-eabihf` | **Passed** | Blocking crate cross-compiles for the installed RP2350 target. | Compilation only; not a firmware integration or hardware test. |
| nRF24L01+ Product Specification v1.0 | **Reviewed** | ACTIVATE argument `0x73` toggles FEATURE-register access; it is not idempotent. | Vendor endpoint returned 403 and the earlier public mirror is unavailable; specification behavior was not independently rechecked against physical silicon. |
| Physical radio validation | **Developer-reported; details in Section 7** | Initialization and application-level communication on the documented board pair. | Partial logs do not establish delivery rate, long-term reliability, or broad compatibility. |

## 7. Physical hardware validation record

The following results and logs were supplied for this documentation update.
They have not been independently reproduced here. "Compiled" is a software
result; "initialized" and "physically verified" refer only to the observed
hardware behavior described in the evidence column.

| ID | Stage | Board | Status and evidence | Not established |
|---|---|---|---|---|
| HW-01 | Firmware compilation | Raspberry Pi Pico 2 W / RP2350 | **Compiled:** `pico2_nrf24_driver` for `thumbv8m.main-none-eabihf`. | Flashing and radio behavior are separate results. |
| HW-02 | Firmware execution and radio initialization | Raspberry Pi Pico 2 W / RP2350 | **Initialized:** output reported `[PASS] Radio initialized` and `State: Standby`; the portable driver completed internal configuration readback verification. A running image is evidenced by the serial output; the flash procedure was not separately reported. | No long-duration initialization or reset/recovery testing. |
| HW-03a | Firmware compilation | Seeed XIAO ESP32-C3, revision v0.4 | **Compiled:** ESP32-C3 firmware. | Compilation alone does not establish physical operation. |
| HW-03b | Flash and boot | Seeed XIAO ESP32-C3, revision v0.4 | **Flashed and booted:** reported successful. | No independent reproduction in this documentation update. |
| HW-03c | Physical radio initialization | Seeed XIAO ESP32-C3, revision v0.4 | **Initialized:** after correcting firmware mapping to match the unchanged physical wiring, `[PASS] Radio initialized` was observed. Temporary diagnostics read `SPI STATUS: 0x0E`, `RF_CH ... VALUE=0x02`, and `CONFIG ... VALUE=0x08`. | Raw diagnostic reads do not independently establish RF delivery; their timing relative to initialization was not reported. |
| HW-04 | Pico TX → ESP32 RX | Both boards | **Physically verified:** ESP32 received and validated the exact 8-byte payload `FERRUM01`; receiver logs include sequences 45–50. | TX/RX counters were not synchronized; do not infer packet loss or a delivery percentage. |
| HW-05 | Bidirectional FER–RUM | Both boards | **Physically verified:** Pico initiated FER requests and accepted matching RUM responses; ESP32 received FER and transmitted RUM with the same sequence. Partial logs show successful sequences 31 and 32 at the Pico and matching receive/transmit activity at the ESP32. | Logs are partial and do not establish 100 exchanges, a 100% delivery rate, or long-term reliability. |

### Boards, wiring, and radio settings

Both radios were nRF24L01+ modules. Both used RF channel 50 (2450 MHz), the
five-byte address `[0xE7; 5]`, 1 MHz SPI in Mode 0, dynamic payloads, and
`W_TX_PAYLOAD_NOACK`. `RadioConfig::new` defaults the air data rate to 1 Mbps
and nominal radio TX power to minimum (−18 dBm); initialization configures
one-byte CRC. These settings are derived from the current `RadioConfig` and
driver implementation. Reception polls the RX FIFO; IRQ was not connected.

**Board A — Raspberry Pi Pico 2 W (RP2350):** SPI0, 1 MHz, Mode 0.

| nRF24 signal | Pico pin |
|---|---|
| SCK | GP18 |
| MOSI | GP19 |
| MISO | GP16 |
| CSN | GP21 |
| CE | GP20 |
| VCC | 3.3 V |
| GND | GND |
| IRQ | Not connected |

**Board B — Seeed XIAO ESP32-C3 (ESP32-C3 revision v0.4):** SPI2, 1 MHz,
Mode 0. XIAO board labels are shown separately from the ESP32-C3 GPIO numbers;
the validated firmware mapping follows the existing physical wiring.

| nRF24 signal | XIAO board label | ESP32-C3 GPIO |
|---|---|---|
| SCK | D8 | GPIO8 |
| MOSI | D10 | GPIO10 |
| MISO | D6 | GPIO21 |
| CSN | D3 | GPIO5 |
| CE | D2 | GPIO4 |
| VCC | 3V3 | — |
| GND | GND | — |
| IRQ | Not connected | — |

The XIAO board labels must not be interpreted as GPIO numbers. This physical
configuration was validated as listed; no conventional SPI pin substitution
or rewiring is implied.

### FER–RUM application protocol

The Pico 2 W is the initiator and the XIAO ESP32-C3 is the responder. The
examples use `ferrum-nrf24` directly, with explicit HAL configuration; they
are low-level `_driver` examples, not platform wrapper examples.

Each frame is exactly 6 bytes:

| Byte offset | Meaning |
|---|---|
| 0 | Message type: FER request `0x01`; RUM response `0x02` |
| 1–4 | Sequence number, `u32` little-endian |
| 5 | Protocol version `0x01` |

The Pico sends FER with a sequence number, then waits for a RUM frame with the
same sequence and reports success or timeout. The ESP32 polls for FER,
validates its length, type, and version, then sends RUM with the same
sequence. Since TX uses NO_ACK, the sender's successful TX operation alone
does not prove delivery; the receiver-side FER log and matching RUM accepted
by the Pico provide application-level evidence for the observed exchanges.

### Diagnostic output and interpretation

The Pico initialization-stage output was:

```text
Ferrum nRF24 - RP2350
SPI0: 1 MHz, Mode 0
SCK=GP18 MOSI=GP19 MISO=GP16
CSN=GP21 CE=GP20
RF Channel: 50
[PASS] Radio initialized
State: Standby
```

In the current FER–RUM example, the serial startup report queries state after
`start_listening()`. Therefore `[PASS] Radio initialized` can appear beside
`State: Listening`; that logging order does not mean `initialize()` enters
Listening. Its combined startup output is expected to resemble:

```text
Ferrum nRF24 - RP2350
SPI0: 1 MHz, Mode 0
SCK=GP18 MOSI=GP19 MISO=GP16
CSN=GP21 CE=GP20
RF Channel: 50
[PASS] Radio initialized
State: Listening
[PASS] Listening enabled
State: Listening
[FER-RUM] Initiator ready
[FER TX] seq=31
[PASS] FER-RUM seq=31
```

The separate initialization-stage capture above showed `State: Standby`.
During later exchanges, sequence numbers advance; timeouts are reported as
`[TIMEOUT] RUM seq=...`.

The ESP32-C3 bring-up output included:

```text
SPI STATUS: 0x0E
SPI RF_CH: STATUS=0x0E, VALUE=0x02
SPI CONFIG: STATUS=0x0E, VALUE=0x08
[PASS] Radio initialized
State: Standby
```

The three raw SPI lines were temporary diagnostics. The final example's
expected output is:

```text
Ferrum nRF24 - ESP32-C3
SPI2: 1 MHz, Mode 0
SCK=GPIO8 MOSI=GPIO10 MISO=GPIO21
CSN=GPIO5 CE=GPIO4
RF Channel: 50
[PASS] Radio initialized
State: Standby
[RX] Listening on channel 50
State: Listening
[FER-RUM] Responder ready
[FER] seq=31 received (total=...)
[RUM TX] seq=31 (total=...)
```

The reported raw `RF_CH` value `0x02` is not channel 50 (`0x32`), and
`CONFIG=0x08` does not show the power-up/listening bits configured by the
driver. The report does not say whether these temporary reads were taken
before or after initialization. The driver reports successful initialization
only after its configuration readback checks, so these raw values must not be
presented as the final configured register values; if they were read after
initialization, they conflict with the reported verification result.

### Remaining unverified behavior

- The supplied exchange logs are partial. No 100-exchange stress test,
  synchronized packet-loss measurement, or 100% delivery claim is supported.
- Long-term reliability, range, coexistence/interference, power-supply margins,
  and compatibility with other boards, nRF24 variants, or clones are untested.
- IRQ reception is untested; IRQ was left disconnected and the examples poll.
- Async backends, platform nRF24 wrappers, and shared-bus composition are not
  implemented or validated by these results.
- Physical behavior outside the exact listed wiring and radio configuration
  is not established.

The current example source has documentation-comment discrepancies that were
not changed as part of this documentation-only task: the Pico file's header
calls its role "responder" although its implementation transmits FER and waits
for RUM as the initiator; both `_driver` example headers refer to a future
`esp32c3_nrf24_platform.rs` example that is not present in the repository.

## 8. Recommended Remediation Order

### Blocking correction

1. **Completed:** Add conditional feature-register activation and
   hardware-semantic tests (NRF24-01).

### Follow-up

1. **Completed:** Add targeted SPI-failure coverage for initialization and
   power transitions (NRF24-02).
2. **Completed:** Correct the implementation-status statement in ADR-002
   (NRF24-03).

## 9. Conclusion

**Ferrum nRF24 Blocking v0.1 — Software checks pass; bounded physical
initialization and bidirectional communication are developer-reported.**

All three findings are resolved and the software validation matrix passes.
The documented results establish RF delivery only for the listed hardware and
configuration. Broader compatibility, delivery-rate characterization, and
long-term reliability remain unverified.
