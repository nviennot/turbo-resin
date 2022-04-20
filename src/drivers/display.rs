// SPDX-License-Identifier: GPL-3.0-or-later

use embassy_stm32::gpio::low_level::{AFType, Pin};
use embassy_stm32::gpio::{Output, Level, Speed};

use embassy_stm32::{rcc::low_level::RccPeripheral, pac::fsmc::vals};

use embassy_stm32::peripherals as p;

use crate::consts::display::*;

pub struct Display {
    pub reset: Output<'static, p::PB12>,
    pub backlight: Output<'static, p::PG8>,
}

impl Display {
    // We use Bank1 (0x60000000) to address the display.
    // Bank1 PSRAM 4 selection: HADDR[27:26] = 11
    // The A16 wire is used to select the DATA or CMD register. Its address is
    // 0x00020000 = 1 << (16 + 1) (The +1 is because of the 16 bit addressing
    // mode as opposed to 8 bit).
    const TFT_CMD:  *mut u16 = 0x6c00_0000u32 as *mut u16;
    const TFT_DATA: *mut u16 = 0x6c00_2000u32 as *mut u16;

    /*
    pub const FULL_SCREEN: Rectangle = Rectangle::new(
        Point::new(0,0),
        Size::new(Self::WIDTH as u32, Self::HEIGHT as u32)
    );
    */

    #[inline(never)]
    pub fn new(
        reset: p::PB12,
        backlight: p::PG8,

        output_enable: p::PD4,
        write_enable: p::PD5,
        cs: p::PG12,

        a12: p::PG2,

        d0: p::PD14,
        d1: p::PD15,
        d2: p::PD0,
        d3: p::PD1,
        d4: p::PE7,
        d5: p::PE8,
        d6: p::PE9,
        d7: p::PE10,
        d8: p::PE11,
        d9: p::PE12,
        d10: p::PE13,
        d11: p::PE14,
        d12: p::PE15,
        d13: p::PD8,
        d14: p::PD9,
        d15: p::PD10,

        fsmc: p::FSMC,
    ) -> Self {
        let fsmc = embassy_stm32::pac::FSMC;
        p::FSMC::enable();

        let reset = Output::new(reset, Level::Low, Speed::Medium);
        let backlight = Output::new(backlight, Level::Low, Speed::Medium);

        unsafe {
            // PD4: EXMC_NOE: Output Enable
            output_enable.set_as_af(12, AFType::OutputPushPull);
            // PD5: EXMC_NWE: Write enable
            write_enable.set_as_af(12, AFType::OutputPushPull);
            // PD7: EXMC_NE0: Chip select
            cs.set_as_af(12, AFType::OutputPushPull);
            // A12: Selects the Command or Data register
            a12.set_as_af(12, AFType::OutputPushPull);

            d0.set_as_af(12, AFType::OutputPushPull);
            d1.set_as_af(12, AFType::OutputPushPull);
            d2.set_as_af(12, AFType::OutputPushPull);
            d3.set_as_af(12, AFType::OutputPushPull);
            d4.set_as_af(12, AFType::OutputPushPull);
            d5.set_as_af(12, AFType::OutputPushPull);
            d6.set_as_af(12, AFType::OutputPushPull);
            d7.set_as_af(12, AFType::OutputPushPull);
            d8.set_as_af(12, AFType::OutputPushPull);
            d9.set_as_af(12, AFType::OutputPushPull);
            d10.set_as_af(12, AFType::OutputPushPull);
            d11.set_as_af(12, AFType::OutputPushPull);
            d12.set_as_af(12, AFType::OutputPushPull);
            d13.set_as_af(12, AFType::OutputPushPull);
            d14.set_as_af(12, AFType::OutputPushPull);
            d15.set_as_af(12, AFType::OutputPushPull);
        }

        #[cfg(feature="mono4k")]
        unsafe {
            fsmc.bcr1().write(|w| {
                // Enable Bank
                w.set_mbken(vals::BcrMbken::ENABLED);
                // data width: 16 bits
                w.set_mwid(vals::BcrMwid::BITS16);
                // write: enable
                w.set_wren(vals::BcrWren::ENABLED);
            });

            fsmc.btr1().write(|w| {
                // Access Mode A
                w.set_accmod(vals::BtrAccmod::A);
                // Address setup time: not needed.
                w.set_addset(0);
                // Data setup and hold time.
                // (2+1)/120MHz = 25ns. Should be plenty enough.
                // Typically, 10ns is the minimum.
                w.set_datast(2);
                w.set_datlat(2);
            });
        }

        #[cfg(feature="saturn")]
        unsafe {
            fsmc.bcr4().write(|w| {
                // Enable Bank
                w.set_mbken(vals::BcrMbken::ENABLED);
                // data width: 16 bits
                w.set_mwid(vals::BcrMwid::BITS16);
                // write: enable
                w.set_wren(vals::BcrWren::ENABLED);
            });

            fsmc.btr4().write(|w| {
                w.set_accmod(vals::BtrAccmod::A);
                w.set_addset(10);
                w.set_datast(10);
            });
        }

        Self { reset, backlight }
    }

