// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::lcd::Color8;
use crate::consts::io::*;

// Color is 4 bpp grayscale
pub type Color4 = u8;
pub const WHITE: u8 = 0x0F;
pub const BLACK: u8 = 0x00;

use super::Lcd;

pub struct Framebuffer<'a> {
    lcd: &'a mut Lcd,
    pending_pixels: u16,
    pending_pixels_cnt: u8, // modulo 4
}

impl<'a> Framebuffer<'a> {
    pub fn new(lcd: &'a mut Lcd) -> Self {
        lcd.start_drawing_raw();
        Self { lcd, pending_pixels: 0, pending_pixels_cnt: 0 }
    }

    pub fn push_pixels(&mut self, color: Color8, mut repeat: u32) {
        let color = (color >> 4) as u16;

        if repeat == 0 { return }

        // First, flush any packed pending pixels.
        // Writing the code like this makes it fast. Performance is critical here.
        if self.pending_pixels_cnt == 1 {
            repeat -= 1;
            self.pending_pixels = (self.pending_pixels << 4) | color;
            self.pending_pixels_cnt += 1;
            if repeat == 0 { return }
        }
        if self.pending_pixels_cnt == 2 {
            repeat -= 1;
            self.pending_pixels = (self.pending_pixels << 4) | color;
            self.pending_pixels_cnt += 1;
            if repeat == 0 { return }
        }
        if self.pending_pixels_cnt == 3 {
            repeat -= 1;
            self.lcd.send_data((self.pending_pixels << 4) | color);
            self.pending_pixels_cnt = 0;
            if repeat == 0 { return }
        }

        // 0x000A turns into 0xAAAA
        let packed_pixels = (color << 12) | (color << 8) | (color << 4) | color;

        // Now we flush pixels 4 by 4
        for _ in 0..repeat/4 {
            self.lcd.send_data(packed_pixels);
        }

        // We may have some leftovers, save them for later
        self.pending_pixels = packed_pixels;
        self.pending_pixels_cnt = (repeat % 4) as u8;
    }
}

impl<'a> Drop for Framebuffer<'a> {
    fn drop(&mut self) {
        // If there's pending pixels, oh well.
        if self.pending_pixels_cnt > 0 {
            debug!("WARN: leftover pixels")
        }
        self.lcd.stop_drawing_raw();
    }
}
