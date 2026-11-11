#![no_std]
#![no_main]

use esp_backtrace as _;

use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    main,
};

use esp_println::println;

use ferrum_hd44780::{DisplaySize, Hd44780};
use ferrum_keypad::MatrixKeypad;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());

    let peripherals = esp_hal::init(config);

    // LCD1602: RS, E, D4, D5, D6, D7
    let rs = Output::new(peripherals.GPIO1, Level::Low, OutputConfig::default());
    let enable = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());

    let data = [
        Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO9, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default()),
        Output::new(peripherals.GPIO11, Level::Low, OutputConfig::default()),
    ];

    let mut lcd = Hd44780::new(rs, enable, data, Delay::new(), DisplaySize::Lcd16x2);

    lcd.init().unwrap();
    lcd.clear().unwrap();

    lcd.set_cursor(0, 0).unwrap();
    lcd.write_str("KEY: -").unwrap();

    lcd.set_cursor(0, 1).unwrap();
    lcd.write_str("INPUT:").unwrap();

    // HX-543: pins 1-4, provisionally mapped as rows.
    let rows = [
        Output::new(peripherals.GPIO4, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO6, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO7, Level::High, OutputConfig::default()),
    ];

    // HX-543: pins 5-8, provisionally mapped as columns.
    let columns = [
        Input::new(
            peripherals.GPIO12,
            InputConfig::default().with_pull(Pull::Up),
        ),
        Input::new(
            peripherals.GPIO13,
            InputConfig::default().with_pull(Pull::Up),
        ),
        Input::new(
            peripherals.GPIO16,
            InputConfig::default().with_pull(Pull::Up),
        ),
        Input::new(
            peripherals.GPIO17,
            InputConfig::default().with_pull(Pull::Up),
        ),
    ];

    let mut keypad = MatrixKeypad::new(rows, columns).unwrap();
    let mut delay = Delay::new();

    // Debouncing state.
    let mut candidate: Option<char> = None;
    let mut stable: Option<char> = None;
    let mut same_samples: u8 = 0;

    // Remaining 10 positions on the second LCD row.
    let mut position: u8 = 0;

    println!("FERRUM keypad + LCD ready");

    loop {
        let reading = keypad.scan().unwrap();

        if reading == candidate {
            same_samples = same_samples.saturating_add(1);
        } else {
            candidate = reading;
            same_samples = 1;
        }

        // Three matching samples, approximately 30 ms.
        if same_samples >= 3 && candidate != stable {
            stable = candidate;

            if let Some(key) = stable {
                println!("KEY DOWN: {}", key);

                // Show the most recently pressed key.
                lcd.set_cursor(5, 0).unwrap();
                lcd.write_char(key as u8).unwrap();

                // Append to our input history.
                if position >= 10 {
                    lcd.set_cursor(6, 1).unwrap();
                    lcd.write_str("          ").unwrap();
                    position = 0;
                }

                lcd.set_cursor(6 + position, 1).unwrap();
                lcd.write_char(key as u8).unwrap();

                position += 1;
            } else {
                println!("KEY UP");
            }
        }

        delay.delay_millis(10);
    }
}
