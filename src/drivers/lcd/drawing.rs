// SPDX-License-Identifier: GPL-3.0-or-later

use crate::consts::io::*;

use super::{Lcd, Color};

pub struct Drawing<'a> {
    lcd: &'a mut Lcd,

    pending_pixels: u16,
    pending_pixels_cnt: u8, // modulo 4
}

impl<'a> Drawing<'a> {
    pub fn new(lcd: &'a mut Lcd) -> Self {
        lcd.start_drawing_raw();
        Self {
            lcd,
            pending_pixels: 0,
            pending_pixels_cnt: 0,
        }
    }

    pub fn set_all_black(mut self) {
        self.push_pixels(0x00, (Lcd::ROWS as usize) * (Lcd::COLS as usize));
    }

    pub fn set_all_white(mut self) {
        self.push_pixels(0x0F, (Lcd::ROWS as usize) * (Lcd::COLS as usize));
    }

    pub fn waves(mut self, mult: u32) {
        for row in 0..Lcd::ROWS {
            for col in 0..Lcd::COLS {
                let color = if row % 100 == 0 || col % 100 == 0 {
                    0x0F
                } else {
                    (((mult*16 * row as u32 * col as u32) / (Lcd::ROWS as u32 * Lcd::COLS as u32)) as u8) & 0x0F
                };

                self.push_pixels(color, 1);
            }
        }
    }

    pub fn push_pixels(&mut self, color: Color, mut repeat: usize) {
        let color = color as u16;
        if repeat == 0 { return }

        // First, flush any packed pending pixels.
        // Writing the code like this makes it fast. Performance is critical here.
        if self.pending_pixels_cnt == 1 {
            repeat -= 1;
            self.pending_pixels = (self.pending_pixels << 4) | color;
            self.pending_pixels_cnt = 2;
            if repeat == 0 { return }
        }
        if self.pending_pixels_cnt == 2 {
            repeat -= 1;
            self.pending_pixels = (self.pending_pixels << 4) | color;
            self.pending_pixels_cnt = 3;
            if repeat == 0 { return }
        }
        if self.pending_pixels_cnt == 3 {
            repeat -= 1;
            self.lcd.draw_raw((self.pending_pixels << 4) | color, 1);
            self.pending_pixels_cnt = 0;
            if repeat == 0 { return }
        }

        // Now we flush pixels 4 by 4
        let packed_pixels = (color << 12) | (color << 8) | (color << 4) | color;
        self.lcd.draw_raw(packed_pixels, repeat/4);
        self.pending_pixels = packed_pixels;
        self.pending_pixels_cnt = (repeat % 4) as u8;
    }
}

impl<'a> Drop for Drawing<'a> {
    fn drop(&mut self) {
        self.lcd.stop_drawing_raw();
    }
}
