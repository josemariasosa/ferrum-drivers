//! FER-RUM responder using the ready-to-use ESP32 nRF24 integration.
//!
//! The platform wrapper configures SPI and GPIO for the radio.
//! See esp32c3_nrf24_composable.rs for externally configured SPI composition.

#![no_std]
#![no_main]

use esp_backtrace as _;

use esp_hal::{clock::CpuClock, delay::Delay, main, time::Duration};

use esp_println::println;
use ferrum_esp32::nrf24::{Nrf24, RadioConfig};

esp_bootloader_esp_idf::esp_app_desc!();

const RADIO_CHANNEL: u8 = 50;
const RADIO_ADDRESS: [u8; 5] = [0xE7; 5];

// ---- FER-RUM PROTOCOL ----

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
    // ---- CLOCKS ----

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());

    let peripherals = esp_hal::init(config);

    println!("Ferrum nRF24 - ESP32-C3");
    println!("SPI2: 1 MHz, Mode 0");
    println!("SCK=GPIO8 MOSI=GPIO10 MISO=GPIO21");
    println!("CSN=GPIO5 CE=GPIO4");
    println!("RF Channel: {}", RADIO_CHANNEL);

    let delay = Delay::new();

    // ---- RADIO ----

    let mut radio = match Nrf24::new(
        peripherals.SPI2,
        peripherals.GPIO8,
        peripherals.GPIO10,
        peripherals.GPIO21,
        peripherals.GPIO5,
        peripherals.GPIO4,
    ) {
        Ok(radio) => radio,
        Err(error) => {
            println!("[FAIL] Radio hardware configuration: {:?}", error);

            loop {
                delay.delay(Duration::from_secs(2));
            }
        }
    };

    let radio_config = RadioConfig::new(RADIO_CHANNEL, RADIO_ADDRESS).unwrap();

    // ---- INITIALIZATION ----

    match radio.initialize(&radio_config) {
        Ok(()) => {
            println!("[PASS] Radio initialized");
            println!("State: {:?}", radio.state());
        }

        Err(error) => {
            println!("[FAIL] Radio initialization: {:?}", error);
            println!("State: {:?}", radio.state());

            loop {
                delay.delay(Duration::from_secs(2));
            }
        }
    }

    // ---- START RX ----

    match radio.start_listening() {
        Ok(()) => {
            println!("[RX] Listening on channel {}", RADIO_CHANNEL);
            println!("State: {:?}", radio.state());
        }

        Err(error) => {
            println!("[FAIL] Start listening: {:?}", error);

            loop {
                delay.delay(Duration::from_secs(2));
            }
        }
    }

    // ---- FER-RUM RESPONDER ----

    let mut buffer = [0u8; 32];
    let mut received = 0u32;
    let mut responded = 0u32;

    println!("[FER-RUM] Responder ready");

    loop {
        match radio.receive(&mut buffer) {
            Ok(Some(length)) => {
                let payload = &buffer[..length];

                if let Some(sequence) = decode_fer(payload) {
                    received = received.wrapping_add(1);

                    println!("[FER] seq={} received (total={})", sequence, received,);

                    // Allow initiator time to return to RX.
                    delay.delay(Duration::from_millis(RESPONSE_DELAY_MS));

                    let response = encode_rum(sequence);

                    match radio.transmit(&response) {
                        Ok(()) => {
                            responded = responded.wrapping_add(1);

                            println!("[RUM TX] seq={} (total={})", sequence, responded,);
                        }

                        Err(error) => {
                            println!("[TX ERROR] seq={} {:?}", sequence, error,);
                            println!("State: {:?}", radio.state());
                            break;
                        }
                    }
                } else {
                    println!("[INVALID FER] length={} data={:02X?}", length, payload,);
                }
            }

            Ok(None) => {
                // No packet available.
            }

            Err(error) => {
                println!("[RX ERROR] {:?}", error);
                println!("State: {:?}", radio.state());
                break;
            }
        }

        delay.delay(Duration::from_millis(1));
    }

    loop {
        delay.delay(Duration::from_secs(2));
    }
}
