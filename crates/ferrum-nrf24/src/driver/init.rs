use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Configures and verifies the nRF24L01+ transceiver, leaving it in Standby-I.
    ///
    /// Requires [`RadioState::Uninitialized`]. Drives CE low and configures RF
    /// settings, shared TX/RX addresses, five-byte address width, one-byte CRC,
    /// and dynamic payloads on pipe 0 with selective NO_ACK support.
    ///
    /// After a 2 ms power-up delay, verifies CONFIG, EN_AA, EN_RXADDR, RF_CH,
    /// RF_SETUP, SETUP_AW, FEATURE, DYNPD, and both TX/RX addresses.
    ///
    /// On success, the radio enters [`RadioState::Standby`] with PWR_UP enabled
    /// and CE low. The transceiver is configured and powered up, but is neither
    /// transmitting nor listening. Call `start_listening()` to enter RX mode,
    /// or `transmit()` to send a payload.
    ///
    /// The caller must allow the supply to stabilize and power-on reset to
    /// complete before initialization. The 2 ms delay assumes the datasheet's
    /// recommended crystal (Ls < 30 mH); it does not replace supply-startup
    /// sequencing. FIFOs and stale STATUS flags are not cleared, and no
    /// ACTIVATE fallback is attempted.
    ///
    /// [`InitError::NotReady`] leaves the driver and hardware unchanged.
    /// Any attempted initialization that fails during SPI communication,
    /// CE control, or register verification transitions the driver to
    /// [`RadioState::Faulted`]. The physical radio state may be uncertain,
    /// and CE is not guaranteed low after a GPIO failure.
    pub fn initialize<D: DelayNs>(
        &mut self,
        config: &RadioConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Uninitialized {
            return Err(InitError::NotReady);
        }

        let result = self.initialize_rx_inner(config, delay);

        match result {
            Ok(()) => {
                self.state = RadioState::Standby;
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                Err(error)
            }
        }
    }

    /// Starts listening for incoming packets on RX pipe 0.
    ///
    /// Requires [`RadioState::Standby`].
    /// On success, leaves CE high and the radio in [`RadioState::Listening`].
    /// A GPIO failure faults the driver because the physical CE level
    /// may be uncertain.
    pub fn start_listening<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Standby {
            return Err(InitError::NotReady);
        }

        match self.ce.set_high() {
            Ok(()) => {
                delay.delay_us(RADIO_SETTLING_US);
                self.state = RadioState::Listening;
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                Err(InitError::Pin(error))
            }
        }
    }

    /// Stops listening and enters Standby-I.
    ///
    /// Requires [`RadioState::Listening`].
    /// On success, CE is low and the radio is in [`RadioState::Standby`].
    pub fn standby(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Listening {
            return Err(InitError::NotReady);
        }

        match self.ce.set_low() {
            Ok(()) => {
                self.state = RadioState::Standby;
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                Err(InitError::Pin(error))
            }
        }
    }

    /// Enters Power Down from Standby-I.
    ///
    /// Preserves the radio register configuration while powered.
    /// On success, CE is low and PWR_UP is cleared.
    ///
    /// Requires [`RadioState::Standby`].
    pub fn power_down(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Standby {
            return Err(InitError::NotReady);
        }

        let result = (|| {
            self.ce.set_low().map_err(InitError::Pin)?;

            let config = self
                .read_register(CONFIG)
                .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

            self.write_register(CONFIG, config & !PWR_UP)
                .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

            Ok::<(), InitError<SPI::Error, CE::Error>>(())
        })();

        match result {
            Ok(()) => {
                self.state = RadioState::PowerDown;
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                Err(error)
            }
        }
    }

    pub(super) fn prepare_rx_standby(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.ce.set_low().map_err(InitError::Pin)?;

        let config = self
            .read_register(CONFIG)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        self.write_register(CONFIG, config | PRIM_RX)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        Ok(())
    }

    pub(super) fn switch_to_tx_inner(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.ce.set_low().map_err(InitError::Pin)?;

        let config = self
            .read_register(CONFIG)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        // CE is low: this selects PTX in standby-I. TX settling starts
        // with the CE pulse, not with this register write (datasheet §6.1.7).
        self.write_register(CONFIG, config & !PRIM_RX)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        Ok(())
    }

    fn initialize_rx_inner<D: DelayNs>(
        &mut self,
        config: &RadioConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        // 1. Disable radio activity during configuration.
        self.ce.set_low().map_err(InitError::Pin)?;

        // 2. Configure the radio as PRX.
        self.configure_rx(config).map_err(InitError::Configure)?;

        // 3. Allow the radio to power up and stabilize.
        delay.delay_ms(POWER_UP_DELAY_MS);

        // 4. Verify hardware configuration.
        self.verify_rx_config(config)?;

        Ok(())
    }

    fn configure_rx(&mut self, config: &RadioConfig) -> Result<(), ConfigureError<SPI::Error>> {
        // DPL requires ENAA_P0. Packets sent with W_TX_PAYLOAD_NOACK
        // suppress acknowledgments without disabling dynamic payload framing.
        self.write_register(EN_AA, ERX_P0)
            .map_err(ConfigureError::Spi)?;

        // 2. Enable reception on pipe 0.
        self.write_register(EN_RXADDR, ERX_P0)
            .map_err(ConfigureError::Spi)?;

        // 3. Configure the RF channel.
        self.write_register(RF_CH, config.channel())
            .map_err(ConfigureError::Spi)?;

        // 4. Configure data rate and TX power.
        self.write_register(RF_SETUP, config.rf_setup())
            .map_err(ConfigureError::Spi)?;

        // 5. Configure the address width to 5 bytes.
        self.write_register(SETUP_AW, ADDRESS_WIDTH_5_BYTES)
            .map_err(ConfigureError::Spi)?;

        // 6. Configure the receive address.
        self.write_register_bytes(RX_ADDR_P0, &config.address())
            .map_err(ConfigureError::Register)?;

        // Keep TX_ADDR equal to RX_ADDR_P0 so a later RX -> TX switch is ready.
        self.write_register_bytes(TX_ADDR, &config.address())
            .map_err(ConfigureError::Register)?;

        // 7. Enable Dynamic Payload Length on pipe 0.
        self.write_register(FEATURE, EN_DPL | EN_DYN_ACK)
            .map_err(ConfigureError::Spi)?;

        self.write_register(DYNPD, DPL_P0)
            .map_err(ConfigureError::Spi)?;

        // 8. Power up the radio in primary receiver mode.
        let original = self.read_register(CONFIG).map_err(ConfigureError::Spi)?;

        // Use the same explicit one-byte CRC on both peers, even after MCU-only reset.
        let rx_config = (original | EN_CRC | PWR_UP | PRIM_RX) & !CRCO;

        self.write_register(CONFIG, rx_config)
            .map_err(ConfigureError::Spi)?;

        Ok(())
    }

    fn verify_rx_config(
        &mut self,
        config: &RadioConfig,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        let read = |radio: &mut Self, register| {
            radio
                .read_register(register)
                .map_err(|error| InitError::Configure(ConfigureError::Spi(error)))
        };

        let config_reg = read(self, CONFIG)?;
        let en_aa = read(self, EN_AA)?;
        let en_rxaddr = read(self, EN_RXADDR)?;
        let channel = read(self, RF_CH)?;
        let rf_setup = read(self, RF_SETUP)?;
        let address_width = read(self, SETUP_AW)?;
        let feature = read(self, FEATURE)?;
        let dynpd = read(self, DYNPD)?;

        let mut address = [0u8; 5];

        self.read_register_bytes(RX_ADDR_P0, &mut address)
            .map_err(|error| InitError::Configure(ConfigureError::Register(error)))?;

        let mut tx_address = [0u8; 5];

        self.read_register_bytes(TX_ADDR, &mut tx_address)
            .map_err(|error| InitError::Configure(ConfigureError::Register(error)))?;

        if config_reg & (EN_CRC | CRCO | PWR_UP | PRIM_RX) != (EN_CRC | PWR_UP | PRIM_RX)
            || en_aa != ERX_P0
            || en_rxaddr != ERX_P0
            || channel != config.channel()
            || rf_setup != config.rf_setup()
            || address_width != ADDRESS_WIDTH_5_BYTES
            || feature != (EN_DPL | EN_DYN_ACK)
            || dynpd & DPL_P0 == 0
            || address != config.address()
            || tx_address != config.address()
        {
            return Err(InitError::VerificationFailed);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
            e[n - 2] = SpiTransaction::transfer_in_place(
                vec![0x20, (original | 0x0B) & !0x04],
                vec![0, 0],
            );
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
        for (index, register, value) in
            [(0, 0x00, 0x03), (0, 0x00, 0x0F), (6, 0x1D, 0), (7, 0x1C, 0)]
        {
            let mut e = verify_rx_expectations(0x0B);
            e[index * 3 + 1] =
                SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0, value]);
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
}
