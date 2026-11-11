/// Logical layout of a 4x4 matrix keypad.
pub const KEYMAP_4X4: [[char; 4]; 4] = [
    ['1', '2', '3', 'A'],
    ['4', '5', '6', 'B'],
    ['7', '8', '9', 'C'],
    ['*', '0', '#', 'D'],
];

/// Returns the character at a given row and column.
pub const fn key_at(row: usize, column: usize) -> Option<char> {
    if row >= 4 || column >= 4 {
        return None;
    }

    Some(KEYMAP_4X4[row][column])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_numeric_keys() {
        assert_eq!(key_at(0, 0), Some('1'));
        assert_eq!(key_at(1, 1), Some('5'));
        assert_eq!(key_at(2, 2), Some('9'));
        assert_eq!(key_at(3, 1), Some('0'));
    }

    #[test]
    fn maps_special_keys() {
        assert_eq!(key_at(0, 3), Some('A'));
        assert_eq!(key_at(3, 0), Some('*'));
        assert_eq!(key_at(3, 2), Some('#'));
        assert_eq!(key_at(3, 3), Some('D'));
    }

    #[test]
    fn rejects_out_of_bounds_coordinates() {
        assert_eq!(key_at(4, 0), None);
        assert_eq!(key_at(0, 4), None);
        assert_eq!(key_at(255, 255), None);
    }
}
