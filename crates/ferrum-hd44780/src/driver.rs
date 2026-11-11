// crates/ferrum-hd44780/src/driver.rs

use embedded_hal::{delay::DelayNs, digital::OutputPin};

use crate::protocol::{
    CLEAR_DISPLAY, DISPLAY_CONTROL, DISPLAY_ON, DisplayOrientation, DisplaySize, ENTRY_INCREMENT,
    ENTRY_MODE_SET, FUNCTION_SET, SET_DDRAM_ADDRESS, TWO_LINE,
};

#[derive(Debug)]
pub enum Error<E> {
    Pin(E),
    InvalidPosition,
    NonAsciiCharacter,
}

/// HD44780 driver using a 4-bit parallel interface.
///
/// RW must be connected to GND.
pub struct Hd44780<P, D> {
    rs: P,
    enable: P,
    data: [P; 4], // D4, D5, D6, D7
    delay: D,
    size: DisplaySize,
    orientation: DisplayOrientation,
}

impl<P, D> Hd44780<P, D>
where
    P: OutputPin,
    D: DelayNs,
{
    pub fn new(rs: P, enable: P, data: [P; 4], delay: D, size: DisplaySize) -> Self {
        Self {
            rs,
            enable,
            data,
            delay,
            size,
            orientation: DisplayOrientation::Normal,
        }
    }

    pub fn set_orientation(&mut self, orientation: DisplayOrientation) {
        self.orientation = orientation;
    }

    fn write_nibble(&mut self, nibble: u8) -> Result<(), Error<P::Error>> {
        // Map bit 0 to D4, bit 1 to D5, etc.
        for (bit, pin) in self.data.iter_mut().enumerate() {
            let high = (nibble & (1 << bit)) != 0;

            if high {
                pin.set_high().map_err(Error::Pin)?;
            } else {
                pin.set_low().map_err(Error::Pin)?;
            }
        }

        // Allow data lines to settle before E rises.
        self.delay.delay_us(1);

        self.enable.set_high().map_err(Error::Pin)?;
        self.delay.delay_us(1);

        self.enable.set_low().map_err(Error::Pin)?;
        self.delay.delay_us(1);

        Ok(())
    }

    fn write_byte(&mut self, value: u8, is_data: bool) -> Result<(), Error<P::Error>> {
        self.enable.set_low().map_err(Error::Pin)?;

        if is_data {
            self.rs.set_high().map_err(Error::Pin)?;
        } else {
            self.rs.set_low().map_err(Error::Pin)?;
        }

        self.write_nibble(value >> 4)?;
        self.write_nibble(value & 0x0F)?;

        // Conservative execution time for ordinary instructions.
        self.delay.delay_us(50);

        Ok(())
    }

    fn command(&mut self, value: u8) -> Result<(), Error<P::Error>> {
        self.write_byte(value, false)
    }

    fn data(&mut self, value: u8) -> Result<(), Error<P::Error>> {
        self.write_byte(value, true)
    }

    pub fn init(&mut self) -> Result<(), Error<P::Error>> {
        self.enable.set_low().map_err(Error::Pin)?;
        self.rs.set_low().map_err(Error::Pin)?;

        // Wait for power and controller startup.
        self.delay.delay_ms(40);

        // Special initialization sequence.
        // These are single nibbles, not full bytes.
        self.write_nibble(0x03)?;
        self.delay.delay_ms(5);

        self.write_nibble(0x03)?;
        self.delay.delay_us(200);

        self.write_nibble(0x03)?;
        self.delay.delay_us(200);

        // Switch to 4-bit mode.
        self.write_nibble(0x02)?;
        self.delay.delay_us(200);

        // 4-bit interface, two-line controller mode.
        self.command(FUNCTION_SET | TWO_LINE)?;

        // Display off during configuration.
        self.command(DISPLAY_CONTROL)?;

        self.clear()?;

        // Cursor increments after each character.
        self.command(ENTRY_MODE_SET | ENTRY_INCREMENT)?;

        // Display on, cursor off, blinking off.
        self.command(DISPLAY_CONTROL | DISPLAY_ON)?;

        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), Error<P::Error>> {
        self.command(CLEAR_DISPLAY)?;

        // Clear display is a slow instruction.
        self.delay.delay_ms(2);

        Ok(())
    }

    /// Column and row are zero-indexed.
    pub fn set_cursor(&mut self, column: u8, row: u8) -> Result<(), Error<P::Error>> {
        let physical_row = self
            .orientation
            .map_row(row, self.size.rows())
            .ok_or(Error::InvalidPosition)?;

        let address = self
            .size
            .address(column, physical_row)
            .ok_or(Error::InvalidPosition)?;

        self.command(SET_DDRAM_ADDRESS | address)
    }

    /// Writes an ASCII byte to the display.
    pub fn write_char(&mut self, character: u8) -> Result<(), Error<P::Error>> {
        if !character.is_ascii() {
            return Err(Error::NonAsciiCharacter);
        }

        self.data(character)
    }

    /// Writes an ASCII string without allocating memory.
    pub fn write_str(&mut self, text: &str) -> Result<(), Error<P::Error>> {
        if !text.is_ascii() {
            return Err(Error::NonAsciiCharacter);
        }

        for byte in text.bytes() {
            self.write_char(byte)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use embedded_hal_mock::eh1::{
        delay::NoopDelay,
        digital::{Mock as PinMock, State, Transaction as PinTransaction},
    };

    #[test]
    fn writes_nibble_to_correct_data_pins() {
        // 0b1010:
        // D4 = 0
        // D5 = 1
        // D6 = 0
        // D7 = 1

        let rs = PinMock::new(&[]);
        let enable = PinMock::new(&[
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
        ]);

        let d4 = PinMock::new(&[PinTransaction::set(State::Low)]);

        let d5 = PinMock::new(&[PinTransaction::set(State::High)]);

        let d6 = PinMock::new(&[PinTransaction::set(State::Low)]);

        let d7 = PinMock::new(&[PinTransaction::set(State::High)]);

        let mut lcd = Hd44780::new(
            rs,
            enable,
            [d4, d5, d6, d7],
            NoopDelay::new(),
            DisplaySize::Lcd16x2,
        );

        lcd.write_nibble(0b1010).unwrap();

        lcd.rs.done();
        lcd.enable.done();

        for pin in &mut lcd.data {
            pin.done();
        }
    }
}
