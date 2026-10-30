/// Failure while loading a payload into the TX FIFO.
#[derive(Debug, PartialEq, Eq)]
pub enum PayloadError<E> {
    /// Payload length is outside the supported range of 1–32 bytes.
    ///
    /// When returned by [`crate::Nrf24::transmit`], no hardware access has
    /// occurred and the driver remains in its previous stable state.
    InvalidLength,
    /// The SPI device failed while writing the payload.
    ///
    /// During transmission, the previous stable state is restored if possible,
    /// but the driver is left faulted even if restoration succeeds.
    Spi(E),
}

/// Error returned by the driver's register-access helpers.
#[derive(Debug, PartialEq, Eq)]
pub enum RegisterError<E> {
    /// A multi-byte register operation was not between 1 and 5 bytes.
    ///
    /// The public initialization workflow uses fixed five-byte addresses, so
    /// this is not expected from it.
    InvalidLength,
    /// The SPI device failed during the register operation.
    Spi(E),
}

/// Failure while constructing or validating an RF configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The requested channel is outside the nRF24L01+ range `0..=125`.
    InvalidChannel,
}

/// Failure while configuring a radio register.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigureError<E> {
    /// The SPI device failed during a single-byte register operation.
    Spi(E),
    /// A multi-byte register operation failed or had an invalid length.
    Register(RegisterError<E>),
}

/// Failure while transmitting a payload.
#[derive(Debug, PartialEq, Eq)]
pub enum TxError<SpiError, PinError> {
    /// The radio is not in Standby or Listening, or has faulted.
    ///
    /// This check does not access hardware or change the current state.
    NotReady,
    /// SPI failed during transmission or mode handling.
    ///
    /// Once TX mode has been entered, the previous stable state is restored
    /// if possible, but the driver is faulted even if restoration succeeds.
    Spi(SpiError),
    /// A CE GPIO operation failed.
    ///
    /// Once TX mode has been entered, the driver attempts to restore the
    /// previous stable state and then remains faulted; the physical CE level
    /// may be uncertain.
    Pin(PinError),
    /// The payload was invalid or could not be loaded into the TX FIFO.
    Payload(PayloadError<SpiError>),
    /// No TX completion status was observed within the bounded poll loop.
    ///
    /// Restoration of the previous stable state is attempted, then the driver
    /// is faulted.
    Timeout,
    /// The radio reported `MAX_RT`.
    ///
    /// Restoration of the previous stable state is attempted, then the driver
    /// is faulted. The public transmit workflow uses NO_ACK, so automatic retry
    /// exhaustion is not expected.
    MaxRetries,
    /// TX completion was reported but the TX FIFO was not empty.
    ///
    /// Restoration of the previous stable state is attempted, then the driver
    /// is faulted.
    FifoNotEmpty,
    /// Entering TX mode or restoring the previous stable mode failed.
    ///
    /// A failed entry switch faults immediately. A failed restoration switch
    /// also faults; if TX had already failed, the original TX error takes
    /// precedence and the restoration error is not returned.
    ModeSwitch(InitError<SpiError, PinError>),
}

/// Failure while initializing or switching the radio operating mode.
#[derive(Debug, PartialEq, Eq)]
pub enum InitError<SpiError, PinError> {
    /// Initialization or mode switching was invalid for the current state.
    ///
    /// A rejected public initialization leaves the existing state unchanged.
    NotReady,
    /// A register configuration or readback transaction failed.
    Configure(ConfigureError<SpiError>),
    /// A CE pin operation failed; the physical pin level may be uncertain.
    Pin(PinError),
    /// Configuration readback did not match the requested settings.
    ///
    /// A failed initialization marks the driver faulted.
    VerificationFailed,
}

/// Receive errors. Only SPI failures fault the driver.
#[derive(Debug, PartialEq, Eq)]
pub enum RxError<SpiError> {
    /// Initialization has not succeeded, the driver is faulted, or it is not
    /// in Listening.
    ///
    /// No hardware access is performed by this rejection.
    NotReady,
    /// The oldest payload remains queued; retry with at least `required` bytes.
    ///
    /// `available` is the supplied buffer length. This error does not fault the
    /// radio and does not consume or modify the payload or buffer.
    BufferTooSmall { required: usize, available: usize },
    /// Width is outside 1–32. The RX FIFO was flushed successfully, discarding
    /// all queued payloads, including any otherwise valid ones. The radio
    /// remains usable in Listening.
    InvalidPayloadWidth(u8),
    /// SPI failed. The payload may already have been consumed or the buffer
    /// partially written; the radio is marked faulted and the mode unknown.
    Spi(SpiError),
}
