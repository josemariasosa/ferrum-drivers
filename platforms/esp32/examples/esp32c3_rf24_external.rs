//! FER-RUM responder using the external `rf24-rs` driver on ESP32-C3.
//!
//! Hardware: ESP32-C3 + nRF24L01+ (SPI2, 1 MHz, mode 0).
//! Wiring: SCK=GPIO8, MOSI=GPIO10, MISO=GPIO21, CSN=GPIO5, CE=GPIO4.
//!
//! This example exercises `rf24-rs` directly through `embedded-hal` SPI/GPIO,
//! without the Ferrum radio driver or platform wrapper. It is intended to run
//! against the Pico 2 W FER-RUM initiator using `ferrum-nrf24`.
//!
//! Both peers must use RF channel 50, address E7:E7:E7:E7:E7, 1 Mbps,
//! 8-bit CRC, dynamic payloads, and NO_ACK transmissions. The responder
//! receives a 6-byte FER frame and replies with a 6-byte RUM frame containing
//! the same little-endian sequence number.
//!
//! The ESP32-C3 acts as responder throughout this test; no rewiring is needed
//! when switching the radio driver.

#![no_std]
#![no_main]

use esp_backtrace as _;

use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::{Duration, Rate},
};
use esp_println::println;
use rf24::radio::{prelude::*, RadioConfig, RF24};
use rf24::{CrcLength, DataRate, PaLevel};

esp_bootloader_esp_idf::esp_app_desc!();

const RADIO_CHANNEL: u8 = 50;
const RADIO_ADDRESS: [u8; 5] = [0xE7; 5];
const PROTOCOL_VERSION: u8 = 1;
const FER: u8 = 0x01;
const RUM: u8 = 0x02;
const FRAME_LEN: usize = 6;
const RESPONSE_DELAY_MS: u64 = 10;

fn decode_fer(frame: &[u8]) -> Option<u32> {
    if frame.len() != FRAME_LEN || frame[0] != FER || frame[5] != PROTOCOL_VERSION {
        return None;
    }
    Some(u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]))
}

fn encode_rum(sequence: u32) -> [u8; FRAME_LEN] {
    let mut frame = [0u8; FRAME_LEN];
    frame[0] = RUM;
    frame[1..5].copy_from_slice(&sequence.to_le_bytes());
    frame[5] = PROTOCOL_VERSION;
    frame
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    println!("RF24 (External) - ESP32-C3");
    println!("SPI2: 1 MHz, Mode 0");
    println!("SCK=GPIO8 MOSI=GPIO10 MISO=GPIO21");
    println!("CSN=GPIO5 CE=GPIO4");
    println!("RF Channel: {}", RADIO_CHANNEL);

    let spi_bus = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(1))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO8)
    .with_mosi(peripherals.GPIO10)
    .with_miso(peripherals.GPIO21);

    let csn = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    let ce = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let spi = ExclusiveDevice::new(spi_bus, csn, Delay::new()).unwrap();

    let mut radio = RF24::new(ce, spi, Delay::new());
    let radio_config = RadioConfig::default()
        .with_channel(RADIO_CHANNEL)
        .with_data_rate(DataRate::Mbps1)
        .with_crc_length(CrcLength::Bit8)
        .with_pa_level(PaLevel::Min)
        .with_tx_address(&RADIO_ADDRESS)
        .with_rx_address(0, &RADIO_ADDRESS)
        .close_rx_pipe(1)
        .with_dynamic_payloads(true)
        .with_auto_ack(1)
        .with_ask_no_ack(true);

    let delay = Delay::new();

    if let Err(error) = radio.init().and_then(|_| radio.with_config(&radio_config)) {
        println!("[FAIL] Radio initialization: {:?}", error);
        loop {
            delay.delay(Duration::from_secs(2));
        }
    }
    println!("[PASS] Radio initialized");

    if let Err(error) = radio.as_rx() {
        println!("[FAIL] Enter RX: {:?}", error);
        loop {
            delay.delay(Duration::from_secs(2));
        }
    }

    println!("[RX] Listening on channel {}", RADIO_CHANNEL);
    println!("[FER-RUM] Responder ready");

    let mut buffer = [0u8; 32];
    let mut received = 0u32;
    let mut responded = 0u32;

    loop {
        match radio.available() {
            Ok(true) => match radio.read(&mut buffer, None) {
                Ok(length) => {
                    let length = usize::from(length);
                    let payload = &buffer[..length];
                    if let Some(sequence) = decode_fer(payload) {
                        received = received.wrapping_add(1);
                        println!("[FER] seq={} received (total={})", sequence, received);

                        // The initiator needs time to switch from TX to RX.
                        delay.delay(Duration::from_millis(RESPONSE_DELAY_MS));

                        if let Err(error) = radio.as_tx(None) {
                            println!("[TX MODE ERROR] {:?}", error);
                            break;
                        }

                        let response = encode_rum(sequence);
                        // `true` requests NO_ACK, matching the Pico initiator.
                        let tx_result = radio.send(&response, true);

                        // RF24 does not automatically restore RX after send().
                        if let Err(error) = radio.as_rx() {
                            println!("[RX MODE ERROR] {:?}", error);
                            break;
                        }

                        match tx_result {
                            Ok(true) => {
                                responded = responded.wrapping_add(1);
                                println!("[RUM TX] seq={} (total={})", sequence, responded);
                            }
                            Ok(false) => {
                                println!("[TX NOT COMPLETED] seq={}", sequence);
                            }
                            Err(error) => {
                                println!("[TX ERROR] seq={} {:?}", sequence, error);
                                break;
                            }
                        }
                    } else {
                        println!("[INVALID FER] length={} data={:02X?}", length, payload);
                    }
                }
                Err(error) => {
                    println!("[RX ERROR] {:?}", error);
                    break;
                }
            },
            Ok(false) => {}
            Err(error) => {
                println!("[AVAILABLE ERROR] {:?}", error);
                break;
            }
        }
        delay.delay(Duration::from_millis(1));
    }

    loop {
        delay.delay(Duration::from_secs(2));
    }
}
