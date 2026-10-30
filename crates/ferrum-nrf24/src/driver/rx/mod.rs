use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Polls for one received payload while the radio is listening.
    ///
    /// Requires [`RadioState::Listening`]. Returns `Ok(None)` when the
    /// RX FIFO is empty, or `Ok(Some(length))` after reading one payload.
    ///
    /// A successful call preserves the Listening state. Invalid buffer
    /// sizes and successfully recovered invalid payload widths do not
    /// change the radio state.
    ///
    /// An SPI failure marks the driver as [`RadioState::Faulted`],
    /// because the hardware state or FIFO contents may be uncertain.
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, RxError<SPI::Error>> {
        if self.state != RadioState::Listening {
            return Err(RxError::NotReady);
        }

        let result = self.receive_inner(buffer);

        if matches!(result, Err(RxError::Spi(_))) {
            self.state = RadioState::Faulted;
        }

        result
    }

    fn receive_inner(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, RxError<SPI::Error>> {
        if !self.rx_available().map_err(RxError::Spi)? {
            // Retire any stale notification, without touching TX flags.
            self.write_register(STATUS, RX_DR).map_err(RxError::Spi)?;
            return Ok(None);
        }

        let width = self.read_payload_width().map_err(RxError::Spi)?;
        if !(1..=32).contains(&width) {
            // Datasheet table 20 mandates FLUSH_RX for widths >32. Treat zero
            // similarly: it is outside this driver's supported payload range.
            self.spi.write(&[FLUSH_RX]).map_err(RxError::Spi)?;
            self.write_register(STATUS, RX_DR).map_err(RxError::Spi)?;
            return Err(RxError::InvalidPayloadWidth(width));
        }
        let length = usize::from(width);
        if buffer.len() < length {
            return Err(RxError::BufferTooSmall {
                required: length,
                available: buffer.len(),
            });
        }

        self.read_payload(&mut buffer[..length])
            .map_err(RxError::Spi)?;
        // Write-one-to-clear: never read/modify/write STATUS (TX flags may be set).
        self.write_register(STATUS, RX_DR).map_err(RxError::Spi)?;
        Ok(Some(length))
    }

    fn read_payload_width(&mut self) -> Result<u8, SPI::Error> {
        let mut command = [R_RX_PL_WID, 0xFF];
        self.spi.transfer_in_place(&mut command)?;
        Ok(command[1])
    }

    // Called only with the validated, full payload length. One CSN assertion
    // covers command and data; CSN rising removes exactly this FIFO entry.
    fn read_payload(&mut self, buffer: &mut [u8]) -> Result<(), SPI::Error> {
        self.spi
            .transaction(&mut [Operation::Write(&[R_RX_PAYLOAD]), Operation::Read(buffer)])
    }

    /// Returns whether the RX FIFO contains at least one payload.
    fn rx_available(&mut self) -> Result<bool, SPI::Error> {
        let status = self.read_register(FIFO_STATUS)?;

        Ok(status & RX_EMPTY == 0)
    }
}

#[cfg(test)]
mod tests;
