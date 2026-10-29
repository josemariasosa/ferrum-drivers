use embedded_hal_async::delay::DelayNs;

use crate::Measurement;
use crate::hardware::{DhtPin, MicrosecondClock};
use crate::protocol::DhtError;

#[cfg(test)]
mod tests;

/// A DHT11 driver owning its DATA pin and microsecond clock.
///
/// `P` adapts the bidirectional DATA line and `C` measures pulse durations.
pub struct Dht11<P, C> {
    pin: P,
    clock: C,
}

impl<P, C> Dht11<P, C>
where
    P: DhtPin,
    C: MicrosecondClock,
{
    /// Create a driver from a DATA pin and monotonic microsecond clock.
    ///
    /// The caller is responsible for configuring the pin's electrical setup,
    /// including an external DATA pull-up.
    pub fn new(pin: P, clock: C) -> Self {
        Self { pin, clock }
    }

    /// Return the owned pin and clock.
    pub fn into_parts(self) -> (P, C) {
        (self.pin, self.clock)
    }

    /// Read and validate one DHT11 measurement.
    ///
    /// The driver first drives DATA low for 18 ms using the supplied async
    /// delay, then releases the line. It synchronously polls the pin and clock
    /// for the sensor response and 40 data bits; it does not yield to the
    /// executor while observing those pulses. This busy polling can delay
    /// other Embassy tasks for the duration of the transaction.
    ///
    /// Returns [`DhtError::ChecksumMismatch`] for an invalid frame checksum,
    /// protocol-specific errors for missing or out-of-range pulses, and
    /// [`DhtError::PinError`] when any GPIO operation fails. The generic
    /// driver does not retain the underlying platform-specific GPIO error.
    pub async fn read<D>(&mut self, delay: &mut D) -> Result<Measurement, DhtError>
    where
        D: DelayNs,
    {
        self.start_signal(delay)
            .await
            .map_err(|_| DhtError::PinError)?;

        self.validate_response()?;

        let frame = self.read_frame()?;

        Measurement::from_frame(frame)
    }

    async fn start_signal<D>(&mut self, delay: &mut D) -> Result<(), P::Error>
    where
        D: DelayNs,
    {
        self.pin.drive_low()?;

        delay.delay_us(18_000).await;

        self.pin.release()?;

        Ok(())
    }

    fn measure_pulse(
        &mut self,
        high: bool,
        timeout_us: u64,
        timeout_error: DhtError,
    ) -> Result<u64, DhtError> {
        let start = self.clock.now_us();

        loop {
            let level = self.pin.is_high().map_err(|_| DhtError::PinError)?;

            let elapsed = self.clock.now_us().wrapping_sub(start);

            if elapsed > timeout_us {
                return Err(timeout_error);
            }

            if level != high {
                return Ok(elapsed);
            }

            if elapsed == timeout_us {
                return Err(timeout_error);
            }
        }
    }

    fn validate_response(&mut self) -> Result<(), DhtError> {
        self.wait_for_initial_low()?;

        let low_us = self.measure_pulse(false, 150, DhtError::LowDidNotEnd)?;

        if !(50..=110).contains(&low_us) {
            return Err(DhtError::LowInvalidDuration);
        }

        let high_us = self.measure_pulse(true, 150, DhtError::HighDidNotEnd)?;

        if !(50..=110).contains(&high_us) {
            return Err(DhtError::HighInvalidDuration);
        }

        Ok(())
    }

    fn wait_for_initial_low(&mut self) -> Result<(), DhtError> {
        let start = self.clock.now_us();

        loop {
            let low = self.pin.is_low().map_err(|_| DhtError::PinError)?;

            let elapsed = self.clock.now_us().wrapping_sub(start);

            if elapsed > 120 {
                return Err(DhtError::NoInitialLow);
            }

            if low {
                return Ok(());
            }

            if elapsed == 120 {
                return Err(DhtError::NoInitialLow);
            }
        }
    }

    fn read_bit(&mut self) -> Result<bool, DhtError> {
        // DATA is already LOW.
        // Wait until the LOW pulse ends.
        self.measure_pulse(false, 100, DhtError::BitLowDidNotEnd)?;

        // Measure the HIGH pulse.
        let high_us = self.measure_pulse(true, 100, DhtError::BitHighDidNotEnd)?;

        // Short HIGH = 0, long HIGH = 1.
        Ok(high_us > 45)
    }

    fn read_byte(&mut self) -> Result<u8, DhtError> {
        let mut byte = 0u8;

        for _ in 0..8 {
            let bit = self.read_bit()?;

            byte = (byte << 1) | u8::from(bit);
        }

        Ok(byte)
    }

    fn read_frame(&mut self) -> Result<[u8; 5], DhtError> {
        let mut frame = [0u8; 5];

        for byte in &mut frame {
            *byte = self.read_byte()?;
        }

        Ok(frame)
    }
}
