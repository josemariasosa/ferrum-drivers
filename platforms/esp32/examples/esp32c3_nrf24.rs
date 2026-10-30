#![no_std]
#![no_main]

use esp_backtrace as _;

use embedded_hal::spi::SpiDevice;
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
use ferrum_nrf24::{Nrf24, RadioConfig};

esp_bootloader_esp_idf::esp_app_desc!();

const RADIO_CHANNEL: u8 = 50;
const RADIO_ADDRESS: [u8; 5] = [0xE7; 5];

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

    let mut spi = ExclusiveDevice::new(spi_bus, csn, Delay::new()).unwrap();

    // ---- RAW SPI DIAGNOSTIC ----

    // NOP returns the nRF24 STATUS register.
    let mut status = [0xFF];
    spi.transfer_in_place(&mut status).unwrap();

    println!("SPI STATUS: 0x{:02X}", status[0]);

    // R_REGISTER RF_CH (register address 0x05).
    // The second byte contains the channel value.
    let mut channel = [0x05, 0xFF];
    spi.transfer_in_place(&mut channel).unwrap();

    println!(
        "SPI RF_CH: STATUS=0x{:02X}, VALUE=0x{:02X}",
        channel[0], channel[1],
    );

    // R_REGISTER CONFIG (register address 0x00).
    let mut config_reg = [0x00, 0xFF];
    spi.transfer_in_place(&mut config_reg).unwrap();

    println!(
        "SPI CONFIG: STATUS=0x{:02X}, VALUE=0x{:02X}",
        config_reg[0], config_reg[1],
    );

    // ---- PORTABLE DRIVER ----

    let mut radio = Nrf24::new(spi, ce);

    let radio_config = RadioConfig::new(RADIO_CHANNEL, RADIO_ADDRESS).unwrap();

    let mut delay = Delay::new();

    // ---- INITIALIZATION ----

    match radio.initialize(&radio_config, &mut delay) {
        Ok(()) => {
            println!("[PASS] Radio initialized");
            println!("State: {:?}", radio.state());
        }

        Err(error) => {
            println!("[FAIL] Radio initialization: {:?}", error);
            println!("State: {:?}", radio.state());
        }
    }

    // ---- IDLE ----

    loop {
        delay.delay(Duration::from_secs(2));
    }
}
