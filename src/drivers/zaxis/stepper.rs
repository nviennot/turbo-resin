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
use stm32f1xx_hal::rcc::BusTimerClock;

use super::step_generator::StepGenerator;

use crate::{consts::stepper::*, runtime::debug, drivers::clock::delay_ns};
const STEPS_PER_MM: f32 = (DRIVER_MICROSTEPS * FULL_STEPS_PER_REVOLUTION) as f32 / SCREW_THREAD_PITCH_MM;

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct Steps(pub i32);

impl Steps {
    pub const MIN: Self = Self(i32::MIN/2);
    pub const MAX: Self = Self(i32::MAX/2);

    pub fn as_mm(self) -> f32 {
        (self.0 as f32) / STEPS_PER_MM
    }
}

impl core::ops::Add for Steps {
    type Output = Steps;
    fn add(self, rhs: Self) -> Self::Output {
        Steps(self.0 + rhs.0)
    }
}

impl core::ops::Sub for Steps {
    type Output = Steps;
    fn sub(self, rhs: Self) -> Self::Output {
        Steps(self.0 - rhs.0)
    }
}

impl core::ops::Neg for Steps {
    type Output = Steps;

    fn neg(self) -> Self::Output {
        Steps(-self.0)
    }
}

pub mod prelude {
    use super::*;

    pub trait StepsExt {
        fn mm(self) -> Steps;
    }

    impl StepsExt for f32 {
        fn mm(self) -> Steps {
            Steps((self * STEPS_PER_MM) as i32)
        }
    }

    impl StepsExt for i32 {
        fn mm(self) -> Steps {
            (self as f32).mm()
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
}

use prelude::*;

pub struct Stepper {
    step_timer: CountDownTimer<TIM7>,
    step: PE5<Output<PushPull>>,
    dir: PE4<Output<PushPull>>,
    enable: PE6<Output<PushPull>>,
    profile: StepGenerator,
    pub current_position: Steps,
    pub target: Steps,
    pub max_speed: Steps,

    mode0: PC3<Dynamic>,
    mode1: PC0<Dynamic>,
    pub step_multiplier: u32,

    gpioc_crl: Cr<CRL, 'C'>,
}

impl Stepper {
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

        step_timer: Timer<TIM7>, // Any timer will do.

        gpioa_crl: &mut Cr<CRL, 'A'>,
        mut gpioc_crl: Cr<CRL, 'C'>, // We need it to reconfigure microstepping at runtime
        gpioe_crl: &mut Cr<CRL, 'E'>,
        mapr: &mut MAPR,
        clocks: &Clocks,
    ) -> Self {
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


        let vref = vref.into_alternate_push_pull(gpioa_crl);


        // TIMER2 (or timer5 remapped), CH4
        let mut pwm = pwm_timer.pwm::<Tim2NoRemap, _, _, _>(vref, mapr, 100.khz());
        pwm.set_duty(Channel::C4, (((pwm.get_max_duty() as u32) * POWER_PERCENT) / 100) as u16);
        pwm.enable(Channel::C4);

        let profile = StepGenerator::new(
            MAX_ACCELERATION.mm().0 as f32,
            MAX_DECELERATION.mm().0 as f32,
            DEFAULT_MAX_SPEED.mm().0 as f32,
        );

        let step_timer = step_timer.start_with_tick_freq(STEP_TIMER_FREQ.hz());

        let current_position = Steps(0);
        let target = Steps(0);
        let max_speed = DEFAULT_MAX_SPEED.mm();

        let step_multiplier = 0;

        Self {
            step_timer, step, dir, enable, profile, current_position,
            max_speed, target, mode0, mode1, step_multiplier, gpioc_crl,
        }
    }

