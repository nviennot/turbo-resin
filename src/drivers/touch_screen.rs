// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::delay_ns;
use core::convert::From;
use crate::consts::touch_screen::*;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::peripherals as p;
use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull};
use embassy::time::{Duration, Timer};
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::U32Ext;

use crate::consts::display::*;


// The scale doesn't really matter. It's just to avoid using floats as we are dealing with small values.
const PRESSURE_SCALE: u16 = 32;
const PRESSURE_THRESHOLD_VALUE: u16 = (PRESSURE_SCALE as f32 * PRESSURE_THRESHOLD) as u16;

// There's an application note that can be useful to follow for getting good
// results https://www.ti.com/lit/an/sbaa036/sbaa036.pdf

/// Raw data coming out of the device
#[derive(Default, Debug)]
struct Packet {
    x: u16,
    y: u16,
    z1: u8,
    z2: u8,
}

#[derive(Default, Debug, Clone, Copy)]
/// Processed packet.
pub struct TouchEvent {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

pub struct TouchScreen {
    device: ADS7846,
    had_touch_event: bool,
}

impl TouchScreen {
    pub fn new(device: ADS7846) -> Self {
        Self { device, had_touch_event: false }
    }

    pub async fn get_next_touch_event(&mut self) -> Option<TouchEvent> {
        loop {
            let touch_event = self.get_stable_sample().await;

            if touch_event.is_some() {
                self.had_touch_event = true;
                return touch_event;
            }

            if self.had_touch_event {
                self.had_touch_event = false;
                return None;
            }

            Timer::after(Duration::from_millis(SLEEP_DELAY_MS)).await;
        }
    }

    async fn get_stable_sample(&mut self) -> Option<TouchEvent> {
        let mut num_samples: u8 = 0;
        let mut last_samples: [TouchEvent; NUM_STABLE_SAMPLES as usize] = Default::default();

        loop {
            // If we get a single bad packet, we bail.
            let sample = self.device.read_packet().try_into().ok()?;
            last_samples[(num_samples % NUM_STABLE_SAMPLES) as usize] = sample;

            // If we wrap, we will be in the same state as if we just received a pen
            // interrupt. It's fine as it's unusual, and we'd rather keep the
            // num_samples as a u8. We don't want to do saturating_add() because
            // that would no longer distribute values in the last_samples array.
            num_samples = num_samples.wrapping_add(1);

            if num_samples >= NUM_STABLE_SAMPLES {
                if let Some(result) = Self::compile_stable_sample(&last_samples) {
                    return Some(result)
                }
            }

            Timer::after(Duration::from_millis(SAMPLE_DELAY_MS)).await;
        }
    }

    /// Returns a sample when the touch events are consistent
    fn compile_stable_sample(last_samples: &[TouchEvent]) -> Option<TouchEvent> {
        let mut avg_sample: TouchEvent = Default::default();
        for sample in last_samples {
            // If the touch pressure is seen to be bad just once, we discard the
            // whole thing.
            if sample.z > PRESSURE_THRESHOLD_VALUE {
                return None;
            }

            avg_sample.x += sample.x;
            avg_sample.y += sample.y;
            avg_sample.z += sample.z;
        }

        avg_sample.x /= last_samples.len() as u16;
        avg_sample.y /= last_samples.len() as u16;
        avg_sample.z /= last_samples.len() as u16;

        for sample in last_samples {
            if avg_sample.x.abs_diff(sample.x) > STABLE_X_Y_VALUE_TOLERANCE ||
               avg_sample.y.abs_diff(sample.y) > STABLE_X_Y_VALUE_TOLERANCE {
                   return None;
           }
        }

        Some(avg_sample)
    }
}

impl TryFrom<Packet> for TouchEvent {
    type Error = ();

