#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use embassy_futures::block_on;
use ferrum_dht11::rp2350::Dht11;
use heapless::String;
use rp235x_hal as hal;

use usb_device::{bus::UsbBusAllocator, prelude::*};
use usbd_serial::{SerialPort, USB_CLASS_CDC};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;
const READ_INTERVAL_US: u64 = 2_000_000;

#[hal::entry]
fn main() -> ! {
    // ---- CLOCKS ----

    let mut pac = hal::pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    let clocks = hal::clocks::init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    // ---- TIMER ----

    let timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    let schedule_timer = timer;

    // ---- GPIO15 / DHT11 ----

    let sio = hal::Sio::new(pac.SIO);

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let dht_pin = pins.gpio15.into_floating_input();
    let mut sensor = Dht11::new(dht_pin, timer);

    // ---- USB CDC ----

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut serial = SerialPort::new(&usb_bus);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .strings(&[StringDescriptors::default()
            .manufacturer("Ferrum Drivers")
            .product("Pico 2 DHT11")
            .serial_number("00000002")])
        .unwrap()
        .device_class(USB_CLASS_CDC)
        .build();

    // ---- OUTPUT STATE ----

    let mut output: String<512> = String::new();
    let mut tx_offset = 0usize;
    let mut tx_pending = false;

    let mut terminal_connected = false;

    // ---- READING SCHEDULE ----

    let mut next_read = schedule_timer
        .get_counter()
        .ticks()
        .wrapping_add(READ_INTERVAL_US);

    // ---- MAIN LOOP ----

    loop {
        usb_dev.poll(&mut [&mut serial]);

        // ---- USB CONNECTION ----

        let connected = serial.dtr();

        // A terminal has just opened the port.
        if connected && !terminal_connected {
            output.clear();
            tx_offset = 0;

            let _ = write!(
                output,
                "Ferrum DHT11 - RP2350\r\n\
                 Sensor: GPIO15\r\n\
                 Reading every 2 seconds...\r\n"
            );

            tx_pending = !output.is_empty();
        }

        // Terminal disconnected: discard pending output.
        if !connected {
            output.clear();
            tx_offset = 0;
            tx_pending = false;
        }

        terminal_connected = connected;

        // ---- NON-BLOCKING USB OUTPUT ----

        if connected && tx_pending {
            match serial.write(&output.as_bytes()[tx_offset..]) {
                Ok(n) if n > 0 => {
                    tx_offset += n;

                    if tx_offset >= output.len() {
                        tx_offset = 0;
                        tx_pending = false;
                        output.clear();
                    }
                }

                Ok(_) | Err(usb_device::UsbError::WouldBlock) => {
                    // Retry on the next loop iteration.
                }

                Err(_) => {
                    tx_offset = 0;
                    tx_pending = false;
                    output.clear();
                }
            }
        }

        // ---- READING SCHEDULE ----

        let now = schedule_timer.get_counter().ticks();

        if now.wrapping_sub(next_read) >= u64::MAX / 2 {
            continue;
        }

        // ---- SENSOR READ ----

        let result = block_on(sensor.read());

        // A completed measurement must not overwrite
        // an unfinished USB transmission.
        //
        // If USB is slow, drop this measurement's log,
        // but keep the sensor sampling schedule running.
        if connected && !tx_pending {
            match result {
                Ok(measurement) => {
                    let _ = write!(
                        output,
                        "Humidity: {}.{} % | Temperature: {}.{} C\r\n",
                        measurement.humidity_integer,
                        measurement.humidity_decimal,
                        measurement.temperature_integer,
                        measurement.temperature_decimal,
                    );
                }

                Err(error) => {
                    let _ = write!(
                        output,
                        "[WARN] DHT11 read failed: {:?} (retrying)\r\n",
                        error,
                    );
                }
            }

            tx_offset = 0;
            tx_pending = !output.is_empty();
        }

        // Always schedule the next measurement,
        // regardless of USB connection state.
        next_read = schedule_timer
            .get_counter()
            .ticks()
            .wrapping_add(READ_INTERVAL_US);
    }
}