    pub fn set_backlight(&mut self, value: bool) {
        if value {
            self.backlight.set_high()
        } else {
            self.backlight.set_low()
        }
    }

    pub fn write_cmd(&mut self, v: u16) {
        unsafe { Self::TFT_CMD.write_volatile(v); }
    }

    pub fn write_data(&mut self, v: u16) {
        unsafe { Self::TFT_DATA.write_volatile(v); }
    }

    pub fn read_data(&mut self) -> u16 {
        unsafe { Self::TFT_DATA.read_volatile() }
    }

    pub fn init(&mut self) {
        // This sequence is mostly taken from the original firmware
        self.reset.set_low();
        delay_ms(50);
        self.reset.set_high();
        delay_ms(50);

        // This is just for debugging
        {
            let mut data = [0; 4];
            // ID4
            self.cmd_r(0xd3, &mut data);
            info!("display 0xd3: {:02x?}", data);
            // Not sure what that is
            self.cmd_r(0xa1, &mut data);
            info!("display 0xa1: {:02x?}", data);
            // Read Display Identification Information (04h)
            self.cmd_r(0x04, &mut data);
            info!("display 0x04: {:02x?}", data);
        }

        #[cfg(feature="mono4k")]
        {
            self.cmd(0xCF, &[0x00, 0xC1, 0x30]);
            self.cmd(0xED, &[0x64, 0x03, 0x12, 0x81]);
            self.cmd(0xE8, &[0x85, 0x10, 0x7A]);
            self.cmd(0xCB, &[0x39, 0x2C, 0x00, 0x34, 0x02]);
            self.cmd(0xF7, &[0x20]);
            self.cmd(0xEA, &[0x00,0x00]);
            self.cmd(0xC0, &[0x1B]);
            self.cmd(0xC1, &[0x01]);
            self.cmd(0xC5, &[0x30, 0x30]);
            self.cmd(0xC7, &[0xB7]);
            self.cmd(0x3A, &[0x55]);
            self.cmd(0x36, &[0xA8]);
            self.cmd(0xB1, &[0x00, 0x12]);
            self.cmd(0xB6, &[0x0A, 0xA2]);
            self.cmd(0x44, &[0x02]);
            self.cmd(0xF2, &[0x00]);

            // Gamma settings
            self.cmd(0x26, &[0x01]);
            self.cmd(0xE0, &[15, 42, 40, 8, 14, 8, 84, 169, 67, 10, 15, 0, 0, 0, 0]);
            self.cmd(0xE1, &[0, 21, 23, 7, 17, 6, 43, 86, 60, 5, 16, 15, 63, 63, 15]);
        }

        #[cfg(feature="saturn")]
        {
            self.cmd(0xe0, &[0x00, 0x03, 0x0c, 0x09, 0x17, 0x09, 0x3e, 0x89, 0x49, 0x08, 0x0d, 0x0a, 0x13, 0x15, 0x0f]);
            self.cmd(0xe1, &[0x00, 0x11, 0x15, 0x03, 0x0f, 0x05, 0x2d, 0x34, 0x41, 0x02, 0x0b, 0x0a, 0x33, 0x37, 0x0f]);
            self.cmd(0xc0, &[0x17, 0x15]);
            self.cmd(0xc1, &[0x41]);
            self.cmd(0xc5, &[0x00, 0x12, 0x80]);
            self.cmd(0x3a, &[0x55]);
            self.cmd(0xb0, &[0x00]);
            self.cmd(0xb1, &[0xa0]);
            self.cmd(0xb4, &[0x02]);
            self.cmd(0xe9, &[0x00]);
            self.cmd(0xf7, &[0xa9, 0x51, 0x2c, 0x82]);
            self.cmd(0xb6, &[0x02, 0x02]);
            self.cmd(0x36, &[0xe8]);
        }

        // Sleep Out
        self.cmd(0x11, &[]);

        // Display ON
        self.cmd(0x29, &[]);

        self.fill_screen(0);
    }

