// SPDX-License-Identifier: GPL-3.0-or-later

use embassy_stm32::exti::ExtiInput;
use embassy_stm32::pac::SPI1;
use embassy_stm32::peripherals as p;
use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull};
use embassy_stm32::rcc::Clocks;
use embassy_stm32::spi::{Config, Spi};
use embassy::time::{Duration, Timer};
use embassy_stm32::rcc::low_level::RccPeripheral;

use crate::drivers::delay_us;

use super::Framebuffer;

pub struct Lcd {
    cs: Output<'static, p::PA4>,
    spi: Spi<'static, p::SPI1, p::DMA1_CH3, p::DMA1_CH2>,
}

impl Lcd {
    pub fn new(
        reset: p::PD12,
        cs: p::PA4,
        sck: p::PA5,
        miso: p::PA6,
        mosi: p::PA7,
        spi1: p::SPI1,
        dma_rx: p::DMA1_CH2,
        dma_tx: p::DMA1_CH3,
    ) -> Self {
        // forget to avoid the pin to get back in the input state.
        core::mem::forget(Output::new(reset, Level::Low, Speed::Low));

        let cs = Output::new(cs, Level::High, Speed::Medium);

        let cfg = Config::default();
        let spi = Spi::new(spi1, sck, mosi, miso, dma_tx, dma_rx, p::SPI1::frequency(), cfg);

        Self { cs, spi }
    }

    pub const COLS: u16 = 3840;
    pub const ROWS: u16 = 2400;

    pub const DEFAULT_PALETTE: [u16; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff
    ];

    pub fn draw(&mut self) -> Framebuffer {
        Framebuffer::new(self)
    }

    pub fn start_drawing_raw(&mut self) {
        self.cs.set_low();
        delay_us(1);

        // The FPGA seems a little buggy.
        // It won't take the command well. Apparently, we have to send it twice.
        // Otherwise, 2/3 of a second frame won't render. There's a strange bug.
        self.cmd(Command::StartDrawing, None, None);
        delay_us(10);
        self.cs.set_high();
        delay_us(10);
        self.cs.set_low();
        delay_us(10);
        self.cmd(Command::StartDrawing, None, None);
    }

    #[inline(always)]
    pub fn send_data(&mut self, data: u16) {
        // We use this small piece of code, it's much faster than the SPI API.
        // Note that the SPI is correctly configured (16bits frames) due to the
        // previous commands.
        unsafe {
            while !SPI1.sr().read().txe() {}
            SPI1.dr().write(|w| w.set_dr(data));
        }
    }

    pub fn stop_drawing_raw(&mut self) {
        unsafe {
            while SPI1.sr().read().bsy() {}
        }
        self.cs.set_high();
    }

    pub fn get_version(&mut self) -> u32 {
        let mut rx = [0_u16; 2];
        self.cmd(Command::GetVersion, None, Some(&mut rx));
        let version = (rx[1] as u32) << 16 | rx[0] as u32;
        return version
    }

    pub fn set_palette(&mut self, map: &[u16; 16]) {
        self.cmd(Command::SetPalette, Some(map), None);
    }

    pub fn get_palette(&mut self) -> [u16; 16] {
        let mut map = [0_u16; 16];
        self.cmd(Command::GetPalette, None, Some(&mut map));
        return map;
    }

    fn cmd(&mut self, cmd: Command, tx: Option<&[u16]>, rx: Option<&mut [u16]>) {
        let toggle_cs = self.cs.is_set_high();

        if toggle_cs {
            self.cs.set_low();
            delay_us(1);
        }

        self.spi.blocking_write(&[cmd as u16, 0]).unwrap();

        if let Some(tx) = tx {
            self.spi.blocking_write(tx).unwrap();
        }

        if let Some(rx) = rx {
            self.spi.blocking_transfer_in_place(rx).unwrap();
        }

        if toggle_cs {
            delay_us(1);
            self.cs.set_high();
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
enum Command {
    GetVersion = 0xF0,
    StartDrawing = 0xFB,

    SetPalette = 0xF1,
    GetPalette = 0xF2,

    /*
    0xF3, // sends 18 u16, essentially a range(1,16) / (num)
    0xF4, // receives 18 u16
    0xFC, // sends 16 u16, zeroed
    */
}
