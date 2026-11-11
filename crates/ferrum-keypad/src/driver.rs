use embedded_hal::digital::{InputPin, OutputPin};

use crate::matrix::key_at;

#[derive(Debug)]
pub enum Error<RowError, ColumnError> {
    Row(RowError),
    Column(ColumnError),
}

pub struct MatrixKeypad<R, C> {
    rows: [R; 4],
    columns: [C; 4],
}

impl<R, C> MatrixKeypad<R, C>
where
    R: OutputPin,
    C: InputPin,
{
    pub fn new(mut rows: [R; 4], columns: [C; 4]) -> Result<Self, Error<R::Error, C::Error>> {
        for row in &mut rows {
            row.set_high().map_err(Error::Row)?;
        }

        Ok(Self { rows, columns })
    }

    /// Scans the keypad and returns the first detected key.
    ///
    /// Column inputs must have pull-up resistors.
    pub fn scan(&mut self) -> Result<Option<char>, Error<R::Error, C::Error>> {
        for row_index in 0..4 {
            self.rows[row_index].set_low().map_err(Error::Row)?;

            let mut pressed = None;

            for column_index in 0..4 {
                match self.columns[column_index].is_low() {
                    Ok(true) => {
                        pressed = key_at(row_index, column_index);
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        let _ = self.rows[row_index].set_high();
                        return Err(Error::Column(error));
                    }
                }
            }

            self.rows[row_index].set_high().map_err(Error::Row)?;

            if pressed.is_some() {
                return Ok(pressed);
            }
        }

        Ok(None)
    }
}
