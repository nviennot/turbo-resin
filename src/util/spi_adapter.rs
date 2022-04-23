// SPDX-License-Identifier: GPL-3.0-or-later

use embedded_hal::blocking::spi::Transfer;
use embedded_hal::digital::v2::OutputPin;

use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull, Pin};
use embassy::time::{Duration, Timer};
use embassy_stm32::spi::{Config, Spi, Instance};
use embassy_stm32::time::U32Ext;

pub struct SpiAdapter<'d, T: Instance, Tx, Rx>(Spi<'d, T, Tx, Rx>);

impl<'d, T: Instance, Tx, Rx> SpiAdapter<'d, T, Tx, Rx> {
    pub fn new(spi: Spi<'d, T, Tx, Rx>) -> Self {
        Self(spi)
    }
}

impl<'d, T: Instance, Tx, Rx> Transfer<u8> for SpiAdapter<'d, T, Tx, Rx> {
    type Error = embassy_stm32::spi::Error;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        self.0.blocking_transfer_in_place(words)?;
        Ok(words)
    }
}

pub struct OutputAdapter<'d, T: Pin>(Output<'d, T>);

impl<'d, T: Pin> OutputAdapter<'d, T> {
    pub fn new(output: Output<'d, T>) -> Self {
        Self(output)
    }
}

impl<'d, T: Pin> OutputPin for OutputAdapter<'d, T> {
    type Error = ();

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.0.set_low();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.0.set_high();
        Ok(())
    }
}
