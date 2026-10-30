extern crate std;

use std::io::ErrorKind;
use std::vec;
use std::vec::Vec;

use super::*;

use embedded_hal_mock::eh1::{
    MockError,
    delay::{CheckedDelay as DelayMock, Transaction as DelayTransaction},
    digital::{Mock as PinMock, State as PinState, Transaction as PinTransaction},
    spi::{Mock as SpiMock, Transaction as SpiTransaction},
};

use crate::error::ConfigError;

fn configure_rx_expectations() -> Vec<SpiTransaction<u8>> {
    let expectations = [
        // Enable pipe 0 Enhanced ShockBurst; TX packets select NO_ACK.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x21, 0x01], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // Enable RX pipe 0.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x22, 0x01], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // RF channel = 50.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // RF_SETUP = 1 Mbps, minimum power.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x26, 0x00], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // SETUP_AW = 5-byte address width.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x23, 0x03], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // RX_ADDR_P0 = [0xE7; 5].
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![0x2A, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
        SpiTransaction::transaction_end(),
        // TX_ADDR = [0xE7; 5].
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
        SpiTransaction::transaction_end(),
        // FEATURE = EN_DPL | EN_DYN_ACK.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x3D, 0x05], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // DYNPD = DPL_P0.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x3C, 0x01], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
        // Read CONFIG: return 0x08.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x00, 0x08]),
        SpiTransaction::transaction_end(),
        // Write CONFIG = 0x08 | PWR_UP | PRIM_RX.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x20, 0x0B], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
    ];

    expectations.into()
}

fn verify_rx_expectations(config_value: u8) -> Vec<SpiTransaction<u8>> {
    verify_rx_expectations_with_tx_addr(config_value, [0xE7; 5])
}

fn verify_rx_expectations_with_tx_addr(
    config_value: u8,
    tx_addr: [u8; 5],
) -> Vec<SpiTransaction<u8>> {
    let registers = [
        (CONFIG, config_value),
        (EN_AA, 0x01),
        (EN_RXADDR, 0x01),
        (RF_CH, 0x32),
        (RF_SETUP, 0x00),
        (SETUP_AW, 0x03),
        (FEATURE, 0x05),
        (DYNPD, 0x01),
    ];

    let mut expectations = Vec::new();

    for (register, value) in registers {
        expectations.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0x00, value]),
            SpiTransaction::transaction_end(),
        ]);
    }

    // read_register_bytes(RX_ADDR_P0, ...)
    expectations.extend([
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![RX_ADDR_P0]),
        SpiTransaction::read_vec(vec![0xE7; 5]),
        SpiTransaction::transaction_end(),
    ]);

    // read_register_bytes(TX_ADDR, ...)
    expectations.extend([
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![TX_ADDR]),
        SpiTransaction::read_vec(tx_addr.to_vec()),
        SpiTransaction::transaction_end(),
    ]);

    expectations
}

#[test]
fn accept_valid_tx_config() {
    let config = RadioConfig::new(50, [0xE7; 5]);

    assert!(config.is_ok());
}

#[test]
fn reject_invalid_tx_channel() {
    let config = RadioConfig::new(126, [0xE7; 5]);

    assert_eq!(config, Err(ConfigError::InvalidChannel));
}

#[test]
fn new_radio_starts_in_unknown_state() {
    let spi = SpiMock::new(&[]);
    let ce = PinMock::new(&[]);

    let radio = Nrf24::new(spi, ce);

    assert_eq!(radio.state(), RadioState::Uninitialized);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
}

