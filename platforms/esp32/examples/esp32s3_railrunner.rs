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
use railrunner::{Controls, Game};

use railrunner::sprites::{
    TRAIN, TRAIN_SPRITE, WORM_BODY, WORM_BODY_SPRITE, WORM_HEAD, WORM_HEAD_SPRITE, WORM_TAIL,
    WORM_TAIL_SPRITE,
};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());

    let peripherals = esp_hal::init(config);

    // ----- LCD1602 -----

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

    // Upload custom characters to HD44780 CGRAM.
    lcd.create_char(TRAIN, &TRAIN_SPRITE).unwrap();
    lcd.create_char(WORM_HEAD, &WORM_HEAD_SPRITE).unwrap();
    lcd.create_char(WORM_BODY, &WORM_BODY_SPRITE).unwrap();
    lcd.create_char(WORM_TAIL, &WORM_TAIL_SPRITE).unwrap();

    // Return to DDRAM before rendering.
    lcd.clear().unwrap();

    // ----- HX-543 keypad -----

    let rows = [
        Output::new(peripherals.GPIO4, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO6, Level::High, OutputConfig::default()),
        Output::new(peripherals.GPIO7, Level::High, OutputConfig::default()),
    ];

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

    // ----- Railrunner -----

    let mut game = Game::new();

    let controls = Controls::new(
        '5', // Up
        '2', // Down
        '3', // Left (reserved)
        '1', // Right (reserved)
        'A', // Fire (reserved)
    );

    let delay = Delay::new();

    // Prevent repeated input while holding a key.
    let mut candidate: Option<char> = None;
    let mut stable: Option<char> = None;
    let mut same_samples: u8 = 0;

    // Previous framebuffer for incremental rendering.
    let mut previous_frame = [0xFFu8; 32];

    println!("FERRUM: RAILRUNNER");
    println!("Controls: 5=UP, 2=DOWN");

    // ----- Main game loop -----

    loop {
        let reading = keypad.scan().unwrap();

        // Simple debounce: three consecutive readings.
        if reading == candidate {
            same_samples = same_samples.saturating_add(1);
        } else {
            candidate = reading;
            same_samples = 1;
        }

        if same_samples >= 3 && candidate != stable {
            stable = candidate;

            if let Some(key) = stable {
                println!("KEY: {}", key);

                if let Some(action) = controls.action_for(key) {
                    game.input(action);
                }
            }
        }

        game.update();

        // Render the game independently of LCD hardware.
        let frame = game.render();

        // Only update LCD cells that changed.
        for index in 0..32 {
            if frame[index] == previous_frame[index] {
                continue;
            }

            let row = (index / 16) as u8;
            let column = (index % 16) as u8;

            lcd.set_cursor(column, row).unwrap();
            lcd.write_char(frame[index]).unwrap();

            previous_frame[index] = frame[index];
        }

        // Approximately 100 iterations per second.
        delay.delay_millis(10);
    }
}
