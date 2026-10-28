/// Errors returned while communicating with or decoding a DHT11 frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhtError {
    /// The sensor did not pull DATA low before the response timeout.
    NoInitialLow,
    /// The sensor's response low pulse did not end before the timeout.
    LowDidNotEnd,
    /// The sensor's response low pulse was outside the accepted duration.
    LowInvalidDuration,
    /// The sensor's response high pulse did not end before the timeout.
    HighDidNotEnd,
    /// The sensor's response high pulse was outside the accepted duration.
    HighInvalidDuration,

    /// A data-bit low pulse did not end before the timeout.
    BitLowDidNotEnd,
    /// A data-bit high pulse did not end before the timeout.
    BitHighDidNotEnd,
    /// The sum of the first four frame bytes did not match the checksum byte.
    ChecksumMismatch,

    /// A GPIO operation failed during the start signal.
    ///
    /// GPIO errors encountered while polling the sensor are currently
    /// reported as protocol timeout/no-response errors instead.
    PinError,
}

/// A validated temperature and humidity reading.
///
/// The integer and decimal components are the bytes transmitted by the DHT11;
/// they are not combined or scaled by this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    /// Integer humidity component, conventionally expressed as percent RH.
    pub humidity_integer: u8,
    /// Decimal humidity component as transmitted by the sensor.
    pub humidity_decimal: u8,
    /// Integer temperature component, conventionally expressed in degrees Celsius.
    pub temperature_integer: u8,
    /// Decimal temperature component as transmitted by the sensor.
    pub temperature_decimal: u8,
}

impl Measurement {
    /// Decode and validate a complete DHT11 frame.
    ///
    /// The frame contains humidity integer and decimal bytes, temperature
    /// integer and decimal bytes, and a checksum byte. The checksum is the
    /// wrapping sum of the first four bytes.
    pub fn from_frame(frame: [u8; 5]) -> Result<Self, DhtError> {
        let expected = frame[..4]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));

        if expected != frame[4] {
            return Err(DhtError::ChecksumMismatch);
        }

        Ok(Self {
            humidity_integer: frame[0],
            humidity_decimal: frame[1],
            temperature_integer: frame[2],
            temperature_decimal: frame[3],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_valid_frame() {
        let frame = [55, 0, 24, 0, 79];

        let measurement = Measurement::from_frame(frame).unwrap();

        assert_eq!(measurement.humidity_integer, 55);
        assert_eq!(measurement.humidity_decimal, 0);
        assert_eq!(measurement.temperature_integer, 24);
        assert_eq!(measurement.temperature_decimal, 0);
    }

    #[test]
    fn reject_invalid_checksum() {
        let frame = [55, 0, 24, 0, 80];

        let result = Measurement::from_frame(frame);

        assert_eq!(result, Err(DhtError::ChecksumMismatch));
    }

    #[test]
    fn checksum_wraps_at_u8_boundary() {
        let frame = [200, 100, 30, 0, 74];

        let result = Measurement::from_frame(frame);

        assert!(result.is_ok());
    }
}
