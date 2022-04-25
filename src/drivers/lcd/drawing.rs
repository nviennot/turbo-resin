// SPDX-License-Identifier: GPL-3.0-or-later

use crate::consts::io::*;

use super::{Lcd};

use crate::consts::lcd::*;

pub type Color7 = u8;
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
        self.push_pixels(BLACK, HEIGHT as usize * WIDTH as usize);
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

    pub fn gradient(mut self) {
        for y in 0..HEIGHT {
            let color = ((WHITE as u32) * (y as u32)) / HEIGHT as u32;
            self.push_pixels(color as u8, WIDTH as usize);
        }
    }

    pub fn checker(mut self, n: u16) {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let color = if (x/n)%2 ^ (y/n)%2 == 0 { WHITE } else { BLACK };
                self.push_pixels(color, 1);
            }
        }
    }

    pub fn waves(mut self, n: u64, grid: u64) {
        const HEIGHT_U64: u64 = HEIGHT as u64;
        const WIDTH_U64: u64 = WIDTH as u64;
        const WHITE_U64: u64 = WHITE as u64;
        for row in 0..HEIGHT_U64 {
            for col in 0..WIDTH_U64 {
                let color = if row % grid == 0 || col % grid == 0 {
                    WHITE
                } else {
                    ((n*WHITE_U64*row*col) / (HEIGHT_U64*WIDTH_U64)) as u8 % WHITE
                };

                self.push_pixels(color, 1);
            }
        }
    }

    const REPEAT_WINDOW_SIZE: u32 = 1920 as u32;

    fn flush_pixels(&mut self) {
        // Data flows bytes per byte. The meaning of a byte is the following:
        // - if its 0x80 bit is set, then it means, draw a pixel of shade
        //   corresponding to the remaining 7 bits.
        // - otherwise, the byte represents an integer n for which the display
        //   should repeat the previously drawn color pixel draw n times.
        //   However, they repeat should never cross the column boundary of 0
        //   and 1920 pixels. This seems to suggest that the FPGA has two 1080p
        //   framebuffers stiched together.

        // Note that 0xFD..=0xFF are forbidden colors as these values are used
        // to send commands (like 0xFE that we use). The original firmware
        // transforms colors with a scaling of 0x7C/0x7F to make up for the
        // missing 3 color shades.

        // Also another interesting note, the framebuffer can only receive up to
        // ~2.8MB of data. Pushing more than that and the display starts to look
        // all glitchy. That means that the display cannot display arbitrary
        // images, and will only tolerate highly compressible images
        // (fortunately, 3d printing material is).

        let encoded_color = ((self.color as u16 * 0x7C)/0x7F) as u8 | 0x80;

        while self.color_repeat > 0 {
            self.lcd.send_data(encoded_color);

            self.total_pixel_count += 1;
            self.color_repeat -= 1;

            let window_position = self.total_pixel_count % Self::REPEAT_WINDOW_SIZE;
            if window_position > 0 {
                let mut repeat = self.color_repeat.min(Self::REPEAT_WINDOW_SIZE - window_position);

                self.color_repeat -= repeat;
                self.total_pixel_count += repeat;

                while repeat > 0 {
                    // The value 0x7E is also forbidden as it seems to indicate
                    // commands as well. 0x7F seems to work, but the original
                    // firmware doesn't use it.
                    let n = repeat.min(0x7d);
                    self.lcd.send_data(n as u8);
                    repeat -= n;
                }
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
