use stm32f1xx_hal::{
    gpio::*,
    gpio::gpioa::*,
    gpio::gpioc::*,
    pac::EXTI,
    afio,
};

use crate::drivers::clock::delay_ns;
use core::convert::From;

use crate::consts::touch_screen::*;

// The scale doesn't really matter. It's just to avoid using floats as we are dealing with small values.
const PRESSURE_SCALE: u16 = 32;
const PRESSURE_THRESHOLD_VALUE: u16 = (PRESSURE_SCALE as f32 * PRESSURE_THRESHOLD) as u16;


// There's an application note that can be useful to follow for getting good
// results https://www.ti.com/lit/an/sbaa036/sbaa036.pdf


pub struct TouchScreen {
    device: ADS7846,
    num_samples: u8,
    last_samples: [TouchEvent; NUM_STABLE_SAMPLES as usize],
}

pub enum TouchScreenResult {
    DelayMs(u8),
    Done(Option<TouchEvent>),
}

struct ADS7846 {
    cs: PC7<Output<PushPull>>,
    sck: PC8<Output<PushPull>>,
    miso: PC9<Input<Floating>>,
    mosi: PA8<Output<PushPull>>,
    touch_detected: PA9<Input<Floating>>,
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

impl TouchScreen {
    pub fn new(
        cs: PC7<Input<Floating>>,
        sck: PC8<Input<Floating>>,
        miso: PC9<Input<Floating>>,
        mosi: PA8<Input<Floating>>,
        mut touch_detected: PA9<Input<Floating>>,
        gpioa_crh: &mut Cr<CRH, 'A'>,
        gpioc_crl: &mut Cr<CRL, 'C'>,
        gpioc_crh: &mut Cr<CRH, 'C'>,
        afio: &mut afio::Parts,
        exti: &EXTI,
    ) -> Self {
        let cs = cs.into_push_pull_output_with_state(gpioc_crl, PinState::High);
        let sck = sck.into_push_pull_output(gpioc_crh);
        let miso = miso.into_floating_input(gpioc_crh);
        let mosi = mosi.into_push_pull_output(gpioa_crh);

        touch_detected.make_interrupt_source(afio);
        touch_detected.trigger_on_edge(exti, Edge::Falling);
        touch_detected.enable_interrupt(exti);

        let device = ADS7846 { cs, sck, miso, mosi, touch_detected };

        let num_samples = 0;
        let last_samples = Default::default();

        Self { device, num_samples, last_samples }
    }

    pub fn on_pen_down_interrupt(&mut self) -> TouchScreenResult {
        self.device.touch_detected.clear_interrupt_pending_bit();
        if self.device.is_touch_detected() {
            return TouchScreenResult::DelayMs(DEBOUNCE_INT_DELAY_MS);
        } else {
            return TouchScreenResult::Done(None);
        }
    }

    /// Returns a touchevent, plus an option amount of milliseconds to wait.
    pub fn on_delay_expired(&mut self) -> TouchScreenResult {
        // The touch line should always be active during the entirety of the sampling.
        if !self.device.is_touch_detected() {
            self.num_samples = 0;
            return TouchScreenResult::Done(None);
        }

        let sample = self.device.read_packet().into();
        self.last_samples[(self.num_samples % NUM_STABLE_SAMPLES) as usize] = sample;
        // If we wrap, we will be in the same state as if we just received a pen
        // interrupt. It's fine as it's unusual.
        self.num_samples = self.num_samples.wrapping_add(1);

        if self.num_samples >= NUM_STABLE_SAMPLES {
            if let Some(result) = self.compile_samples() {
                self.num_samples = 0;
                return TouchScreenResult::Done(Some(result));
            }
        }
        return TouchScreenResult::DelayMs(SAMPLE_DELAY_MS);
    }

    /// Returns a sample when the touch events are consistent
    fn compile_samples(&self) -> Option<TouchEvent> {
        let mut avg_sample: TouchEvent = Default::default();
        for sample in &self.last_samples {
            // If the touch pressure is seen to be bad just once, we discard the
            // whole thing.
            if sample.z > PRESSURE_THRESHOLD_VALUE {
                return None;
            }

            avg_sample.x += sample.x;
            avg_sample.y += sample.y;
            avg_sample.z += sample.z;
        }

        avg_sample.x /= self.last_samples.len() as u16;
        avg_sample.y /= self.last_samples.len() as u16;
        avg_sample.z /= self.last_samples.len() as u16;

        for sample in &self.last_samples {
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


impl ADS7846 {
    fn is_touch_detected(&self) -> bool {
        self.touch_detected.is_low()
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
