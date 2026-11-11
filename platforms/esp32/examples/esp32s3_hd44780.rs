#![no_std]
#![no_main]

use esp_backtrace as _;
esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};

use ferrum_hd44780::{DisplayOrientation, DisplaySize, Hd44780};

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());

    let peripherals = esp_hal::init(config);

    let output_config = OutputConfig::default();

    // Control pins
    let rs = Output::new(peripherals.GPIO1, Level::Low, output_config);

    let enable = Output::new(peripherals.GPIO2, Level::Low, output_config);

    // 4-bit parallel data bus: D4, D5, D6, D7
    let d4 = Output::new(peripherals.GPIO8, Level::Low, output_config);

    let d5 = Output::new(peripherals.GPIO9, Level::Low, output_config);

    let d6 = Output::new(peripherals.GPIO10, Level::Low, output_config);

    let d7 = Output::new(peripherals.GPIO11, Level::Low, output_config);

    let delay = Delay::new();

    let mut lcd = Hd44780::new(rs, enable, [d4, d5, d6, d7], delay, DisplaySize::Lcd16x2);

    lcd.init().unwrap();

    lcd.set_orientation(DisplayOrientation::FlippedRows);

    lcd.set_cursor(0, 0).unwrap();
    lcd.write_str("HOLA, FERRUM!").unwrap();

    lcd.set_cursor(0, 1).unwrap();
    lcd.write_str("ESP32-S3 + RUST").unwrap();

    loop {
        // The LCD retains its contents.
    }
}
