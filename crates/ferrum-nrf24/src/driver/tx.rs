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
mod tests {
    extern crate std;

    use std::io::ErrorKind;
    use std::vec;
    use std::vec::Vec;

    use super::*;

    use embedded_hal_mock::eh1::MockError;
    use embedded_hal_mock::eh1::delay::{CheckedDelay, NoopDelay, Transaction as DelayTransaction};
    use embedded_hal_mock::eh1::digital::{Mock as PinMock, State, Transaction as PinTransaction};
    use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction as SpiTransaction};

    // embedded-hal-mock 0.11 has no SPI error injection. Validate the
    // transaction normally, then report a bus failure at the selected call.
    struct FailingSpi {
        mock: SpiMock<u8>,
        fail_at: usize,
        calls: usize,
    }

    impl embedded_hal::spi::ErrorType for FailingSpi {
        type Error = embedded_hal::spi::ErrorKind;
    }

    impl SpiDevice for FailingSpi {
        fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
            self.mock.transaction(operations)?;
            let failed = self.calls == self.fail_at;
            self.calls += 1;
            if failed {
                Err(embedded_hal::spi::ErrorKind::Other)
            } else {
                Ok(())
            }
        }
    }

    fn switch_expectations(original: u8, updated: u8) -> Vec<SpiTransaction<u8>> {
        vec![
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x40, original]),
            SpiTransaction::transaction_end(),
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x20, updated], vec![0x40, 0]),
            SpiTransaction::transaction_end(),
        ]
    }

    #[test]
    fn pulse_ce_high_then_low() {
        let spi = SpiMock::new(&[]);

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
        ];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delay = NoopDelay::new();

        radio.pulse_ce(&mut delay).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_success() {
        let expectations = [
            // FLUSH_TX
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
            // Clear STATUS flags.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x30], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Write three-byte payload.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xB0, 0x01, 0x02, 0x03]),
            SpiTransaction::transaction_end(),
            // STATUS: TX_DS = 1
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x07, 0xFF], vec![0x6E, 0x6E]),
            SpiTransaction::transaction_end(),
            // FIFO_STATUS: TX_EMPTY = 1
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x17, 0xFF], vec![0x2E, 0x11]),
            SpiTransaction::transaction_end(),
            // Clear TX_DS.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x20], vec![0x2E, 0x00]),
            SpiTransaction::transaction_end(),
        ];

        let ce_expectations = [
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
        ];

        let expectations = [
            switch_expectations(0x0B, 0x0A),
            expectations.to_vec(),
            switch_expectations(0x0A, 0x0B),
        ]
        .concat();
        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delays = vec![DelayTransaction::delay_us(20)];
        delays.push(DelayTransaction::delay_us(130));
        let mut delay = CheckedDelay::new(&delays);

        radio.transmit(&[0x01, 0x02, 0x03], &mut delay).unwrap();

        assert_eq!(radio.state(), RadioState::Listening);
        delay.done();
        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_max_retries() {
        let expectations = [
            // FLUSH_TX
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
            // Clear STATUS flags.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x30], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Load payload.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xB0, 0x01, 0x02, 0x03]),
            SpiTransaction::transaction_end(),
            // STATUS: MAX_RT = 1.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x07, 0xFF], vec![0x1E, 0x1E]),
            SpiTransaction::transaction_end(),
        ];

        let ce_expectations = [
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
        ];

        let expectations = [
            switch_expectations(0x0B, 0x0A),
            expectations.to_vec(),
            switch_expectations(0x0A, 0x0B),
        ]
        .concat();
        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delays = vec![DelayTransaction::delay_us(20)];
        delays.push(DelayTransaction::delay_us(130));
        let mut delay = CheckedDelay::new(&delays);

        let result = radio.transmit(&[0x01, 0x02, 0x03], &mut delay);

        assert_eq!(result, Err(TxError::MaxRetries));

        assert_eq!(radio.state(), RadioState::Faulted);
        delay.done();
        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_timeout() {
        let mut expectations = vec![
            // FLUSH_TX
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
            // Clear interrupt flags
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x30], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Load payload
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xB0, 0x01]),
            SpiTransaction::transaction_end(),
        ];

        // Radio never reports TX_DS or MAX_RT.
        for _ in 0..100 {
            expectations.extend([
                SpiTransaction::transaction_start(),
                SpiTransaction::transfer_in_place(vec![0x07, 0xFF], vec![0x0E, 0x0E]),
                SpiTransaction::transaction_end(),
            ]);
        }

        let ce_expectations = [
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
        ];

        let expectations = [
            switch_expectations(0x0B, 0x0A),
            expectations.to_vec(),
            switch_expectations(0x0A, 0x0B),
        ]
        .concat();
        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delays = vec![DelayTransaction::delay_us(20)];
        delays.extend((0..100).map(|_| DelayTransaction::delay_us(100)));
        delays.push(DelayTransaction::delay_us(130));
        let mut delay = CheckedDelay::new(&delays);

        let result = radio.transmit(&[0x01], &mut delay);

        assert_eq!(result, Err(TxError::Timeout));
        assert_eq!(radio.state(), RadioState::Faulted);

        // A second transmission must be rejected immediately.
        let second = radio.transmit(&[0x01], &mut delay);

        assert_eq!(second, Err(TxError::NotReady));
        assert_eq!(radio.state(), RadioState::Faulted);

        assert_eq!(radio.state(), RadioState::Faulted);
        delay.done();
        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_fails_when_set_high_fails() {
        let spi = SpiMock::new(&[]);

        let error = MockError::Io(ErrorKind::Other);

        let ce_expectations = [PinTransaction::set(State::High).with_error(error.clone())];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delay = NoopDelay::new();

        let result = radio.pulse_ce(&mut delay);

        assert_eq!(result, Err(error));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_fails_when_set_low_fails() {
        let spi = SpiMock::new(&[]);

        let error = MockError::Io(ErrorKind::Other);

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low).with_error(error.clone()),
            PinTransaction::set(State::Low),
        ];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delay = NoopDelay::new();

        let result = radio.pulse_ce(&mut delay);

        assert_eq!(result, Err(error));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_retries_set_low_after_failure() {
        let spi = SpiMock::new(&[]);
        let error = MockError::Io(ErrorKind::Other);

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low).with_error(error.clone()),
            PinTransaction::set(State::Low),
        ];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Listening;

        let mut delay = NoopDelay::new();

        let result = radio.pulse_ce(&mut delay);

        assert_eq!(result, Err(error));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_rejects_unknown_radio() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);
        let mut delay = NoopDelay::new();

        assert_eq!(radio.state(), RadioState::Uninitialized);

        let result = radio.transmit(&[0x01], &mut delay);

        assert_eq!(result, Err(TxError::NotReady));
        assert_eq!(radio.state(), RadioState::Uninitialized);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_rejects_invalid_payload_without_hardware_access() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);
        radio.state = RadioState::Listening;

        let mut delay = NoopDelay::new();

        let empty = radio.transmit(&[], &mut delay);

        assert_eq!(empty, Err(TxError::Payload(PayloadError::InvalidLength)));

        let oversized = radio.transmit(&[0u8; 33], &mut delay);

        assert_eq!(
            oversized,
            Err(TxError::Payload(PayloadError::InvalidLength))
        );

        assert_eq!(radio.state(), RadioState::Listening);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_rejects_invalid_states_without_hardware_access() {
        for state in [
            RadioState::Uninitialized,
            RadioState::PowerDown,
            RadioState::Faulted,
        ] {
            let mut radio = Nrf24::new(SpiMock::new(&[]), PinMock::new(&[]));

            radio.state = state;

            let mut delay = CheckedDelay::new(&[]);

            assert_eq!(radio.transmit(&[1], &mut delay), Err(TxError::NotReady));

            // Rejected operations must preserve the state.
            assert_eq!(radio.state(), state);

            let (mut spi, mut ce) = radio.release();
            spi.done();
            ce.done();
            delay.done();
        }
    }

    #[test]
    fn transmit_switch_failure_before_tx_faults() {
        // Fail each CONFIG transaction, proving that TX is never started.
        for index in [1, 4] {
            let mut expectations = switch_expectations(0x0B, 0x0A);
            expectations.truncate(index + 2);
            let mut radio = Nrf24::new(
                FailingSpi {
                    mock: SpiMock::new(&expectations),
                    fail_at: index / 3,
                    calls: 0,
                },
                PinMock::new(&[PinTransaction::set(State::Low)]),
            );
            radio.state = RadioState::Listening;
            let mut delay = CheckedDelay::new(&[]);
            assert_eq!(
                radio.transmit(&[1], &mut delay),
                Err(TxError::ModeSwitch(InitError::Configure(
                    ConfigureError::Spi(embedded_hal::spi::ErrorKind::Other)
                )))
            );
            assert_eq!(radio.state(), RadioState::Faulted);
            let (mut spi, mut ce) = radio.release();
            spi.mock.done();
            ce.done();
            delay.done();
        }
    }

    #[test]
    fn tx_spi_failure_still_restores_rx_then_faults() {
        let expectations = [
            switch_expectations(0x0B, 0x0A),
            vec![
                SpiTransaction::transaction_start(),
                SpiTransaction::write_vec(vec![0xE1]),
                SpiTransaction::transaction_end(),
            ],
            switch_expectations(0x0A, 0x0B),
        ]
        .concat();
        let mut radio = Nrf24::new(
            FailingSpi {
                mock: SpiMock::new(&expectations),
                fail_at: 2,
                calls: 0,
            },
            PinMock::new(&[
                PinTransaction::set(State::Low),
                PinTransaction::set(State::Low),
                PinTransaction::set(State::High),
            ]),
        );
        radio.state = RadioState::Listening;
        let mut delay = CheckedDelay::new(&[DelayTransaction::delay_us(130)]);
        assert_eq!(
            radio.transmit(&[1], &mut delay),
            Err(TxError::Spi(embedded_hal::spi::ErrorKind::Other))
        );
        assert_eq!(radio.state(), RadioState::Faulted);
        let (mut spi, mut ce) = radio.release();
        spi.mock.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn restoration_failures_fault_and_preserve_original_tx_error() {
        // Exercise both success and MAX_RT followed by every restoration failure.
        for tx_failed in [false, true] {
            for failure in 0..4 {
                let mut expectations = switch_expectations(0x0B, 0x0A);
                expectations.extend([
                    SpiTransaction::transaction_start(),
                    SpiTransaction::write_vec(vec![0xE1]),
                    SpiTransaction::transaction_end(),
                    SpiTransaction::transaction_start(),
                    SpiTransaction::transfer_in_place(vec![0x27, 0x30], vec![0, 0]),
                    SpiTransaction::transaction_end(),
                    SpiTransaction::transaction_start(),
                    SpiTransaction::write_vec(vec![0xB0, 1]),
                    SpiTransaction::transaction_end(),
                    SpiTransaction::transaction_start(),
                    SpiTransaction::transfer_in_place(
                        vec![0x07, 0xFF],
                        vec![0, if tx_failed { 0x50 } else { 0x60 }],
                    ),
                    SpiTransaction::transaction_end(),
                ]);
                if !tx_failed {
                    expectations.extend([
                        SpiTransaction::transaction_start(),
                        SpiTransaction::transfer_in_place(vec![0x17, 0xFF], vec![0, 0x10]),
                        SpiTransaction::transaction_end(),
                        SpiTransaction::transaction_start(),
                        SpiTransaction::transfer_in_place(vec![0x27, 0x20], vec![0, 0]),
                        SpiTransaction::transaction_end(),
                    ]);
                }
                let pin_error = MockError::Io(ErrorKind::Other);
                let spi_error = embedded_hal::spi::ErrorKind::Other;
                let mut pins = vec![
                    PinTransaction::set(State::Low),
                    PinTransaction::set(State::High),
                    PinTransaction::set(State::Low),
                ];
                let restore_error = if failure == 0 || failure == 3 {
                    InitError::Pin(pin_error.clone())
                } else {
                    InitError::Configure(ConfigureError::Spi(spi_error))
                };
                pins.push(if failure == 0 {
                    PinTransaction::set(State::Low).with_error(pin_error.clone())
                } else {
                    PinTransaction::set(State::Low)
                });
                if failure != 0 {
                    let mut restore = switch_expectations(0x0A, 0x0B);
                    if failure != 3 {
                        let index = if failure == 1 { 1 } else { 4 };
                        restore.truncate(index + 2);
                    } else {
                        pins.push(PinTransaction::set(State::High).with_error(pin_error));
                    }
                    expectations.extend(restore);
                }
                let mut radio = Nrf24::new(
                    FailingSpi {
                        mock: SpiMock::new(&expectations),
                        fail_at: if failure == 1 || failure == 2 {
                            expectations.len() / 3 - 1
                        } else {
                            usize::MAX
                        },
                        calls: 0,
                    },
                    PinMock::new(&pins),
                );
                radio.state = RadioState::Listening;
                let mut delay = CheckedDelay::new(&[DelayTransaction::delay_us(20)]);
                assert_eq!(
                    radio.transmit(&[1], &mut delay),
                    Err(if tx_failed {
                        TxError::MaxRetries
                    } else {
                        TxError::ModeSwitch(restore_error)
                    })
                );
                assert_eq!(radio.state(), RadioState::Faulted);
                assert_eq!(radio.transmit(&[1], &mut delay), Err(TxError::NotReady));
                let (mut spi, mut ce) = radio.release();
                spi.mock.done();
                ce.done();
                delay.done();
            }
        }
    }
}
