// crates/ferrum-hd44780/src/protocol.rs

/// HD44780 instruction codes.
pub const CLEAR_DISPLAY: u8 = 0x01;
pub const RETURN_HOME: u8 = 0x02;
pub const ENTRY_MODE_SET: u8 = 0x04;
pub const DISPLAY_CONTROL: u8 = 0x08;
pub const FUNCTION_SET: u8 = 0x20;
pub const SET_DDRAM_ADDRESS: u8 = 0x80;

/// Instruction flags.
pub const ENTRY_INCREMENT: u8 = 0x02;
pub const DISPLAY_ON: u8 = 0x04;
pub const TWO_LINE: u8 = 0x08;

/// Supported display geometries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySize {
    Lcd16x2,
    Lcd20x4,
}

/// Set Character Generator RAM address.
/// CGRAM contains 8 custom characters of 8 bytes each.
pub const SET_CGRAM_ADDRESS: u8 = 0x40;

impl DisplaySize {
    pub const fn columns(self) -> u8 {
        match self {
            Self::Lcd16x2 => 16,
            Self::Lcd20x4 => 20,
        }
    }

    pub const fn rows(self) -> u8 {
        match self {
            Self::Lcd16x2 => 2,
            Self::Lcd20x4 => 4,
        }
    }

    /// Maps a visible row to its DDRAM base address.
    pub const fn row_offset(self, row: u8) -> Option<u8> {
        match (self, row) {
            (Self::Lcd16x2, 0) => Some(0x00),
            (Self::Lcd16x2, 1) => Some(0x40),

            (Self::Lcd20x4, 0) => Some(0x00),
            (Self::Lcd20x4, 1) => Some(0x40),
            (Self::Lcd20x4, 2) => Some(0x14),
            (Self::Lcd20x4, 3) => Some(0x54),

            _ => None,
        }
    }

    pub const fn address(self, column: u8, row: u8) -> Option<u8> {
        if column >= self.columns() {
            return None;
        }

        match self.row_offset(row) {
            Some(offset) => Some(offset + column),
            None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayOrientation {
    #[default]
    Normal,
    FlippedRows,
}

impl DisplayOrientation {
    pub const fn map_row(self, row: u8, rows: u8) -> Option<u8> {
        if row >= rows {
            return None;
        }

        match self {
            Self::Normal => Some(row),
            Self::FlippedRows => Some(rows - 1 - row),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcd1602_dimensions() {
        let size = DisplaySize::Lcd16x2;

        assert_eq!(size.columns(), 16);
        assert_eq!(size.rows(), 2);
    }

    #[test]
    fn lcd1602_ddram_addresses() {
        let size = DisplaySize::Lcd16x2;

        assert_eq!(size.address(0, 0), Some(0x00));
        assert_eq!(size.address(15, 0), Some(0x0F));

        assert_eq!(size.address(0, 1), Some(0x40));
        assert_eq!(size.address(15, 1), Some(0x4F));
    }

    #[test]
    fn lcd2004_ddram_addresses() {
        let size = DisplaySize::Lcd20x4;

        assert_eq!(size.address(0, 0), Some(0x00));
        assert_eq!(size.address(0, 1), Some(0x40));
        assert_eq!(size.address(0, 2), Some(0x14));
        assert_eq!(size.address(0, 3), Some(0x54));
    }

    #[test]
    fn rejects_invalid_positions() {
        let size = DisplaySize::Lcd16x2;

        assert_eq!(size.address(16, 0), None);
        assert_eq!(size.address(0, 2), None);
        assert_eq!(size.address(255, 255), None);
    }

    #[test]
    fn normal_orientation_preserves_rows() {
        let orientation = DisplayOrientation::Normal;

        assert_eq!(orientation.map_row(0, 2), Some(0));
        assert_eq!(orientation.map_row(1, 2), Some(1));
    }

    #[test]
    fn flipped_orientation_reverses_rows() {
        let orientation = DisplayOrientation::FlippedRows;

        assert_eq!(orientation.map_row(0, 2), Some(1));
        assert_eq!(orientation.map_row(1, 2), Some(0));
    }

    #[test]
    fn flipped_orientation_supports_four_rows() {
        let orientation = DisplayOrientation::FlippedRows;

        assert_eq!(orientation.map_row(0, 4), Some(3));
        assert_eq!(orientation.map_row(1, 4), Some(2));
        assert_eq!(orientation.map_row(2, 4), Some(1));
        assert_eq!(orientation.map_row(3, 4), Some(0));
    }

    #[test]
    fn orientation_rejects_invalid_rows() {
        assert_eq!(DisplayOrientation::FlippedRows.map_row(2, 2), None);
    }

    #[test]
    fn cgram_slots_have_eight_bytes() {
        for slot in 0u8..8 {
            let address = SET_CGRAM_ADDRESS | (slot * 8);

            assert_eq!(address, 0x40 + slot * 8);
            assert!(address <= 0x78);
        }
    }
}
