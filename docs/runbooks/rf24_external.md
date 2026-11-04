### RP2350 — External RF24 driver (`rf24-rs`)

Build the external `rf24-rs` driver example for Raspberry Pi Pico 2 W (RP2350):

```bash
cd platforms/rp2350

cargo build \
  --release \
  --target thumbv8m.main-none-eabihf \
  --features rf24-external \
  --example pico2_rf24_external
```

Convert the ELF binary to UF2:

```bash
picotool uf2 convert \
  target/thumbv8m.main-none-eabihf/release/examples/pico2_rf24_external \
  -t elf \
  ../../firmware/pico2_rf24_external.uf2

ls /dev/cu.usbmodem*

screen /dev/cu.usbmodemXXXX 115200
```

Flash the UF2 file onto the Pico 2 W using BOOTSEL mode, or an appropriate flashing tool.

**Hardware configuration**

| Signal | RP2350 pin |
|---|---|
| SCK | GP18 |
| MOSI | GP19 |
| MISO | GP16 |
| CSN | GP21 |
| CE | GP20 |

**Radio configuration**

- RF channel: 50 (2450 MHz)
- Data rate: 1 Mbps
- Address: `E7:E7:E7:E7:E7`
- Dynamic payloads enabled
- Selective NO_ACK enabled
- CRC: 8-bit

**Validation**

Tested bidirectional FER-RUM communication between:

- RP2350 using `rf24-rs` as initiator.
- ESP32-C3 using `ferrum-nrf24` as responder.

Expected serial output:

```text
[PASS] Radio initialized
[PASS] Listening enabled
[FER-RUM] Initiator ready
[FER TX] seq=3
[PASS] FER-RUM seq=3
```

This confirms initialization, TX/RX transitions, and successful bidirectional communication between the two driver implementations.