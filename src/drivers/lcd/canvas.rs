// SPDX-License-Identifier: GPL-3.0-or-later

use crate::consts::io::*;

use crate::consts::lcd::*;
use super::Framebuffer;

/// Color8 represents a regular 8bpp grayscale value
pub type Color8 = u8;
const WHITE: u8 = 0xFF;
const BLACK: u8 = 0x00;

const HEIGHT_U64: u64 = HEIGHT as u64;
const WIDTH_U64: u64 = WIDTH as u64;
const WHITE_U64: u64 = WHITE as u64;

pub struct Canvas<'a> {
    fb: Framebuffer<'a>,
    color: Color8,
    color_repeat: u32,
    total_pixel_count: u32,
}

impl<'a> Canvas<'a> {
    pub fn new(fb: Framebuffer<'a>) -> Self {
        Self { fb, color: 0, color_repeat: 0, total_pixel_count: 0 }
    }

    pub fn set_all_black(mut self) {
        self.push_pixels(BLACK, HEIGHT * WIDTH);
    }

    pub fn set_all_white(mut self) {
        self.push_pixels(WHITE, HEIGHT * WIDTH);
    }

    pub fn stripes(mut self, n: u32) {
        for i in 0..n {
            let color = if i%2 == 0 { BLACK } else { WHITE };
            self.push_pixels(color, (HEIGHT * WIDTH) / n);
        }
    }

    pub fn gradient(mut self) {
        for y in 0..HEIGHT {
            let color = ((WHITE as u32) * y) / HEIGHT;
            self.push_pixels(color as Color8, WIDTH);
        }
    }

    pub fn checker(mut self, n: u32) {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let color = if (x/n)%2 ^ (y/n)%2 == 0 { WHITE } else { BLACK };
                self.push_pixels(color, 1);
            }
        }
    }

    pub fn waves(mut self, n: u64, grid: u16) {
        for row in 0..HEIGHT_U64 {
            for col in 0..WIDTH_U64 {
                let color = if row as u16 % grid == 0 || col as u16  % grid == 0 {
                    WHITE
                } else {
                    ((n*WHITE_U64*row*col) / (HEIGHT_U64*WIDTH_U64)) as Color8
                };

                self.push_pixels(color, 1);
            }
        }
    }

    #[inline]
    pub fn push_pixels(&mut self, color: Color8, repeat: u32) {
        self.fb.push_pixels(color, repeat)
    }
}
