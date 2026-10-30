# Ferrum nRF24 Blocking Driver Audit

**Audit date:** 2026-10-30

## 1. Executive Summary

The blocking driver now probes FEATURE-register access, uses ACTIVATE only
when necessary, and verifies the configured FEATURE/DYNPD values. Tests model
feature access as disabled until ACTIVATE, and cover SPI failures during
initialization and both power transitions. ADR-002 now distinguishes the
implemented blocking backend from planned async/platform work. All requested
software checks pass; no physical radio was available for validation.

## 2. Scope and Methodology

- Inspected the blocking implementation, unit and integration tests,
  `crates/ferrum-nrf24/README.md`, `docs/adr/002-nrf24-driver-architecture.md`,
  and workspace/package Cargo manifests.
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

The blocking backend is software-mocked and has no physical-radio or
over-the-air validation. The initialization mock now models FEATURE-register
access being disabled until ACTIVATE is issued, but software mocks still
cannot establish physical-radio behavior.

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
- **Residual limitations:** No physical nRF24L01+ or compatible clone was
  tested. The mock models the specified FEATURE access toggle but cannot
  establish silicon or clone compatibility.

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
- **Correction:** ADR-002 now lists the blocking SPI lifecycle, dynamic RX,
  NO_ACK transmission, and successful TX restoration as implemented, while
  marking async backends, platform integrations, and physical validation as
  pending. It also documents the conditional ACTIVATE strategy and the
  fail-stop TX error policy even when hardware restoration succeeds.

## 6. Test and Hardware Validation Matrix

| Check / evidence type | Result | What it establishes | Limitations |
|---|---|---|---|
| `cargo fmt --all -- --check` | **Passed** | Workspace Rust formatting is clean. | Does not validate protocol behavior. |
| `cargo test -p ferrum-nrf24` | **Passed:** 64 unit tests, 1 integration test, and 1 Rustdoc test. | Mocked state, SPI, TX/RX, boundary, activation, and error paths exercised by the suite. | Mocks do not establish silicon register semantics; see NRF24-01. |
| `cargo clippy -p ferrum-nrf24 --all-targets -- -D warnings` | **Passed** | No Clippy warnings for the crate's targets. | Static analysis only. |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p ferrum-nrf24 --no-deps` | **Passed** | Crate documentation builds without warnings. | Does not establish documentation accuracy beyond compilation. |
| `cargo check -p ferrum-nrf24 --target thumbv8m.main-none-eabihf` | **Passed** | Blocking crate cross-compiles for the installed RP2350 target. | Compilation only; not a firmware integration or hardware test. |
| nRF24L01+ Product Specification v1.0 | **Reviewed** | ACTIVATE argument `0x73` toggles FEATURE-register access; it is not idempotent. | Vendor endpoint returned 403 and the earlier public mirror is unavailable; no physical silicon was checked. |
| Physical radio / over-the-air behavior | **Not tested** | Nothing. | No hardware validation was performed; successful mocks and compilation are not evidence of radio operation. |

## 7. Recommended Remediation Order

### Blocking correction

1. **Completed:** Add conditional feature-register activation and
   hardware-semantic tests (NRF24-01).

### Follow-up

1. **Completed:** Add targeted SPI-failure coverage for initialization and
   power transitions (NRF24-02).
2. **Completed:** Correct the implementation-status statement in ADR-002
   (NRF24-03).

## 8. Conclusion

**Ferrum nRF24 Blocking v0.1 — Software complete, hardware validation pending.**

All three findings are resolved and the software validation matrix passes.
Physical-radio compatibility, RF communication, and over-the-air delivery
have not been established; those remain the next validation step.
