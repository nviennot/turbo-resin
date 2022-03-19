// SPDX-License-Identifier: GPL-3.0-or-later

use embassy_stm32::pwm::{simple_pwm::SimplePwm, Channel};

use embassy_stm32::gpio::{Dynamic, Output, Level, Speed, Pull};
use embassy_stm32::{rcc::low_level::RccPeripheral, pac::fsmc::vals};
use embassy_stm32::time::U32Ext;

use embassy_stm32::peripherals as p;

use crate::consts::zaxis::hardware::*;

#[derive(PartialEq, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
}

pub struct Drv8424 {
    step: Output<'static, p::PE5>,
    dir: Output<'static, p::PE4>,
    enable: Output<'static, p::PE6>,
    mode0: Dynamic<'static, p::PC3>,
    mode1: Dynamic<'static, p::PC0>,
    pub step_multiplier: u32,
}

impl Drv8424 {
    #[inline(never)]
    pub fn new(
        dir: p::PE4,
        step: p::PE5,
        enable: p::PE6,

        mode0: p::PC3,
        mode1: p::PC0,

        decay0: p::PC1,
        decay1: p::PC2,

        vref: p::PA3,
        pwm_timer: p::TIM2, // // Or TIM5 in alternate mode.
    ) -> Self
    {
        // Pins that are related, but usage not known:
        // PC13 output (1)
        // PA2 output

        let dir = Output::new(dir, Level::Low, Speed::Medium);
        let step = Output::new(step, Level::Low, Speed::Medium);
        let enable = Output::new(enable, Level::Low, Speed::Medium);

        let mode0 = Dynamic::new(mode0);
        let mode1 = Dynamic::new(mode1);


        // Decay0 | Decay1 | Increasing Steps          | Decreasing Steps
        // -------|--------|---------------------------|----------------------------
        //  0     |   0    | Smart tune Dynamic Decay  | Smart tune Dynamic Decay
        //  0     |   1    | Smart tune Ripple Control | Smart tune Ripple Control
        //  1     |   0    | Mixed decay: 30% fast     | Mixed decay: 30% fast
        //  1     |   1    | Slow decay                | Mixed decay: 30% fast
        //  Hi-Z  |   0    | Mixed decay: 60% fast     | Mixed decay: 60% fast
        //  Hi-Z  |   1    | Slow decay                | Slow decay

        // New decay settings take 10us to take effect.
        // forget() because dropping will turn back the GPIO into inputs.
        core::mem::forget(Output::new(decay0, Level::Low, Speed::Low));
        core::mem::forget(Output::new(decay1, Level::Low, Speed::Low));

        // vref is used to set the amount of current the motor receives.
        let mut pwm = SimplePwm::new_1ch4(pwm_timer, vref, 100.khz());
        pwm.set_duty(Channel::Ch4, ((pwm.get_max_duty() as u32) * MOTOR_CURRENT_PERCENT / 100) as u16);
        pwm.enable(Channel::Ch4);

        let step_multiplier = 0;

        Self { dir, step, enable, mode0, mode1, step_multiplier }
    }

    // Note: wait at least 200ns before STEP changes after changing the microstepping
    pub fn set_step_multiplier(&mut self, step_multiplier: u32) {
        // Multip.   | Mode0     | Mode1     | Step mode
        // ----------|-----------|-----------|------------
        //      256  | 0         |  0        | Full step (100% current)
        //           | 0         |  330k GND | Full step (71% current)
        //           | 1         |  0        | Non-circular 1/2 step
        //      128  | Hi-Z      |  0        | 1/2 step
        //       64  | 0         |  1        | 1/4 step
        //       32  | 1         |  1        | 1/8 step
        //       16  | Hi-Z      |  1        | 1/16 step
        //        8  | 0         |  Hi-Z     | 1/32 step
        //        4  | 0         |  330k GND | 1/64 step
        //        2  | Hi-Z      |  Hi-Z     | 1/128 step
        //        1  | 1         |  Hi-Z     | 1/256 step

        // Step multiplier 4 (1/64) is not available because we can't do the 330k GND configuration.

        if self.step_multiplier != step_multiplier {
            match step_multiplier {
                2|16|128 =>   { self.mode0.make_input(Pull::None) },
                4|8|64|256 => { self.mode0.make_output(Level::Low, Speed::Medium) },
                1|32 =>       { self.mode0.make_output(Level::High, Speed::Medium) },
                _ => { unimplemented!() },
            }

            match step_multiplier {
                1|2|8 =>    { self.mode1.make_input(Pull::None); },
                128|256 =>  { self.mode1.make_output(Level::Low, Speed::Medium); },
                16|32|64 => { self.mode1.make_output(Level::High, Speed::Medium); },
                _ => { unimplemented!() },
            }

            self.step_multiplier = step_multiplier;
        }
    }

    // Note: wait at least 200ns before STEP changes after changing the direction
    pub fn set_direction(&mut self, direction: Direction) {
        match direction {
            Direction::Up => self.dir.set_high(),
            Direction::Down => self.dir.set_low(),
        }
    }

    pub fn get_direction(&self) -> Direction {
        match self.dir.is_set_high() {
            true  => Direction::Up,
            false => Direction::Down,
        }
    }

    // Note: f() must take at least 1us to complete. Also, two consecutive calls
    // to do_step() should also be separated by 1us.
    pub fn do_step<R>(&mut self, mut f: impl FnMut(&mut Self) -> R) -> R {
        // The stepper motor advances when the `step` pin rises from low to high.
        // We have to hold the `step` pin high for at least 1us according to the datasheet.
        // Might as well do something useful during this time
        self.step.set_high();
        let ret = f(self);
        self.step.set_low();
        ret
    }

    // Note: STEP can only be toggled after 5us
    pub fn enable(&mut self) {
        self.enable.set_high();
    }

    pub fn disable(&mut self) {
        self.step.set_low();
        self.enable.set_low();
    }

    pub fn is_enabled(&self) -> bool {
        self.enable.is_set_high()
    }
}
