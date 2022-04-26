// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::lcd::Color8;
use super::Lcd;

/// This framebuffer uses 7-bit grascale values
pub type Color7 = u8;

pub struct Framebuffer<'a> {
    lcd: &'a mut Lcd,
    color: Color7,
    color_repeat: u32,
    total_pixel_count: u32,
}

impl<'a> Framebuffer<'a> {
    pub fn new(lcd: &'a mut Lcd) -> Self {
        lcd.start_drawing_raw();
        Self { lcd, color: 0, color_repeat: 0, total_pixel_count: 0 }
    }

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
        const REPEAT_WINDOW_SIZE: u32 = 1920 as u32;

        let encoded_color = ((self.color as u16 * 0x7C)/0x7F) as u8 | 0x80;

        while self.color_repeat > 0 {
            self.lcd.send_data(encoded_color);

            self.total_pixel_count += 1;
            self.color_repeat -= 1;

            let window_position = self.total_pixel_count % REPEAT_WINDOW_SIZE;
            if window_position > 0 {
                let mut repeat = self.color_repeat.min(REPEAT_WINDOW_SIZE - window_position);

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

    pub fn push_pixels(&mut self, color: Color8, repeat: u32) {
        let color: Color7 = color >> 1;

        if color == self.color {
            self.color_repeat += repeat as u32;
            return;
        }

        self.flush_pixels();

        self.color = color;
        self.color_repeat = repeat as u32;
    }
}

impl<'a> Drop for Framebuffer<'a> {
    fn drop(&mut self) {
        self.flush_pixels();
        self.lcd.stop_drawing_raw();
    }
}
