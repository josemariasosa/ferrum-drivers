#![no_std]
#![no_main]

use embassy_futures::block_on;
use esp_backtrace as _;

use esp_hal::{clock::CpuClock, delay::Delay, main, time::Duration};

use esp_println::println;
use ferrum_esp32::dht11::Dht11;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // ---- CLOCKS ----

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());

    let peripherals = esp_hal::init(config);

    // ---- DHT11 ON GPIO3 / XIAO D1 ----

    let mut sensor = Dht11::new(peripherals.GPIO3);

    // ---- READING INTERVAL ----

    let delay = Delay::new();

    println!("Ferrum DHT11 - ESP32-C3");
    println!("Sensor: GPIO3 (XIAO D1)");
    println!("Reading every 2 seconds...");

    loop {
        let result = block_on(sensor.read());

        match result {
            Ok(measurement) => {
                println!(
                    "Humidity: {}.{} % | Temperature: {}.{} C",
                    measurement.humidity_integer,
                    measurement.humidity_decimal,
                    measurement.temperature_integer,
                    measurement.temperature_decimal,
                );
            }

            Err(error) => {
                println!("[WARN] DHT11 read failed: {:?} (retrying)", error,);
            }
        }

        delay.delay(Duration::from_secs(2));
    }
}
