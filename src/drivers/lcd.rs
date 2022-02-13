// SPDX-License-Identifier: GPL-3.0-or-later

use stm32f1xx_hal::{
    pac::SPI1,
    gpio::*,
    gpio::gpioa::*,
    gpio::gpiod::*,
    afio::MAPR,
    rcc::Clocks,
    prelude::*,
    spi::*,
    spi,
};

pub struct Lcd {
    cs: PA4<Output<PushPull>>,
    spi: Spi<
            SPI1,
            Spi1NoRemap,
            (
                PA5<Alternate<PushPull>>,
                PA6<Input<Floating>>,
                PA7<Alternate<PushPull>>
            ),
            u16,
         >,
}

impl Lcd {
    pub fn new(
        reset: PD12<Input<Floating>>,
        cs: PA4<Input<Floating>>,
        sck: PA5<Input<Floating>>,
        miso: PA6<Input<Floating>>,
        mosi: PA7<Input<Floating>>,
        spi1: SPI1,
        clocks: &Clocks,
        gpioa_crl: &mut Cr<CRL, 'A'>,
        gpiod_crh: &mut Cr<CRH, 'D'>,
        mapr: &mut MAPR,
    ) -> Self {
        let _reset = reset.into_push_pull_output_with_state(gpiod_crh, PinState::Low);
        let cs = cs.into_push_pull_output_with_state(gpioa_crl, PinState::High);

        let spi = {
            let sck = sck.into_alternate_push_pull(gpioa_crl);
            let miso = miso.into_floating_input(gpioa_crl);
            let mosi = mosi.into_alternate_push_pull(gpioa_crl);

            Spi::spi1(
                spi1,
                (sck, miso, mosi),
                mapr,
                spi::Mode { polarity: spi::Polarity::IdleLow, phase: spi::Phase::CaptureOnFirstTransition },
                clocks.pclk1()/2, // Run as fast as we can (60Mhz)
                *clocks,
            ).frame_size_16bit()
        };

        Self { cs, spi }
    }

    const COLS: u16 = 3840;
    const ROWS: u16 = 2400;

    /*
    const DEFAULT_COLOR_MAP: [u16; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff
    ];

    pub fn demo(&mut self) {
        crate::debug!("version = 0x{:x?}", self.get_version());

        self.draw_waves(8);

        let mut cmap = Self::DEFAULT_COLOR_MAP;
        loop {
            let first = cmap[0];
            for k in 0..15 {
                cmap[k] = cmap[k+1];
            }
            cmap[15] = first;

            self.set_color_map(&cmap);
            self.delay_150ns(300000);
        }

        self.draw_waves(16);
    }
    */

    pub fn draw_all_black(&mut self) {
        self.draw(|row, col| { 0 })
    }

    pub fn draw_waves(&mut self, mult: u32) {
        self.draw(|row, col| {
            if row % 100 == 0 || col % 100 == 0 {
                0x0F
            } else {
                ((mult*16 * row as u32 * col as u32) / (Self::ROWS as u32 * Self::COLS as u32)) as u8
            }
        })
    }

    pub fn draw(&mut self, f: impl Fn(u16, u16) -> u8) {
        self.cs.set_low();
        self.delay_150ns(10);

        // The FPGA seems a little buggy.
        // It won't take the command well. Apparently, we have to send it twice.
        // Otherwise, 2/3 of a second frame won't render. There's a strange bug.
        self.cmd(Command::StartDrawing, None, None);
        self.delay_150ns(60);
        self.cs.set_high();
        self.delay_150ns(6000); // 1ms delay
        self.cs.set_low();
        self.delay_150ns(10);
        self.cmd(Command::StartDrawing, None, None);

        for row in 0..Self::ROWS {
            for col in 0..Self::COLS/4 {
                let color =
                    (((f(row, 4*col+0)&0x0F) as u16) << 12) |
                    (((f(row, 4*col+1)&0x0F) as u16) <<  8) |
                    (((f(row, 4*col+2)&0x0F) as u16) <<  4) |
                    (((f(row, 4*col+3)&0x0F) as u16) <<  0);
                self.spi.spi_write(&[color]).unwrap();
            }
        }

        self.delay_150ns(60);
        self.cs.set_high();
    }

    pub fn get_version(&mut self) -> u32 {
        let mut rx = [0_u16; 2];
        self.cmd(Command::GetVersion, None, Some(&mut rx));
        let version = (rx[1] as u32) << 16 | rx[0] as u32;
        return version
    }

    pub fn set_color_map(&mut self, map: &[u16; 16]) {
        self.cmd(Command::SetColorMap, Some(map), None);
    }

    pub fn get_color_map(&mut self) -> [u16; 16] {
        let mut map = [0_u16; 16];
        self.cmd(Command::GetColorMap, None, Some(&mut map));
        return map;
    }

    fn cmd(&mut self, cmd: Command, tx: Option<&[u16]>, rx: Option<&mut [u16]>) {
        let toggle_cs = self.cs.is_set_high();

        if toggle_cs {
            self.cs.set_low();
            self.delay_150ns(10);
        }

        self.spi.spi_write(&[cmd as u16, 0]).unwrap();

        if let Some(tx) = tx {
            self.spi.spi_write(tx).unwrap();
        }

        if let Some(rx) = rx {
            self.spi.transfer(rx).unwrap();
        }

        if toggle_cs {
            self.delay_150ns(60);
            self.cs.set_high();
        }
    }

    pub fn delay_150ns(&self, count: u32) {
        cortex_m::asm::delay(20*count);
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
enum Command {
    GetVersion = 0xF0,
    StartDrawing = 0xFB,

    SetColorMap = 0xF1,
    GetColorMap = 0xF2,

    /*
    0xF3, // sends 18 u16, essentially a range(1,16) / (num)
    0xF4, // receives 18 u16
    0xFC, // sends 16 u16, zeroed
    */
}
