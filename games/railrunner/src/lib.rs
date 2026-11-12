#![no_std]

pub mod sprites;

pub const WIDTH: u8 = 16;
pub const HEIGHT: u8 = 2;

pub const TRAIN_MIN_COLUMN: u8 = 4;
pub const TRAIN_MAX_COLUMN: u8 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Upper,
    Lower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Train {
    pub column: u8,
    pub track: Track,
}

pub struct Game {
    pub train: Train,
    pub tick: u32,
    pub worm: Option<Worm>,
    rng_state: u32,
    worm_cooldown: u16,
}

impl Game {
    pub const fn new() -> Self {
        Self {
            train: Train {
                column: 7,
                track: Track::Lower,
            },
            tick: 0,
            worm: None,
            rng_state: 0xF377_2026,
            worm_cooldown: 60,
        }
    }

    pub fn input(&mut self, action: Action) {
        match action {
            Action::MoveUp => self.train.track = Track::Upper,
            Action::MoveDown => self.train.track = Track::Lower,

            Action::MoveLeft => {
                self.train.column = self.train.column.saturating_sub(1).max(TRAIN_MIN_COLUMN);
            }

            Action::MoveRight => {
                self.train.column = self.train.column.saturating_add(1).min(TRAIN_MAX_COLUMN);
            }
        }
    }

    pub fn update(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        // Move the worm once every 18 game ticks.
        if let Some(mut worm) = self.worm {
            if self.tick.is_multiple_of(18) {
                worm.head_column += 1;

                // The tail has completely left the screen.
                if worm.head_column - i16::from(worm.length) >= i16::from(WIDTH) {
                    self.worm = None;

                    // Variable delay before the next worm.
                    self.worm_cooldown = 70 + (self.random() % 111) as u16;
                } else {
                    self.worm = Some(worm);
                }
            }

            return;
        }

        // Wait before spawning another worm.
        if self.worm_cooldown > 0 {
            self.worm_cooldown -= 1;
        } else {
            self.spawn_worm();
        }
    }

    pub fn render(&self) -> [u8; 32] {
        let mut frame = [b' '; 32];

        // Draw the worm first.
        if let Some(worm) = self.worm {
            let row = match worm.track {
                Track::Upper => 0,
                Track::Lower => 1,
            };

            for segment in 0..worm.length {
                let column = worm.head_column - i16::from(segment);

                // Clip segments outside the LCD.
                if !(0..i16::from(WIDTH)).contains(&column) {
                    continue;
                }

                let sprite = if segment == 0 {
                    sprites::WORM_HEAD
                } else if segment == worm.length - 1 {
                    sprites::WORM_TAIL
                } else {
                    sprites::WORM_BODY
                };

                let index = row * WIDTH as usize + column as usize;

                frame[index] = sprite;
            }
        }

        // Draw the train last, so it remains visible.
        let train_row = match self.train.track {
            Track::Upper => 0,
            Track::Lower => 1,
        };

        let train_index = train_row * WIDTH as usize + self.train.column as usize;

        frame[train_index] = sprites::TRAIN;

        frame
    }

    fn random(&mut self) -> u32 {
        // Xorshift32: small, deterministic, no_std.
        let mut x = self.rng_state;

        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;

        self.rng_state = x;
        x
    }

    fn spawn_worm(&mut self) {
        let length = 3 + (self.random() % 6) as u8;

        let track = if self.random() & 1 == 0 {
            Track::Upper
        } else {
            Track::Lower
        };

        self.worm = Some(Worm {
            head_column: -1,
            length,
            track,
        });
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Worm {
    pub head_column: i16,
    pub length: u8,
    pub track: Track,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Controls {
    pub up: char,
    pub down: char,
    pub left: char,
    pub right: char,
    pub fire: char,
}

impl Controls {
    pub const fn new(up: char, down: char, left: char, right: char, fire: char) -> Self {
        Self {
            up,
            down,
            left,
            right,
            fire,
        }
    }

    pub const fn action_for(&self, key: char) -> Option<Action> {
        if key == self.up {
            Some(Action::MoveUp)
        } else if key == self.down {
            Some(Action::MoveDown)
        } else if key == self.left {
            Some(Action::MoveLeft)
        } else if key == self.right {
            Some(Action::MoveRight)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_starts_on_lower_track() {
        let game = Game::new();

        assert_eq!(game.train.column, 7);
        assert_eq!(game.train.track, Track::Lower);
        assert_eq!(game.tick, 0);
    }

    #[test]
    fn train_can_switch_tracks() {
        let mut game = Game::new();

        game.input(Action::MoveUp);
        assert_eq!(game.train.track, Track::Upper);

        game.input(Action::MoveDown);
        assert_eq!(game.train.track, Track::Lower);
    }

    #[test]
    fn train_stays_on_selected_track() {
        let mut game = Game::new();

        game.input(Action::MoveUp);
        game.input(Action::MoveUp);

        assert_eq!(game.train.track, Track::Upper);
    }

    #[test]
    fn game_tick_advances() {
        let mut game = Game::new();

        game.update();
        game.update();

        assert_eq!(game.tick, 2);
    }

    #[test]
    fn renders_train_on_correct_track() {
        let mut game = Game::new();

        let frame = game.render();
        assert_eq!(frame[23], sprites::TRAIN);

        game.input(Action::MoveUp);

        let frame = game.render();
        assert_eq!(frame[7], sprites::TRAIN);
        assert_eq!(frame[23], b' ');
    }

    #[test]
    fn worm_spawns_after_cooldown() {
        let mut game = Game::new();

        for _ in 0..61 {
            game.update();
        }

        let worm = game.worm.expect("Worm should spawn");

        assert_eq!(worm.head_column, -1);
        assert!((3..=8).contains(&worm.length));
    }

    #[test]
    fn worm_moves_right() {
        let mut game = Game::new();

        game.worm = Some(Worm {
            head_column: 3,
            length: 5,
            track: Track::Upper,
        });

        for _ in 0..18 {
            game.update();
        }

        assert_eq!(game.worm.unwrap().head_column, 4);
    }

    #[test]
    fn renders_worm_segments() {
        let mut game = Game::new();

        game.worm = Some(Worm {
            head_column: 5,
            length: 4,
            track: Track::Upper,
        });

        let frame = game.render();

        assert_eq!(frame[2], sprites::WORM_TAIL);
        assert_eq!(frame[3], sprites::WORM_BODY);
        assert_eq!(frame[4], sprites::WORM_BODY);
        assert_eq!(frame[5], sprites::WORM_HEAD);
    }

    #[test]
    fn train_respects_horizontal_boundaries() {
        let mut game = Game::new();

        for _ in 0..20 {
            game.input(Action::MoveLeft);
        }

        assert_eq!(game.train.column, TRAIN_MIN_COLUMN);

        for _ in 0..20 {
            game.input(Action::MoveRight);
        }

        assert_eq!(game.train.column, TRAIN_MAX_COLUMN);
    }
}