    pub fn on_interrupt(&mut self) {
        let next_delay = self.do_step(|self_| {
            // We do some useful things while we wait for the 1us delay for
            // holding the STEP pin high.
            self_.profile.next()
            // XXX If we are running faster than 120Mhz, we would need to
            // introduce an additional delay here.
        });

        if let Some((delay_us, multiplier)) = next_delay {
            if multiplier == 4 {
                // We don't have access ot the multiplier 4 (1/64 microstepping).
                // We'll do two steps at multiplier 2. Not really ideal, but good enough.
                self.set_step_multiplier(2);
                delay_ns(1000);
                self.do_step(|_| delay_ns(1000));
            } else {
                self.set_step_multiplier(multiplier);
            }

            let arr = if delay_us >= u16::MAX as f32 {
                u16::MAX
            } else {
                // f+0.5 is to round the value to the nearest integer
                // sub(1) is because a value of arr=0 generates an interrupt every 1us.
                ((delay_us + 0.5) as u16).saturating_sub(1)
            };
            self.step_timer.set_arr(arr);

            if self.step_timer.cnt() >= arr {
                // If we have passed the delay we wanted, we need to do the next
                // step immedately. This should never happen because
                // MIN_DELAY_VALUE == 20, and we should have plenty of time to
                // do our things.
            } else {
                self.step_timer.clear_update_interrupt_flag();
            }
        } else {
            self.stop();
            self.step_timer.clear_update_interrupt_flag();
        }
    }

    fn do_step<R>(&mut self, mut f: impl FnMut(&mut Self) -> R) -> R {
        // The stepper motor advances when the `step` pin rises from low to high.
        // We have to hold the `step` pin high for at least 1us according to the datasheet.
        self.step.set_high();

        match self.current_direction() {
            Direction::Up   => self.current_position.0 += self.step_multiplier as i32,
            Direction::Down => self.current_position.0 -= self.step_multiplier as i32,
        }

        let ret = f(self);
        self.step.set_low();

        ret
    }

    fn current_direction(&self) -> Direction {
        match self.dir.is_set_high() {
            true => Direction::Up,
            false => Direction::Down,
        }
    }

    // Note: wait at least 200ns before STEP changes after changing the direction
    fn set_direction(&mut self, direction: Direction) {
        match direction {
            Direction::Up => self.dir.set_high(),
            Direction::Down => self.dir.set_low(),
        }
    }

    // If max_speed is None, it goes back to default.
    pub fn set_max_speed(&mut self, max_speed: Option<Steps>) {
        self.max_speed = max_speed.unwrap_or(DEFAULT_MAX_SPEED.mm());
        self.profile.set_max_speed(self.max_speed.0 as f32);
    }

    // to current position
    pub fn set_target_relative(&mut self, steps: Steps) {
        self.set_target(self.current_position + steps);
    }

    pub fn set_target(&mut self, target: Steps) {
        self.target = target;
        let steps = target - self.current_position;

        if steps.0 == 0 {
            return;
        }

        let (dir, steps) = if steps.0 > 0 {
            (Direction::Up, steps.0 as u32)
        } else {
            (Direction::Down, -steps.0 as u32)
        };

        self.set_direction(dir);
        self.set_step_multiplier(1);

        // steps-1 because we are going to do the first step immedately.
        self.profile.set_remaining_steps(steps-1);

        // We need to hold the enable pin high for 5us before we can start stepping the motor.
        self.enable.set_high();

        self.step_timer.set_arr(5);
        self.step_timer.reset();

        self.step_timer.listen(Event::Update);
    }

    pub fn set_origin(&mut self, origin_position: Steps) {
        self.target = self.target + self.current_position - origin_position;
        self.current_position = -origin_position;
    }

    pub fn controlled_stop(&mut self) {
        self.profile.set_remaining_steps(
            self.profile.num_steps_to_stop()
        );
    }

    pub fn stop(&mut self) {
        self.profile.set_remaining_steps(0);
        self.target = self.current_position;

        self.step_timer.unlisten(Event::Update);
        self.step.set_low();
        self.enable.set_low();
    }

    pub fn is_idle(&self) -> bool {
        self.enable.is_set_low()
    }
}
