// SPDX-License-Identifier: GPL-3.0-or-later

use stm32f1xx_hal::{
    prelude::*,
    gpio::*,
    gpio::gpioa::*,
    gpio::gpiob::*,
    gpio::gpioc::*,
    gpio::gpioe::*,
    timer::{Timer, Tim2NoRemap, Event, CountDownTimer},
    afio::MAPR,
    pac::{TIM2, TIM7},
    pwm::Channel, rcc::Clocks,
};

use embedded_hal::digital::v2::OutputPin;

use crate::{consts::stepper::*, runtime::debug, drivers::clock::delay_ns};

#[derive(PartialEq, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
}

pub struct Drv8424 {
    step: PE5<Output<PushPull>>,
    dir: PE4<Output<PushPull>>,
    enable: PE6<Output<PushPull>>,
    mode0: PC3<Dynamic>,
    mode1: PC0<Dynamic>,
    gpioc_crl: Cr<CRL, 'C'>,
    pub step_multiplier: u32,
}

impl Drv8424 {
    pub fn new(
        dir: PE4<Input<Floating>>,
        step: PE5<Input<Floating>>,
        enable: PE6<Input<Floating>>,

        mode0: PC3<Input<Floating>>,
        mode1: PC0<Input<Floating>>,

        decay0: PC1<Input<Floating>>,
        decay1: PC2<Input<Floating>>,

        vref: PA3<Input<Floating>>,
        pwm_timer: Timer<TIM2>, // Or TIM5 in alternate mode.

        gpioa_crl: &mut Cr<CRL, 'A'>,
        mut gpioc_crl: Cr<CRL, 'C'>, // We need it to reconfigure microstepping at runtime
        gpioe_crl: &mut Cr<CRL, 'E'>,

        mapr: &mut MAPR,
    ) -> Self
    {
        // Pins that are related, but usage not known:
        // PC13 output (1)
        // PA2 output

        let dir = dir.into_push_pull_output(gpioe_crl);
        let step = step.into_push_pull_output(gpioe_crl);
        let enable = enable.into_push_pull_output(gpioe_crl);

        let mode0 = mode0.into_dynamic(&mut gpioc_crl);
        let mode1 = mode1.into_dynamic(&mut gpioc_crl);


        // Decay0 | Decay1 | Increasing Steps          | Decreasing Steps
        // -------|--------|---------------------------|----------------------------
        //  0     |   0    | Smart tune Dynamic Decay  | Smart tune Dynamic Decay
        //  0     |   1    | Smart tune Ripple Control | Smart tune Ripple Control
        //  1     |   0    | Mixed decay: 30% fast     | Mixed decay: 30% fast
        //  1     |   1    | Slow decay                | Mixed decay: 30% fast
        //  Hi-Z  |   0    | Mixed decay: 60% fast     | Mixed decay: 60% fast
        //  Hi-Z  |   1    | Slow decay                | Slow decay

        // New decay settings take 10us to take effect.
        decay0.into_push_pull_output_with_state(&mut gpioc_crl, PinState::Low);
        decay1.into_push_pull_output_with_state(&mut gpioc_crl, PinState::Low);

        // vref is used to set the amount of current the motor receives.
        let vref = vref.into_alternate_push_pull(gpioa_crl);
        let mut pwm = pwm_timer.pwm::<Tim2NoRemap, _, _, _>(vref, mapr, 100.khz());
        pwm.set_duty(Channel::C4, (((pwm.get_max_duty() as u32) * POWER_PERCENT) / 100) as u16);
        pwm.enable(Channel::C4);

        let step_multiplier = 0;

        Self { dir, step, enable, mode0, mode1, gpioc_crl, step_multiplier }
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
                2|16|128 =>   { self.mode0.make_floating_input(&mut self.gpioc_crl); },
                4|8|64|256 => { self.mode0.make_push_pull_output(&mut self.gpioc_crl); let _ = self.mode0.set_low(); },
                1|32 =>       { self.mode0.make_push_pull_output(&mut self.gpioc_crl); let _ = self.mode0.set_high(); },
                _ => { unimplemented!() },
            }

            match step_multiplier {
                1|2|8 =>    { self.mode1.make_floating_input(&mut self.gpioc_crl); },
                128|256 =>  { self.mode1.make_push_pull_output(&mut self.gpioc_crl); let _ = self.mode1.set_low(); },
                16|32|64 => { self.mode1.make_push_pull_output(&mut self.gpioc_crl); let _ = self.mode1.set_high(); },
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

    pub fn current_direction(&self) -> Direction {
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
