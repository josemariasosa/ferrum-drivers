use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Reads a single-byte register via SPI.
    pub(super) fn read_register(&mut self, register: u8) -> Result<u8, SPI::Error> {
        let command = R_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, 0xFF];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(buffer[1])
    }

    /// Writes a single-byte register via SPI.
    pub(super) fn write_register(&mut self, register: u8, value: u8) -> Result<(), SPI::Error> {
        let command = W_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, value];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(())
    }

    /// Reads 1..=5 bytes from a register into `output`.
    ///
    /// Returns `RegisterError::InvalidLength` for invalid lengths.
    pub(super) fn read_register_bytes(
        &mut self,
        register: u8,
        output: &mut [u8],
    ) -> Result<(), RegisterError<SPI::Error>> {
        if output.is_empty() || output.len() > 5 {
            return Err(RegisterError::InvalidLength);
        }

        let command = [R_REGISTER | (register & REGISTER_MASK)];

        self.spi
            .transaction(&mut [Operation::Write(&command), Operation::Read(output)])
            .map_err(RegisterError::Spi)?;

        Ok(())
    }

    /// Writes 1..=5 bytes to a register.
    ///
    /// Returns `RegisterError::InvalidLength` for invalid lengths.
    pub(super) fn write_register_bytes(
        &mut self,
        register: u8,
        values: &[u8],
    ) -> Result<(), RegisterError<SPI::Error>> {
        if values.is_empty() || values.len() > 5 {
            return Err(RegisterError::InvalidLength);
        }

        let mut buffer = [0u8; 6];

        buffer[0] = W_REGISTER | (register & REGISTER_MASK);
        buffer[1..1 + values.len()].copy_from_slice(values);

        self.spi
            .write(&buffer[..1 + values.len()])
            .map_err(RegisterError::Spi)?;

        Ok(())
    }

    /// Loads a payload of 1..=32 bytes into the TX FIFO.
    ///
    /// Uses selective NO_ACK; requires FEATURE.EN_DYN_ACK from initialize().
    /// Does not trigger transmission.
    pub(super) fn write_payload(&mut self, payload: &[u8]) -> Result<(), PayloadError<SPI::Error>> {
        if payload.is_empty() || payload.len() > 32 {
            return Err(PayloadError::InvalidLength);
        }

        let mut buffer = [0u8; 33];

        buffer[0] = W_TX_PAYLOAD_NOACK;
        buffer[1..1 + payload.len()].copy_from_slice(payload);

        self.spi
            .write(&buffer[..1 + payload.len()])
            .map_err(PayloadError::Spi)?;

        Ok(())
    }

    /// Discards all pending payloads from the TX FIFO.
    pub(super) fn flush_tx(&mut self) -> Result<(), SPI::Error> {
        self.spi.write(&[FLUSH_TX])
    }
}

#[cfg(test)]
mod tests;