#[test]
fn configure_rx_writes_expected_registers() {
    let expectations = configure_rx_expectations();

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);

    let mut radio = Nrf24::new(spi, ce);
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    assert_eq!(radio.configure_rx(&config), Ok(()));

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn verify_rx_rejects_wrong_tx_addr() {
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();
    let expectations = verify_rx_expectations_with_tx_addr(0x0B, [0xC2; 5]);

    let spi = SpiMock::new(&expectations);
    let ce = PinMock::new(&[]);

    let mut radio = Nrf24::new(spi, ce);

    assert_eq!(
        radio.verify_rx_config(&config),
        Err(InitError::VerificationFailed)
    );

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn initialize_inner_leaves_radio_in_standby() {
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    let mut expectations = configure_rx_expectations();
    expectations.extend(verify_rx_expectations(0x0B));

    let spi = SpiMock::new(&expectations);

    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

    let mut radio = Nrf24::new(spi, ce);

    assert_eq!(radio.initialize_rx_inner(&config, &mut delay), Ok(()));

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

#[test]
fn initialize_rx_does_not_listen_when_verification_fails() {
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    let mut expectations = configure_rx_expectations();

    // PRIM_RX is not set.
    expectations.extend(verify_rx_expectations(0x0A));

    let spi = SpiMock::new(&expectations);

    // CE must never go HIGH.
    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

    let mut radio = Nrf24::new(spi, ce);

    assert_eq!(
        radio.initialize_rx_inner(&config, &mut delay),
        Err(InitError::VerificationFailed)
    );

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

#[test]
fn initialize_transitions_to_standby() {
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    let mut expectations = configure_rx_expectations();
    expectations.extend(verify_rx_expectations(0x0B));

    let spi = SpiMock::new(&expectations);

    // Initialization keeps CE low.
    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    // Only the power-up delay is required.
    let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

    let mut radio = Nrf24::new(spi, ce);

    assert_eq!(radio.initialize(&config, &mut delay), Ok(()));
    assert_eq!(radio.state(), RadioState::Standby);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

#[test]
fn initialize_transitions_to_faulted_on_error() {
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    let mut expectations = configure_rx_expectations();

    // PRIM_RX is not set.
    expectations.extend(verify_rx_expectations(0x0A));

    let spi = SpiMock::new(&expectations);

    // CE must never go HIGH.
    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

    let mut radio = Nrf24::new(spi, ce);

    assert_eq!(
        radio.initialize(&config, &mut delay),
        Err(InitError::VerificationFailed)
    );
    assert_eq!(radio.state(), RadioState::Faulted);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

fn switch_expectations(original: u8, updated: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        // Read CONFIG.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![CONFIG, 0xFF], vec![0x00, original]),
        SpiTransaction::transaction_end(),
        // Write CONFIG.
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![W_REGISTER | CONFIG, updated], vec![0x00, 0x00]),
        SpiTransaction::transaction_end(),
    ]
}

#[test]
fn switch_to_tx_clears_prim_rx() {
    let spi = SpiMock::new(&switch_expectations(0x0B, 0x0A));
    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    let mut radio = Nrf24::new(spi, ce);
    radio.state = RadioState::Listening;

    assert_eq!(radio.switch_to_tx_inner(), Ok(()));

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
}

#[test]
fn transmit_faults_when_entering_tx_fails() {
    let spi = SpiMock::new(&[]);

    // CE fails when attempting to enter TX mode.
    let ce = PinMock::new(&[
        PinTransaction::set(PinState::Low).with_error(MockError::Io(ErrorKind::Other))
    ]);

    let mut delay = DelayMock::new(&[]);

    let mut radio = Nrf24::new(spi, ce);

    // Simulate an initialized radio in RX mode.
    radio.state = RadioState::Listening;

    let result = radio.transmit(&[0x42], &mut delay);

    assert!(matches!(
        result,
        Err(TxError::ModeSwitch(InitError::Pin(_)))
    ));

    // A failed hardware transition must fault the driver.
    assert_eq!(radio.state(), RadioState::Faulted);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

#[test]
fn configure_rx_normalizes_crc_after_mcu_only_reset() {
    for original in [0x00, 0x0C, 0x7C] {
        let mut e = configure_rx_expectations();
        let n = e.len();
        e[n - 5] = SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0, original]);
        e[n - 2] =
            SpiTransaction::transfer_in_place(vec![0x20, (original | 0x0B) & !0x04], vec![0, 0]);
        let mut radio = Nrf24::new(SpiMock::new(&e), PinMock::new(&[]));
        radio
            .configure_rx(&RadioConfig::new(50, [0xE7; 5]).unwrap())
            .unwrap();
        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }
}

#[test]
fn verify_rx_rejects_wrong_crc_and_disabled_dpl() {
    for (index, register, value) in [(0, 0x00, 0x03), (0, 0x00, 0x0F), (6, 0x1D, 0), (7, 0x1C, 0)] {
        let mut e = verify_rx_expectations(0x0B);
        e[index * 3 + 1] = SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0, value]);
        let mut radio = Nrf24::new(SpiMock::new(&e), PinMock::new(&[]));
        assert_eq!(
            radio.verify_rx_config(&RadioConfig::new(50, [0xE7; 5]).unwrap()),
            Err(InitError::VerificationFailed)
        );
        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }
}

#[test]
fn start_listening_transitions_from_standby() {
    let spi = SpiMock::new(&[]);

    let ce = PinMock::new(&[PinTransaction::set(PinState::High)]);

    let mut delay = DelayMock::new(&[DelayTransaction::delay_us(RADIO_SETTLING_US)]);

    let mut radio = Nrf24::new(spi, ce);
    radio.state = RadioState::Standby;

    assert_eq!(radio.start_listening(&mut delay), Ok(()));
    assert_eq!(radio.state(), RadioState::Listening);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

#[test]
fn standby_stops_listening() {
    let spi = SpiMock::new(&[]);

    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    let mut radio = Nrf24::new(spi, ce);
    radio.state = RadioState::Listening;

    assert_eq!(radio.standby(), Ok(()));
    assert_eq!(radio.state(), RadioState::Standby);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
}

#[test]
fn power_up_transitions_from_power_down_to_standby() {
    let spi = SpiMock::new(&switch_expectations(0x09, 0x0B));

    let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

    let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

    let mut radio = Nrf24::new(spi, ce);
    radio.state = RadioState::PowerDown;

    assert_eq!(radio.power_up(&mut delay), Ok(()));
    assert_eq!(radio.state(), RadioState::Standby);

    let (mut spi, mut ce) = radio.release();

    spi.done();
    ce.done();
    delay.done();
}

#[test]
fn power_up_rejects_invalid_states_without_hardware_access() {
    for state in [
        RadioState::Uninitialized,
        RadioState::Standby,
        RadioState::Listening,
        RadioState::Faulted,
    ] {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);
        let mut delay = DelayMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);
        radio.state = state;

        assert_eq!(radio.power_up(&mut delay), Err(InitError::NotReady));

        assert_eq!(radio.state(), state);

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
        delay.done();
    }
}

#[test]
fn power_up_pin_failure_faults_driver() {
    let spi = SpiMock::new(&[]);

    let ce = PinMock::new(&[
        PinTransaction::set(PinState::Low).with_error(MockError::Io(ErrorKind::Other))
    ]);

    let mut delay = DelayMock::new(&[]);
    let mut radio = Nrf24::new(spi, ce);

    radio.state = RadioState::PowerDown;

    assert!(matches!(radio.power_up(&mut delay), Err(InitError::Pin(_))));

    assert_eq!(radio.state(), RadioState::Faulted);

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
    delay.done();
}
