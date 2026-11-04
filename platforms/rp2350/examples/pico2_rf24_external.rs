//! FER-RUM initiator using the external `rf24-rs` driver on RP2350.
//!
//! Runs on a Raspberry Pi Pico 2 W with an nRF24L01+ transceiver.
//! Uses explicit embedded-hal SPI and GPIO configuration.
//!
//! Exchanges FER/RUM frames with an ESP32-C3 responder.
//! Uses 1 Mbps, channel 50, 8-bit CRC, dynamic payloads, and NO_ACK.
//!
//! USB CDC reports initialization, transmission, and response results.
//!
//! Intended as a reproducible hardware integration example for rf24-rs.

#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use embedded_hal::digital::OutputPin;
use embedded_hal::spi::MODE_0;
use embedded_hal_bus::spi::ExclusiveDevice;

use heapless::String;
use rf24::radio::{prelude::*, RadioConfig, RF24};
use rf24::{CrcLength, DataRate, PaLevel};
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

// ---- FER-RUM PROTOCOL ----

const PROTOCOL_VERSION: u8 = 1;
const FER: u8 = 0x01;
const RUM: u8 = 0x02;
const FRAME_LEN: usize = 6;

const TX_INTERVAL_US: u64 = 2_000_000;
const RESPONSE_TIMEOUT_US: u64 = 500_000;

fn encode_fer(sequence: u32) -> [u8; FRAME_LEN] {
    let mut frame = [0u8; FRAME_LEN];

    frame[0] = FER;
    frame[1..5].copy_from_slice(&sequence.to_le_bytes());
    frame[5] = PROTOCOL_VERSION;

    frame
}

