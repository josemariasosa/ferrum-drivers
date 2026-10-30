extern crate std;

use std::vec;

use super::*;

use embedded_hal_mock::eh1::digital::Mock as PinMock;
use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction as SpiTransaction};

#[test]
fn read_rf_channel_register() {
    let expectations = [
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x02]),
        SpiTransaction::transaction_end(),
    ];

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    let channel = radio.read_register(0x05).unwrap();

    assert_eq!(channel, 0x02);

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn write_rf_channel_register() {
    let expectations = [
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
        SpiTransaction::transaction_end(),
    ];

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    radio.write_register(0x05, 0x32).unwrap();

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn write_tx_payload() {
    let payload = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ];

    let expected = [
        0xB0, 0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    let expectations = [
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(expected.to_vec()),
        SpiTransaction::transaction_end(),
    ];

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    radio.write_payload(&payload).unwrap();

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn reject_invalid_payload_lengths() {
    for size in [0, 33] {
        // No SPI transactions should occur.
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        let payload = [0u8; 33];

        let result = radio.write_payload(&payload[..size]);

        assert!(matches!(result, Err(PayloadError::InvalidLength)));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }
}

#[test]
fn flush_tx_fifo() {
    let expectations = [
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![0xE1]),
        SpiTransaction::transaction_end(),
    ];

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    radio.flush_tx().unwrap();

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn write_tx_address() {
    let expectations = [
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![
            0x30, // W_REGISTER | TX_ADDR (0x10)
            0xE7, 0xE7, 0xE7, 0xE7, 0xE7,
        ]),
        SpiTransaction::transaction_end(),
    ];

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    radio.write_register_bytes(0x10, &[0xE7; 5]).unwrap();

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn read_tx_address() {
    let expectations = [
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![0x10]),
        SpiTransaction::read_vec(vec![0xE7; 5]),
        SpiTransaction::transaction_end(),
    ];

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    let mut address = [0u8; 5];

    radio.read_register_bytes(0x10, &mut address).unwrap();

    assert_eq!(address, [0xE7; 5]);

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}
