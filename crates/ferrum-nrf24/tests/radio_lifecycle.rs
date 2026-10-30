use embedded_hal_mock::eh1::{
    delay::{CheckedDelay, Transaction as DelayTransaction},
    digital::{Mock as PinMock, State as PinState, Transaction as PinTransaction},
    spi::{Mock as SpiMock, Transaction as SpiTransaction},
};
use ferrum_nrf24::{Nrf24, RadioConfig, RadioState};

const POWER_UP_DELAY_MS: u32 = 2;
const RADIO_SETTLING_US: u32 = 130;

fn register_write(reg: u8, value: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![0x20 | reg, value], vec![0x0E, 0x00]),
        SpiTransaction::transaction_end(),
    ]
}

fn register_read(reg: u8, value: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer_in_place(vec![reg, 0xFF], vec![0x0E, value]),
        SpiTransaction::transaction_end(),
    ]
}

fn address_write(reg: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![0x20 | reg, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
        SpiTransaction::transaction_end(),
    ]
}

fn address_read(reg: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![reg]),
        SpiTransaction::read_vec(vec![0xE7; 5]),
        SpiTransaction::transaction_end(),
    ]
}

fn initialization_expectations() -> Vec<SpiTransaction<u8>> {
    let mut e = Vec::new();

    // configure_rx()
    for (reg, value) in [
        (0x01, 0x01), // EN_AA
        (0x02, 0x01), // EN_RXADDR
        (0x05, 0x32), // RF_CH = 50
        (0x06, 0x00), // RF_SETUP
        (0x03, 0x03), // SETUP_AW
    ] {
        e.extend(register_write(reg, value));
    }

    e.extend(address_write(0x0A)); // RX_ADDR_P0
    e.extend(address_write(0x10)); // TX_ADDR
    e.extend(register_write(0x1D, 0x05)); // FEATURE
    e.extend(register_read(0x1D, 0x05)); // FEATURE access probe
    e.extend(register_write(0x1C, 0x01)); // DYNPD

    // Read CONFIG, then enable PWR_UP, PRIM_RX and CRC.
    e.extend(register_read(0x00, 0x08));
    e.extend(register_write(0x00, 0x0B));

    // verify_rx_config()
    for (reg, value) in [
        (0x00, 0x0B),
        (0x01, 0x01),
        (0x02, 0x01),
        (0x05, 0x32),
        (0x06, 0x00),
        (0x03, 0x03),
        (0x1D, 0x05),
        (0x1C, 0x01),
    ] {
        e.extend(register_read(reg, value));
    }

    e.extend(address_read(0x0A));
    e.extend(address_read(0x10));

    e
}

#[test]
fn radio_completes_power_and_listening_lifecycle() {
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    let mut expectations = initialization_expectations();

    // Standby -> PowerDown
    expectations.extend(register_read(0x00, 0x0B));
    expectations.extend(register_write(0x00, 0x09));

    // PowerDown -> Standby
    expectations.extend(register_read(0x00, 0x09));
    expectations.extend(register_write(0x00, 0x0B));

    let spi = SpiMock::new(&expectations);

    let ce = PinMock::new(&[
        PinTransaction::set(PinState::Low),  // initialize
        PinTransaction::set(PinState::High), // start_listening
        PinTransaction::set(PinState::Low),  // standby
        PinTransaction::set(PinState::Low),  // power_down
        PinTransaction::set(PinState::Low),  // power_up
    ]);

    let mut delay = CheckedDelay::new(&[
        DelayTransaction::delay_ms(POWER_UP_DELAY_MS),
        DelayTransaction::delay_us(RADIO_SETTLING_US),
        DelayTransaction::delay_ms(POWER_UP_DELAY_MS),
    ]);

    let mut radio = Nrf24::new(spi, ce);

    assert_eq!(radio.state(), RadioState::Uninitialized);

    radio.initialize(&config, &mut delay).unwrap();
    assert_eq!(radio.state(), RadioState::Standby);

    radio.start_listening(&mut delay).unwrap();
    assert_eq!(radio.state(), RadioState::Listening);

    radio.standby().unwrap();
    assert_eq!(radio.state(), RadioState::Standby);

    radio.power_down().unwrap();
    assert_eq!(radio.state(), RadioState::PowerDown);

    radio.power_up(&mut delay).unwrap();
    assert_eq!(radio.state(), RadioState::Standby);

    let (mut spi, mut ce) = radio.release();
    spi.done();
    ce.done();
    delay.done();
}