fn decode_rum(frame: &[u8]) -> Option<u32> {
    if frame.len() != FRAME_LEN || frame[0] != RUM || frame[5] != PROTOCOL_VERSION {
        return None;
    }

    Some(u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]))
}

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

    let mut radio = RF24::new(ce, spi, timer);

    let config = RadioConfig::default()
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

    let init_result = radio.init().and_then(|_| radio.with_config(&config));

    let listening_result = if init_result.is_ok() {
        Some(radio.as_rx())
    } else {
        None
    };

    let mut radio_ready = matches!(listening_result, Some(Ok(())));

    let mut sequence = 1u32;
    let mut waiting_for_rum = false;
    let mut consecutive_timeouts = 0u8;
    let mut deadline = 0u64;

    let mut next_tx = schedule_timer
        .get_counter()
        .ticks()
        .wrapping_add(TX_INTERVAL_US);

    let mut rx_buffer = [0u8; 32];

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
                "RF24 (External) nRF24 - RP2350\r\n\
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
                         RX mode: {:?}\r\n",
                        radio.is_rx(),
                    );
                }
                Err(error) => {
                    let _ = write!(
                        output,
                        "[FAIL] Radio initialization: {:?}\r\n\
                         RX mode: {:?}\r\n",
                        error,
                        radio.is_rx(),
                    );
                }
            }

            match &listening_result {
                Some(Ok(())) => {
                    let _ = write!(
                        output,
                        "[PASS] Listening enabled\r\n\
                        RX mode: {:?}\r\n\
                        [FER-RUM] Initiator ready\r\n",
                        radio.is_rx(),
                    );
                }

                Some(Err(error)) => {
                    let _ = write!(
                        output,
                        "[FAIL] Start listening: {:?}\r\n\
                        RX mode: {:?}\r\n",
                        error,
                        radio.is_rx(),
                    );
                }

                None => {
                    // Initialization failed; listening was not attempted.
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

        // ---- FER-RUM INITIATOR ----

        if !radio_ready {
            continue;
        }

        // ---- RECEIVE RUM ----

        let available = if waiting_for_rum {
            match radio.available() {
                Ok(available) => available,
                Err(error) => {
                    radio_ready = false;

                    if connected && !tx_pending {
                        output.clear();
                        let _ = write!(output, "[RX ERROR] available(): {:?}\r\n", error);
                        tx_offset = 0;
                        tx_pending = !output.is_empty();
                    }

                    continue;
                }
            }
        } else {
            false
        };

        if available {
            match radio.read(&mut rx_buffer, None) {
                Ok(length) => {
                    let payload = &rx_buffer[..usize::from(length)];

                    match decode_rum(payload) {
                        Some(received_seq) if received_seq == sequence => {
                            waiting_for_rum = false;
                            consecutive_timeouts = 0;

                            if connected && !tx_pending {
                                output.clear();

                                let _ = write!(output, "[PASS] FER-RUM seq={}\r\n", sequence,);

                                tx_offset = 0;
                                tx_pending = !output.is_empty();
                            }

                            sequence = sequence.wrapping_add(1);
                        }

                        Some(other_seq) => {
                            if connected && !tx_pending {
                                output.clear();

                                let _ = write!(
                                    output,
                                    "[STALE] Expected RUM {}, got {}\r\n",
                                    sequence, other_seq,
                                );

                                tx_offset = 0;
                                tx_pending = !output.is_empty();
                            }
                        }

                        None => {
                            if connected && !tx_pending {
                                output.clear();

                                let _ = write!(output, "[INVALID RUM] length={}\r\n", length,);

                                tx_offset = 0;
                                tx_pending = !output.is_empty();
                            }
                        }
                    }
                }

                Err(error) => {
                    radio_ready = false;

                    if connected && !tx_pending {
                        output.clear();

                        let _ = write!(output, "[RX ERROR] {:?}\r\n", error,);

                        tx_offset = 0;
                        tx_pending = !output.is_empty();
                    }

                    continue;
                }
            }
        }

        // ---- RESPONSE TIMEOUT ----

        let now = schedule_timer.get_counter().ticks();

        if waiting_for_rum && now.wrapping_sub(deadline) < u64::MAX / 2 {
            waiting_for_rum = false;
            consecutive_timeouts += 1;

            if connected && !tx_pending {
                output.clear();

                let _ = write!(
                    output,
                    "[TIMEOUT] RUM seq={} ({}/3)\r\n",
                    sequence, consecutive_timeouts
                );

                tx_offset = 0;
                tx_pending = !output.is_empty();
            }

            sequence = sequence.wrapping_add(1);

            if consecutive_timeouts >= 3 {
                radio_ready = false;
                continue;
            }
        }

        // ---- TRANSMIT NEXT FER ----

        let now = schedule_timer.get_counter().ticks();

        if !waiting_for_rum && now.wrapping_sub(next_tx) < u64::MAX / 2 {
            next_tx = now.wrapping_add(TX_INTERVAL_US);

            let frame = encode_fer(sequence);

            // Enter TX mode.
            if let Err(error) = radio.as_tx(None) {
                radio_ready = false;

                if connected && !tx_pending {
                    output.clear();
                    let _ = write!(output, "[TX MODE ERROR] {:?}\r\n", error);
                    tx_offset = 0;
                    tx_pending = !output.is_empty();
                }

                continue;
            }

            // Transmit without acknowledgment.
            let tx_result = radio.send(&frame, true);

            // Always attempt to restore RX mode after transmission.
            let rx_result = radio.as_rx();

            match tx_result {
                Ok(true) => {
                    waiting_for_rum = true;

                    deadline = schedule_timer
                        .get_counter()
                        .ticks()
                        .wrapping_add(RESPONSE_TIMEOUT_US);

                    if connected && !tx_pending {
                        output.clear();
                        let _ = write!(output, "[FER TX] seq={}\r\n", sequence);
                        tx_offset = 0;
                        tx_pending = !output.is_empty();
                    }
                }

                Ok(false) => {
                    consecutive_timeouts = consecutive_timeouts.saturating_add(1);

                    if connected && !tx_pending {
                        output.clear();
                        let _ = write!(
                            output,
                            "[TX FAILED] FER seq={} ({}/3)\r\n",
                            sequence, consecutive_timeouts,
                        );
                        tx_offset = 0;
                        tx_pending = !output.is_empty();
                    }

                    sequence = sequence.wrapping_add(1);

                    if consecutive_timeouts >= 3 {
                        radio_ready = false;
                    }
                }

                Err(error) => {
                    radio_ready = false;

                    if connected && !tx_pending {
                        output.clear();
                        let _ = write!(output, "[TX ERROR] seq={} {:?}\r\n", sequence, error,);
                        tx_offset = 0;
                        tx_pending = !output.is_empty();
                    }
                }
            }

            // Check whether RX mode was restored successfully.
            if let Err(error) = rx_result {
                radio_ready = false;

                if connected && !tx_pending {
                    output.clear();
                    let _ = write!(output, "[RX MODE ERROR] {:?}\r\n", error);
                    tx_offset = 0;
                    tx_pending = !output.is_empty();
                }
            }
        }
    }
}
