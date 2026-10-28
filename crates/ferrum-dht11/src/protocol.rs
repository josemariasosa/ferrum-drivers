/// Errors returned when decoding a DHT11 frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhtError {
    NoInitialLow,
    ChecksumMismatch,
}

/// A validated temperature and humidity reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    pub humidity_integer: u8,
    pub humidity_decimal: u8,
    pub temperature_integer: u8,
    pub temperature_decimal: u8,
}

impl Measurement {
    /// Decode and validate a complete DHT11 frame.
    ///
    /// A frame contains 5 bytes:
    /// - Humidity integer
    /// - Humidity decimal
    /// - Temperature integer
    /// - Temperature decimal
    /// - Checksum
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
