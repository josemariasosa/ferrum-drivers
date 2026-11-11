#![no_std]

pub mod driver;
pub mod matrix;

pub use driver::{Error, MatrixKeypad};
pub use matrix::{KEYMAP_4X4, key_at};
