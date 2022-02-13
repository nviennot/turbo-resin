// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::clock::delay_ns;
use core::convert::From;
use crate::consts::touch_screen::*;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::peripherals as p;
use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull};
use embassy::time::{Duration, Timer};


// The scale doesn't really matter. It's just to avoid using floats as we are dealing with small values.
const PRESSURE_SCALE: u16 = 32;
const PRESSURE_THRESHOLD_VALUE: u16 = (PRESSURE_SCALE as f32 * PRESSURE_THRESHOLD) as u16;

// There's an application note that can be useful to follow for getting good
// results https://www.ti.com/lit/an/sbaa036/sbaa036.pdf

pub struct ADS7846 {
    cs: Output<'static, p::PC7>,
    sck: Output<'static, p::PC8>,
    miso: Input<'static, p::PC9>,
    mosi: Output<'static, p::PA8>,
    touch_detected: ExtiInput<'static, p::PA9>,
}

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
            {
                // We do the had_touch_event check after we register the wait
                // for touch future. This is so we avoid a race when checking
                // for the touch detection to return the None event, and
                // blocking in wait_for_touch_detected.
                let touch_detected_fut = self.device.wait_for_touch_detected();
                futures::pin_mut!(touch_detected_fut);
                if futures::poll!(&mut touch_detected_fut).is_pending() {
                    if self.had_touch_event {
                        self.had_touch_event = false;
                        return None
                    }
                    touch_detected_fut.await;
                }
            }

            if let Some(touch_event) = self.get_stable_sample().await {
                self.had_touch_event = true;
                return Some(touch_event);
            }
        }
    }

    async fn get_stable_sample(&mut self) -> Option<TouchEvent> {
        let mut num_samples: u8 = 0;
        let mut last_samples: [TouchEvent; NUM_STABLE_SAMPLES as usize] = Default::default();

        loop {
            // The touch line should be active during the entirety of the sampling.
            if !self.device.is_touch_detected() {
                return None;
            }

            let sample = self.device.read_packet().into();
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

impl From<Packet> for TouchEvent {
    fn from(p: Packet) -> Self {
        const MAX_X: u16 = 1 << 12;

        let (x,y) = (p.y,p.x);
        let x = MAX_X - x;
        let x = (x/11).saturating_sub(36);
        let y = (y/15).saturating_sub(15);

        let z = if p.z1 > 1 {
            // Equation (2) in the manual
            ((p.z2 as u32) * (p.x as u32) /
             (p.z1 as u32 * (MAX_X as u32 / PRESSURE_SCALE as u32))) as u16
        } else {
            100
        };

        Self { x, y, z }
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


impl ADS7846 {
    pub fn new(
        cs: p::PC7,
        sck: p::PC8,
        miso: p::PC9,
        mosi: p::PA8,
        touch_detected: p::PA9,
        exti9: p::EXTI9,
    ) -> Self {
        let cs = Output::new(cs, Level::High, Speed::Medium);
        let sck = Output::new(sck, Level::Low, Speed::Medium);
        let miso = Input::new(miso, Pull::None);
        let mosi = Output::new(mosi, Level::Low, Speed::Medium);
        let touch_detected = ExtiInput::new(Input::new(touch_detected, Pull::None), exti9);

        Self { cs, sck, miso, mosi, touch_detected }
    }

    fn is_touch_detected(&self) -> bool {
        self.touch_detected.is_low()
    }

    async fn wait_for_touch_detected(&mut self) {
        self.touch_detected.wait_for_low().await
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

    fn exchange_data(&mut self, mut tx: u8) -> u8 {
        // Timings are specified in Table VI. from the ADS7846 datasheet
        let mut rx: u8 = 0;

        if tx & 0x80 != 0 {
            self.mosi.set_high();
        } else {
            self.mosi.set_low();
        }
        delay_ns(200);

        for _ in 0..8 {
            // mosi is captured by the ADS7846 device on this clock rise
            // it should be stable for at least 10ns before a clock rise. (2 instructions)
            self.sck.set_high();
            delay_ns(200);
            // miso is set after the clock falling by the device
            self.sck.set_low();

            tx <<= 1;
            if tx & 0x80 != 0 {
                self.mosi.set_high();
            } else {
                self.mosi.set_low();
            }
            delay_ns(200);

            rx <<= 1;
            if self.miso.is_high() {
                rx |= 1;
            }
        }

        rx
    }
}
