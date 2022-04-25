// SPDX-License-Identifier: GPL-3.0-or-later

use crate::consts::io::*;

use super::{Lcd};

use crate::consts::lcd::*;

pub type Color7 = u8;

const GRAY_50PCT: Color7 = 0x3F;
const BLACK: Color7 = 0x00;
const WHITE: Color7 = 0x7F;


pub struct Drawing<'a> {
    lcd: &'a mut Lcd,

    color: Color7,
    color_repeat: u32,

    total_pixel_count: u32,
}

impl<'a> Drawing<'a> {
    pub fn new(lcd: &'a mut Lcd) -> Self {
        lcd.start_drawing_raw();
        Self {
            lcd,
            color: 0,
            color_repeat: 0,
            total_pixel_count: 0,
        }
    }

    pub fn set_all_black(mut self) {
        self.push_pixels(BLACK, 2 * WIDTH as usize);
    }

    pub fn set_all_white(mut self) {
        self.push_pixels(WHITE, HEIGHT as usize * WIDTH as usize);
    }

    pub fn stripes(mut self, n: usize) {
        for i in 0..n {
            let color = if i%2 == 0 { BLACK } else { WHITE };
            self.push_pixels(color, HEIGHT as usize * WIDTH as usize / n);
        }
    }

    pub fn waves(mut self, mult: u64) {
        for row in 0..HEIGHT {
            for col in 0..WIDTH {
                let color = if row % 100 == 0 || col % 100 == 0 {
                    WHITE
                } else {
                    (((mult * (WHITE as u64) * row as u64 * col as u64) / (HEIGHT as u64 * WIDTH as u64)) as u8) & WHITE
                };

                self.push_pixels(color, 1);
            }
        }
    }

    // For some reason, when the pixel position reaches the middle of
    // the screen, repeats are canceled and drawing must start over.
    const WINDOW_SIZE: u32 = (WIDTH/2) as u32;

    pub fn flush_pixels(&mut self) {
        while self.color_repeat > 0 {
            /*
            if self.color == BLACK || self.color == WHITE {
                self.lcd.send_data(self.color | 0x80);
            }
            else {
                self.lcd.send_data(((self.color as u16 * 0x7C)/0x7F) as u8 | 0x80);
            }
            */

                self.lcd.send_data(((self.color as u16 * 0x7C)/0x7F) as u8 | 0x80);

            self.total_pixel_count += 1;
            self.color_repeat -= 1;

            let window_position = self.total_pixel_count % Self::WINDOW_SIZE;
            let mut repeat = self.color_repeat.min(Self::WINDOW_SIZE - window_position);

            self.color_repeat -= repeat;
            self.total_pixel_count += repeat;

            while repeat > 0 {
                let n = repeat.min(0x7D);
                self.lcd.send_data(n as u8);
                repeat -= n;
            }
        }
    }

    pub fn push_pixels(&mut self, color: Color7, repeat: usize) {
        if color == self.color {
            self.color_repeat += repeat as u32;
            return;
        }

        self.flush_pixels();

        debug_assert!(color & 0x80 == 0, "color isn't 7 bit: {:02x}", color);

        self.color = color;
        self.color_repeat = repeat as u32;
    }
}

impl<'a> Drop for Drawing<'a> {
    fn drop(&mut self) {
        self.flush_pixels();
        self.lcd.finish_drawing_raw();
    }
}
