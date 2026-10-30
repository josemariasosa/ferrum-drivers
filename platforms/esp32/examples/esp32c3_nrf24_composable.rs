//! FER-RUM responder using the ESP32 nRF24 platform integration.
//!
//! Uses the composable ferrum-esp32 nRF24 wrapper.
//! SPI and GPIO are configured explicitly in this example.
//!
//! See esp32c3_nrf24_driver.rs for direct portable-driver usage.

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

    // ---- SPI2 ----

    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(1))
        .with_mode(Mode::_0);

    let spi_bus = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO8)
        .with_mosi(peripherals.GPIO10)
        .with_miso(peripherals.GPIO21);

    // ---- RADIO CONTROL ----

    let csn = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    let ce = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    // ---- SPI DEVICE ----

    let spi = ExclusiveDevice::new(spi_bus, csn, Delay::new()).unwrap();

    // ---- PORTABLE DRIVER ----

    let mut radio = Nrf24::from_spi_device(spi, ce);

    let radio_config = RadioConfig::new(RADIO_CHANNEL, RADIO_ADDRESS).unwrap();

    let mut delay = Delay::new();

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
