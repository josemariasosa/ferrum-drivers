use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Transmits a single payload and restores the previous radio state.
    ///
    /// Requires [`RadioState::Standby`] or [`RadioState::Listening`] and a
    /// payload of 1..=32 bytes. Transmission from PowerDown, Uninitialized,
    /// or Faulted is rejected without hardware access.
    ///
    /// Temporarily switches the transceiver to TX mode, flushes the TX FIFO,
    /// clears TX interrupt flags, loads the payload, pulses CE, and polls
    /// STATUS until transmission completes or times out.
    ///
    /// On successful transmission, verifies that the TX FIFO is empty and
    /// clears TX_DS. The previous stable radio state is then restored:
    ///
    /// - Standby -> TX -> Standby
    /// - Listening -> TX -> Listening
    ///
    /// When restoring Listening, the driver re-enables RX and waits 130 us
    /// for the radio to settle. RX is unavailable during transmission.
    ///
    /// Uses W_TX_PAYLOAD_NOACK with FEATURE.EN_DYN_ACK enabled during
    /// initialization. No acknowledgment is requested and no automatic
    /// retransmission is performed. TX completion does not guarantee that
    /// another device received the payload.
    ///
    /// Pending RX payloads and RX_DR are preserved. Any previously queued
    /// TX payloads are discarded before transmission.
    ///
    /// Invalid payload length and invalid starting state cause no hardware
    /// I/O and preserve the current radio state.
    ///
    /// If entering TX mode fails, the driver transitions to
    /// [`RadioState::Faulted`] without attempting restoration.
    ///
    /// Once TX mode has been entered, restoration of the previous stable
    /// state is attempted regardless of the transmission result. Any
    /// transmission or restoration failure leaves the driver Faulted.
    /// If both fail, the original transmission error takes precedence.
    ///
    /// A restoration failure may occur after the payload was transmitted.
    /// Retrying the same application message may therefore produce duplicates.
    ///
    /// The timeout uses at most 100 STATUS polls separated by 100 us delays.
    /// It is not an exact wall-clock deadline.
    pub fn transmit<D: DelayNs>(
        &mut self,
        payload: &[u8],
        delay: &mut D,
    ) -> Result<(), TxError<SPI::Error, CE::Error>> {
        let previous_state = self.state;

        if !matches!(previous_state, RadioState::Standby | RadioState::Listening) {
            return Err(TxError::NotReady);
        }

        if payload.is_empty() || payload.len() > 32 {
            return Err(TxError::Payload(PayloadError::InvalidLength));
        }

        // Enter TX mode.
        if let Err(error) = self.switch_to_tx_inner() {
            self.state = RadioState::Faulted;
            return Err(TxError::ModeSwitch(error));
        }

        // Perform the actual transmission.
        let tx_result = self.transmit_inner(payload, delay);

        // Restore RX configuration in Standby.
        let restore_result = (|| {
            self.prepare_rx_standby()?;

            if previous_state == RadioState::Listening {
                self.ce.set_high().map_err(InitError::Pin)?;
                delay.delay_us(RADIO_SETTLING_US);
            }

            Ok::<_, InitError<SPI::Error, CE::Error>>(())
        })();

        match (tx_result, restore_result) {
            (Ok(()), Ok(())) => {
                self.state = previous_state;
                Ok(())
            }

            (Err(error), Ok(())) => {
                self.state = RadioState::Faulted;
                Err(error)
            }

            (Ok(()), Err(error)) => {
                self.state = RadioState::Faulted;
                Err(TxError::ModeSwitch(error))
            }

            (Err(error), Err(_)) => {
                self.state = RadioState::Faulted;
                Err(error)
            }
        }
    }

    /// Generates a 20 us CE pulse.
    ///
    /// If setting CE LOW fails, retries once as a best-effort
    /// recovery and returns the original error.
    ///
    /// Does not guarantee the physical pin level after a GPIO error.
    fn pulse_ce<D: DelayNs>(&mut self, delay: &mut D) -> Result<(), CE::Error> {
        self.ce.set_high()?;

        delay.delay_us(2 * MIN_CE_PULSE_US);

        if let Err(error) = self.ce.set_low() {
            // Best-effort recovery: try to return CE to LOW.
            let _ = self.ce.set_low();

            return Err(error);
        }

        Ok(())
    }

    fn transmit_inner<D: DelayNs>(
        &mut self,
        payload: &[u8],
        delay: &mut D,
    ) -> Result<(), TxError<SPI::Error, CE::Error>> {
        // 1. Start with a clean TX FIFO.
        self.flush_tx().map_err(TxError::Spi)?;

        // 2. Clear only TX flags; preserve RX_DR and pending RX payloads.
        self.write_register(STATUS, TX_DS | MAX_RT)
            .map_err(TxError::Spi)?;

        // 3. Load exactly one payload.
        self.write_payload(payload).map_err(TxError::Payload)?;

        // 4. Trigger RF transmission.
        self.pulse_ce(delay).map_err(TxError::Pin)?;

        // 5. Poll STATUS with a bounded timeout.
        for _ in 0..100 {
            let status = self.read_register(STATUS).map_err(TxError::Spi)?;

            if status & MAX_RT != 0 {
                return Err(TxError::MaxRetries);
            }

            if status & TX_DS != 0 {
                // 6. Confirm that the payload left the FIFO.
                let fifo = self.read_register(FIFO_STATUS).map_err(TxError::Spi)?;

                if fifo & TX_EMPTY == 0 {
                    return Err(TxError::FifoNotEmpty);
                }

                // 7. Clear TX completion flag.
                self.write_register(STATUS, TX_DS).map_err(TxError::Spi)?;

                return Ok(());
            }

            delay.delay_us(100);
        }

        Err(TxError::Timeout)
    }
}

#[cfg(test)]
mod tests;
