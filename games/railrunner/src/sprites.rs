pub const TRAIN: u8 = 0;
pub const WORM_HEAD: u8 = 1;
pub const WORM_BODY: u8 = 2;
pub const WORM_TAIL: u8 = 3;

/// Player locomotive, facing right.
pub const TRAIN_SPRITE: [u8; 8] = [
    0b00000, 0b00100, 0b01110, 0b11010, 0b11111, 0b11111, 0b01010, 0b01010,
];

/// Sandworm head, moving from left to right.
pub const WORM_HEAD_SPRITE: [u8; 8] = [
    0b00000, 0b00101, 0b00010, 0b11110, 0b11110, 0b00010, 0b00101, 0b00000,
];

pub const WORM_BODY_SPRITE: [u8; 8] = [
    0b00000, 0b00000, 0b00100, 0b11111, 0b11111, 0b00000, 0b00000, 0b00000,
];

pub const WORM_TAIL_SPRITE: [u8; 8] = [
    0b00000, 0b00000, 0b00100, 0b01111, 0b11111, 0b01100, 0b00100, 0b00000,
];
