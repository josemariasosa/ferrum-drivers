use super::*;

#[cfg(test)]
mod tests;

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

        let result = self.initialize_inner(config, delay);

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

    /// Powers up the radio from Power Down into Standby-I.
    ///
    /// Requires [`RadioState::PowerDown`].
    /// Preserves the existing register configuration while setting PWR_UP.
    /// On success, CE remains low and the radio enters [`RadioState::Standby`].
    ///
    /// Waits for the radio's power-up settling time before returning.
    /// Does not start RX or transmit any payload.
    ///
    /// Any SPI or GPIO failure transitions the driver to
    /// [`RadioState::Faulted`].
    pub fn power_up<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::PowerDown {
            return Err(InitError::NotReady);
        }

        let result = (|| {
            self.ce.set_low().map_err(InitError::Pin)?;

            let config = self
                .read_register(CONFIG)
                .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

            self.write_register(CONFIG, config | PWR_UP)
                .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

            delay.delay_ms(POWER_UP_DELAY_MS);

            Ok::<(), InitError<SPI::Error, CE::Error>>(())
        })();

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

    fn initialize_inner<D: DelayNs>(
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
