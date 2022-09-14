// SPDX-License-Identifier: GPL-3.0-or-later

use embassy_stm32::peripherals as p;
use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull};
use crate::consts::lcd::*;
use crate::util::bitbang_spi::Spi;
use super::Framebuffer;
use super::super::Canvas;

const CMD_PREFIX: u8 = 0xfe;
const REPLY_HEADER: u16 = 0xfbfd;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum Command {
    MaskDisplay = 0x00,
    Unknown03 = 0x03,
    GetResolution = 0x04,
    UnmaskDisplay = 0x08,
    Unknown10 = 0x10,
    Unknown20 = 0x20,
    StartDrawing = 0xfd,
}

pub struct Lcd {
    cs: Output<'static, p::PA15>,
    spi: Spi<p::PC7, p::PG3, p::PC6, SPI_FREQ_HZ>,
}

impl Lcd {
    pub fn new(
        cs: p::PA15,
        clk: p::PC7,
        miso: p::PC6,
        mosi: p::PG3,
    ) -> Self {
        let cs = Output::new(cs, Level::High, Speed::Medium);
        let clk = Output::new(clk, Level::Low, Speed::Medium);
        let mosi = Output::new(mosi, Level::Low, Speed::Medium);
        let miso = Input::new(miso, Pull::None);
        let spi = Spi::new(clk, mosi, miso);
        Self { cs, spi }
    }

    pub fn init(&mut self) {
        self.cmd(Command::MaskDisplay);
        // Not sure what that command does. But the original firmware does it.
        self.cmd(Command::Unknown03);

        if let Ok((w,h)) = self.get_resolution() {
            debug!("LCD resolution is {}x{}", w, h);
        } else {
            debug!("Failed to get LCD resolution");
        }
    }

    pub fn draw(&mut self) -> Canvas {
        Canvas::new(Framebuffer::new(self))
    }

    pub fn start_drawing_raw(&mut self) {
        self.cmd(Command::MaskDisplay);
        // These two unknown commands are done in the original firmware.
        // Taking it away doesn't seem to break anything, but we'll leave it there.
        self.cmd(Command::Unknown10);
        self.cmd(Command::Unknown20);
        self.cmd(Command::StartDrawing);
        self.cs.set_low();
    }

    #[inline]
    pub fn send_data(&mut self, data: u8) {
        self.spi.xfer(data);
    }

    pub fn stop_drawing_raw(&mut self) {
        self.cs.set_high();
        self.cmd(Command::UnmaskDisplay);
    }

    pub fn get_resolution(&mut self) -> Result<(u16, u16), ()> {
        self.cs.set_low();
        self.cmd(Command::GetResolution);

        self.wait_for_reply()?;
        let width = self.spi.xfer(0u16).to_be();
        let height = self.spi.xfer(0u16).to_be();
        self.cs.set_high();

        Ok((width, height))
    }

    fn wait_for_reply(&mut self) -> Result<(), ()> {
        // 3 is abitrary. In the original firmware, they look for the reply
        // header within ~11 bytes. It's a bit silly though, the FPGA
        // should reply deterministically.
        for _ in 0..3 {
            if self.spi.xfer(0u16) == REPLY_HEADER {
                return Ok(())
            }
        }
        return Err(());
    }

    fn cmd(&mut self, cmd: Command) {
        let toggle_cs = self.cs.is_set_high();
        if toggle_cs { self.cs.set_low(); }

        self.spi.xfer(((CMD_PREFIX as u16) << 8) | cmd as u16);

        if toggle_cs { self.cs.set_high(); }
    }
}
