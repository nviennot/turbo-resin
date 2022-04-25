// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem::{MaybeUninit, size_of};

use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull, Pin};
use embassy_stm32::peripherals as p;

use crate::drivers::delay_ms;
use crate::drivers::ext_flash::{ExtFlash, Error};
use crate::util::bitbang_spi::Spi;
use spi_memory::prelude::*;

use crate::consts::lcd::*;

pub struct LcdFpga {
    clk: Output<'static, p::PF9>,
    mosi: Output<'static, p::PF8>,
    reset: Output<'static, p::PG4>,

    ready1: Input<'static, p::PE2>,
    ready2: Input<'static, p::PE5>,
}

impl LcdFpga {
    pub fn new(
        clk: p::PF9,
        mosi: p::PF8,
        reset: p::PG4,

        ready1: p::PE2,
        ready2: p::PE5,
    ) -> Self {
        let clk = Output::new(clk, Level::Low, Speed::Medium);
        let mosi = Output::new(mosi, Level::Low, Speed::Medium);
        let reset = Output::new(reset, Level::Low, Speed::Medium);

        let ready1 = Input::new(ready1, Pull::Down);
        let ready2 = Input::new(ready2, Pull::Down);

        Self { clk, mosi, reset, ready1, ready2 }
    }

    fn wait_ready<P: Pin>(pin: &Input<'static, P>) -> Result<(), ()> {
        for _ in 0..100 {
            if pin.is_high() {
              return Ok(());
            }
            delay_ms(1);
        }
        return Err(());
    }

    pub fn upload_bitstream(mut self, ext_flash: &mut ExtFlash) {
        delay_ms(10);
        self.reset.set_high();
        Self::wait_ready(&self.ready1).expect("FPGA is not detected");

        // We give self.ready1 as the miso pin (even though it's semantically
        // incorrect) to avoid making a Spi implementation that doesn't have a
        // miso pin (no rx).
        let mut spi = Spi::<_,_,_,SPI_FREQ_HZ>::new(self.clk, self.mosi, self.ready1);

        let bitstream = BitstreamMetadata::from_flash(ext_flash);
        debug!("Uploading bitstream. size={}", bitstream.size);

        let start = bitstream.offset;
        let end = bitstream.offset + bitstream.size;

        const BUFFER_SIZE: usize = 1024;
        let mut buf = [0; BUFFER_SIZE];

        for pos in (start..end).step_by(BUFFER_SIZE) {
            let chunk_size = BUFFER_SIZE.min((end-pos) as usize);
            let chunk = &mut buf[0..chunk_size];
            ext_flash.0.read(pos as u32, chunk)
                .expect("Failed to read flash");

            spi.send_bytes(chunk);
        }

        Self::wait_ready(&self.ready2).expect("FPGA is not booting");
        debug!("FPGA is ready");
    }
}

struct BitstreamHeader {
    magic: u32,
    size: u32,
}

struct BitstreamMetadata {
    offset: u32,
    size: u32,
}

impl BitstreamMetadata {
    fn from_flash(ext_flash: &mut ExtFlash) -> Self {
        let header: BitstreamHeader = ext_flash.read_obj(BITSTREAM_HEADER_OFFSET)
            .expect("Failed to read from ext-flash");
        assert!(header.magic == BITSTREAM_MAGIC, "Bitstream header magic invalid");

        Self {
            offset: BITSTREAM_HEADER_OFFSET + size_of::<BitstreamHeader>() as u32,
            size: header.size,
        }
    }
}
