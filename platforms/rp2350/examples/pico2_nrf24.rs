#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use embedded_hal::digital::OutputPin;
use embedded_hal::spi::MODE_0;
use embedded_hal_bus::spi::ExclusiveDevice;

use ferrum_nrf24::{Nrf24, RadioConfig};
use heapless::String;
use rp235x_hal as hal;

use hal::clocks::Clock;
use hal::fugit::RateExtU32;
use hal::gpio::FunctionSpi;

use usb_device::{bus::UsbBusAllocator, prelude::*};
use usbd_serial::{SerialPort, USB_CLASS_CDC};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        hal::arch::wfi();
    }
}

#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;
const RADIO_CHANNEL: u8 = 50;
const RADIO_ADDRESS: [u8; 5] = [0xE7; 5];

const PAYLOAD: [u8; 8] = *b"FERRUM01";
const TX_INTERVAL_US: u64 = 2_000_000;

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

    let mut timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    let mut spi_delay = timer;
    let schedule_timer = timer;

    // ---- GPIO ----

    let sio = hal::Sio::new(pac.SIO);

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // ---- SPI0 ----

    let miso = pins.gpio16.into_function::<FunctionSpi>();
    let sck = pins.gpio18.into_function::<FunctionSpi>();
    let mosi = pins.gpio19.into_function::<FunctionSpi>();

    let spi_bus = hal::spi::Spi::<_, _, _, 8>::new(pac.SPI0, (mosi, miso, sck)).init(
        &mut pac.RESETS,
        clocks.peripheral_clock.freq(),
        1.MHz(),
        MODE_0,
    );

    // ---- RADIO CONTROL ----

    let csn = pins.gpio21.into_function::<hal::gpio::FunctionSioOutput>();

    let mut ce = pins.gpio20.into_function::<hal::gpio::FunctionSioOutput>();

    ce.set_low().unwrap();

    // ---- SPI DEVICE ----

    let spi = ExclusiveDevice::new(spi_bus, csn, &mut spi_delay)
        .ok()
        .unwrap();

    // ---- PORTABLE DRIVER ----

    let mut radio = Nrf24::new(spi, ce);

    let config = RadioConfig::new(RADIO_CHANNEL, RADIO_ADDRESS).unwrap();

    // Initialize exactly once.
    // Preserve the result until USB becomes available.
    let init_result = radio.initialize(&config, &mut timer);

    let radio_ready = init_result.is_ok();

    let mut tx_count = 0u32;

    let mut next_tx = schedule_timer
        .get_counter()
        .ticks()
        .wrapping_add(TX_INTERVAL_US);

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
            .product("Pico 2 nRF24")
            .serial_number("00000003")])
        .unwrap()
        .device_class(USB_CLASS_CDC)
        .build();

    // ---- OUTPUT STATE ----

    let mut output: String<512> = String::new();
    let mut tx_offset = 0usize;
    let mut tx_pending = false;
    let mut terminal_connected = false;

    loop {
        usb_dev.poll(&mut [&mut serial]);

        let connected = serial.dtr();

        // Send startup diagnostics whenever a terminal opens.
        if connected && !terminal_connected {
            output.clear();
            tx_offset = 0;

            let _ = write!(
                output,
                "Ferrum nRF24 - RP2350\r\n\
                 SPI0: 1 MHz, Mode 0\r\n\
                 SCK=GP18 MOSI=GP19 MISO=GP16\r\n\
                 CSN=GP21 CE=GP20\r\n\
                 RF Channel: {}\r\n",
                RADIO_CHANNEL,
            );

            match &init_result {
                Ok(()) => {
                    let _ = write!(
                        output,
                        "[PASS] Radio initialized\r\n\
                         State: {:?}\r\n",
                        radio.state(),
                    );
                }
                Err(error) => {
                    let _ = write!(
                        output,
                        "[FAIL] Radio initialization: {:?}\r\n\
                         State: {:?}\r\n",
                        error,
                        radio.state(),
                    );
                }
            }

            tx_pending = !output.is_empty();
        }

        if !connected {
            output.clear();
            tx_offset = 0;
            tx_pending = false;
        }

        terminal_connected = connected;

        // Non-blocking USB transmission.
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
                Ok(_) | Err(usb_device::UsbError::WouldBlock) => {}
                Err(_) => {
                    output.clear();
                    tx_offset = 0;
                    tx_pending = false;
                }
            }
        }

        // ---- PERIODIC RADIO TX ----

        let now = schedule_timer.get_counter().ticks();

        if now.wrapping_sub(next_tx) < u64::MAX / 2 {
            next_tx = now.wrapping_add(TX_INTERVAL_US);

            if radio_ready {
                tx_count = tx_count.wrapping_add(1);

                let result = radio.transmit(&PAYLOAD, &mut timer);

                // Do not overwrite pending USB messages.
                if connected && !tx_pending {
                    output.clear();

                    match result {
                        Ok(()) => {
                            let _ = write!(output, "[TX OK] #{} FERRUM01 (8 bytes)\r\n", tx_count,);
                        }

                        Err(error) => {
                            let _ = write!(output, "[TX FAIL] #{} {:?}\r\n", tx_count, error,);
                        }
                    }

                    tx_offset = 0;
                    tx_pending = !output.is_empty();
                }
            }
        }
    }
}
