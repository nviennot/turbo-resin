// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem::MaybeUninit;

use embassy_stm32::peripherals as p;
use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull};
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use spi_memory::prelude::*;

type Flash = spi_memory::series25::Flash<
    SpiAdapter<'static, p::SPI3, p::DMA1_CH5, p::DMA1_CH2>,
    OutputAdapter<'static, p::PG15>
>;

pub type Error = spi_memory::Error<
    SpiAdapter<'static, p::SPI3, p::DMA1_CH5, p::DMA1_CH2>,
    OutputAdapter<'static, p::PG15>
>;

use crate::util::{SpiAdapter, OutputAdapter};

use crate::consts::ext_flash::*;

pub struct ExtFlash(pub Flash);
use embassy_stm32::rcc::low_level::RccPeripheral;

impl ExtFlash {
    pub fn new(
        cs: p::PG15,
        sck: p::PB3,
        miso: p::PB4,
        mosi: p::PB5,
        spi: p::SPI3,
        dma_rx: p::DMA1_CH2,
        dma_tx: p::DMA1_CH5,
    ) -> Result<Self, Error> {
        let cs = Output::new(cs, Level::High, Speed::Medium);
        let cfg = Config::default();
        debug!("spi3 f={}", p::SPI3::frequency().0);
        let spi = Spi::new(spi, sck, mosi, miso, dma_tx, dma_rx, Hertz::hz(SPI_FREQ_HZ), cfg);

        let spi = SpiAdapter::new(spi);
        let cs = OutputAdapter::new(cs);

        let flash = Flash::init(spi, cs)?;

        Ok(Self(flash))
    }

    /*
    // XXX Make sure to use the BlockIfFull setting in logging.rs
    pub fn dump(&mut self) -> Result<(), Error> {
        const EXT_FLASH_SIZE: usize = 4*1024*1024;

        const BUFFER_SIZE: usize = 32*1024; // 32KB
        let mut buf = [0; BUFFER_SIZE];

        for addr in (0..EXT_FLASH_SIZE).step_by(BUFFER_SIZE) {
            self.0.read(addr as u32, &mut buf).expect("Failed to read flash");
            for i in (0..BUFFER_SIZE).step_by(16) {
                debug!("{:08x} {:02x?}", addr+i, &buf[i..i+16]);
            }
        }

        debug!("OK");

        Ok(())
    }
    */

    pub fn read_obj<O>(&mut self, addr: u32) -> Result<O, Error> {
        let mut obj = MaybeUninit::<O>::uninit();
        let buf = unsafe { core::slice::from_raw_parts_mut(
            obj.as_mut_ptr() as *mut _,
            obj.as_bytes().len(),
        )};

        self.0.read(addr, buf)?;
        unsafe { Ok(obj.assume_init()) }
    }
}
