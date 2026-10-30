# ferrum-nrf24

An experimental, hardware-independent `no_std` blocking driver for the
nRF24L01+ transceiver. It uses synchronous `embedded-hal` 1.0 SPI, GPIO output,
and delay traits. No platform adapters, async SPI/polling backend, or
IRQ-driven backend are currently available. The driver has developer-reported
physical validation on one documented RP2350 ↔ ESP32-C3 setup; this does not
establish broad compatibility or long-term reliability.

## Radio lifecycle

`Nrf24::new` starts in `Uninitialized`. Successful initialization configures
the radio and enters Standby-I; it does not start listening.

| Operation | Required state | Successful state |
|---|---|---|
| `initialize` | `Uninitialized` | `Standby` |
| `start_listening` | `Standby` | `Listening` |
| `standby` | `Listening` | `Standby` |
| `power_down` | `Standby` | `PowerDown` |
| `power_up` | `PowerDown` | `Standby` |
| `transmit` | `Standby` or `Listening` | Previous state restored |
| `receive` | `Listening` | `Listening` |

Requests made from another state are rejected without hardware access.
`Faulted` is entered when a hardware operation leaves the radio state uncertain
or when transmission/restoration fails; normal operations are then rejected.
`release` returns the SPI device and CE pin without attempting hardware
recovery.

## Transmission and reception

`transmit` sends one 1–32 byte payload using `W_TX_PAYLOAD_NOACK`. It does not
request an acknowledgment or automatically retransmit, and TX completion does
not guarantee delivery. A successful call restores the prior `Standby` or
`Listening` state. Pending RX payloads and `RX_DR` are preserved. Once TX mode
has been entered, a transmission or restoration error leaves the driver
`Faulted`, even if restoring the prior mode succeeds; a restoration error can
occur after the payload was sent.

`receive` polls for at most one dynamic-length payload and does not wait for a
packet. It returns `None` when the RX FIFO is empty. The supplied buffer must
hold the full payload (1–32 bytes); if it is too small, the payload remains
queued and the error reports the required and available lengths. An invalid
payload width flushes the RX FIFO and returns an error. SPI failures fault the
driver. Reception is only available in `Listening`, which must be entered
explicitly with `start_listening`.

The driver supports channels 0–125, five-byte addresses, 250 kbit/s, 1 Mbit/s
or 2 Mbit/s, four nominal TX power levels, one-byte CRC, and dynamic payloads
on RX pipe 0. SPI must be configured for Mode 0, MSB-first, at no more than
10 MHz. See [ADR-002](../../docs/adr/002-nrf24-driver-architecture.md) for the
intended architecture; async backends and platform integrations described
there are planned, not implemented.

Tests use mocked HAL interactions; they do not establish physical-radio
compatibility or over-the-air delivery. Physical initialization and
bidirectional FER–RUM results for the tested boards and wiring are recorded in
the [nRF24 hardware audit](../../docs/audits/ferrum-nrf24.md). Other radio
configurations, exhaustive delivery rates, and long-term reliability remain
unverified.

Licensed under `MIT OR Apache-2.0`; see `LICENSE-MIT` and `LICENSE-APACHE`.