    fn try_from(p: Packet) -> Result<Self, Self::Error> {
        const MAX: u16 = 1 << 12;
        let (mut x, mut y) = (MAX-p.y,p.x);

        #[cfg(feature="saturn")]
        {
            #[inline]
            fn scale(v: u16, old_min: u16, old_max: u16, new_min: u16, new_max: u16) -> Result<u16, ()> {
                let (v, old_min, old_max, new_min, new_max) =
                    (v as i32, old_min as i32, old_max as i32, new_min as i32, new_max as i32);

                if (old_min..old_max).contains(&v) {
                    let v = (v - old_min) * (new_max - new_min) / (old_max - old_min) + new_min;
                    Ok(v as u16)
                } else {
                    Err(())
                }
            }

            x = scale(x, TOP_LEFT.0, BOTTOM_RIGHT.0, 0, WIDTH-1)?;
            y = scale(y, TOP_LEFT.1, BOTTOM_RIGHT.1, 0, HEIGHT-1)?;
        }

        #[cfg(feature="mono4k")]
        {
            x = (x/11).saturating_sub(36);
            y = (y/15).saturating_sub(15);
        }

        let z = if p.z1 > 1 {
            // Equation (2) in the manual
            ((p.z2 as u32) * (p.x as u32) /
             (p.z1 as u32 * (MAX as u32 / PRESSURE_SCALE as u32))) as u16
        } else {
            return Err(());
        };

        Ok(Self { x, y, z })
    }
}

pub fn into_lvgl_event(e: &Option<TouchEvent>) -> lvgl::core::TouchPad {
    use lvgl::core::TouchPad;
    if let Some(e) = e.as_ref() {
        TouchPad::Pressed { x: e.x as i16, y: e.y as i16 }
    } else {
        TouchPad::Released
    }
}


pub struct ADS7846 {
    cs: Output<'static, p::PD11>,
    spi: Spi<'static, p::SPI2, p::DMA1_CH4, p::DMA1_CH3>,
}

impl ADS7846 {
    pub fn new(
        cs: p::PD11,
        sck: p::PB13,
        miso: p::PB14,
        mosi: p::PB15,
        spi: p::SPI2,
        dma_rx: p::DMA1_CH3,
        dma_tx: p::DMA1_CH4,
    ) -> Self {
        let cs = Output::new(cs, Level::High, Speed::Medium);
        let cfg = Config::default();
        let spi = Spi::new(spi, sck, mosi, miso, dma_tx, dma_rx, SPI_FREQ_HZ.hz(), cfg);

        Self { cs, spi }
    }

    // Returns (x,y) coordinates if a touch is detected
    fn read_packet(&mut self) -> Packet {
        self.cs.set_low();

        // 1            101           0               0             11
        // Start bit    Measure X     Mode 12-bits    differential  Power always on
        let x = self.cmd_u12(0b11010011);

        // 1            001           0               0             11
        // Start bit    Measure Y     Mode 12-bits    differential  Power always on
        let y = self.cmd_u12(0b10010011);

        // 1            011           1               0             11
        // Start bit    Measure Z1    Mode 8-bits     differential  Power always on
        let z1 = self.cmd_u8(0b10111011);

        // 1            100           1               0             11
        // Start bit    Measure Z2    Mode 8-bits     differential  Power always on
        let z2 = self.cmd_u8(0b11001000);

        self.cs.set_high();

        Packet { x, y, z1, z2 }
    }

    fn cmd_u12(&mut self, cmd: u8) -> u16 {
        self.exchange_data(cmd);
        let high_bits = self.exchange_data(0) as u16;
        let low_bits = self.exchange_data(0) as u16;
        let result = (high_bits << 8) | (low_bits);
        result >> 4
    }

    fn cmd_u8(&mut self, cmd: u8) -> u8 {
        self.exchange_data(cmd);
        self.exchange_data(0)
    }

    fn exchange_data(&mut self, tx: u8) -> u8 {
        let mut read = [0];
        let _ = self.spi.blocking_transfer(&mut read, &[tx]);
        read[0]
    }
}