    pub fn write_data_as_two_u8(&mut self, v: u16) {
        self.write_data(v >> 8);
        self.write_data(v & 0xFF);
    }

    pub fn cmd(&mut self, cmd: u16, args: &[u16]) {
        self.write_cmd(cmd);
        for a in args {
            self.write_data(*a);
        }
    }

    pub fn cmd_r(&mut self, cmd: u16, data: &mut [u16]) {
        self.write_cmd(cmd);
        for a in data {
            *a = self.read_data();
        }
    }

    pub fn start_drawing(&mut self, top_left: (u16, u16), bottom_right: (u16, u16)) {
        let (left, top) = top_left;
        let (right, bottom) =  bottom_right;

        self.write_cmd(0x2A);
        self.write_data_as_two_u8(left);
        self.write_data_as_two_u8(right - 1);
        self.write_cmd(0x2B);
        self.write_data_as_two_u8(top);
        self.write_data_as_two_u8(bottom - 1);
        self.write_cmd(0x2C);
    }

    pub fn start_drawing_full_screen(&mut self) {
        self.start_drawing((0,0), (WIDTH, HEIGHT));
    }

    pub fn fill_screen(&mut self, color: u16) {
        self.start_drawing_full_screen();
        for _ in 0..WIDTH {
            for _ in 0..HEIGHT {
                self.write_data(color);
            }
        }
    }

    /*
    pub fn draw_background_image(&mut self, ext_flash: &mut ExtFlash, img_index: u8, area: &Rectangle) {
        let area = area.intersection(&self.bounding_box());
        if area.is_zero_sized() {
            return;
        }

        let image_addr = 0x30000 * (img_index as u32);

        let width = area.size.width as u16;
        let left_col = area.top_left.x as u16;
        let right_col = left_col + width;

        const BYTES_PER_PIXEL: u32 = 2;

        let mut buf_ = [0u8; (BYTES_PER_PIXEL as usize)*Self::WIDTH as usize];

        for row in area.rows() {
            let buf = &mut buf_[0..(BYTES_PER_PIXEL as usize)*(width as usize)];
            let start_pixel_index = (row as u32) * (Self::WIDTH as u32) + left_col as u32;
            ext_flash.0.read(image_addr + BYTES_PER_PIXEL*start_pixel_index, buf).unwrap();

            let row = row as u16;
            self.start_drawing((left_col,  row),
                               (right_col, row+1));

            for i in 0..width {
                let i = i as usize;
                self.write_data(((buf[2*i+1] as u16) << 8) | buf[2*i] as u16);
            }
        }
    }
    */
}


// Embedded Graphics integration

use core::convert::TryInto;
use embedded_graphics::{
    prelude::*,
    pixelcolor::{Rgb565, raw::RawU16},
    primitives::Rectangle,
};

use super::delay_ms;

impl DrawTarget for Display {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            const W: i32 = WIDTH as i32;
            const H: i32 = HEIGHT as i32;
            if let Ok((x @ 0..=W, y @ 0..=H)) = coord.try_into() {
                let x = x as u16;
                let y = y as u16;
                self.start_drawing((x,y), (x+1,y+1));
                self.write_data(RawU16::from(color).into_inner());
            }
        }

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        // Clamp area to drawable part of the display target
        let drawable_area = area.intersection(&self.bounding_box());

        // Check that there are visible pixels to be drawn
        if drawable_area.size != Size::zero() {
            let start = drawable_area.top_left;
            let end = drawable_area.bottom_right().unwrap();
            self.start_drawing((start.x as u16, start.y as u16),
                               ((end.x+1) as u16, (end.y+1) as u16));

            area.points()
                .zip(colors)
                .filter(|(pos, _color)| drawable_area.contains(*pos))
                .for_each(|(_, color)| self.write_data(RawU16::from(color).into_inner()));
        }
        Ok(())
    }
}

impl OriginDimensions for Display {
    fn size(&self) -> Size {
        Size::new(WIDTH.into(), HEIGHT.into())
    }
}
