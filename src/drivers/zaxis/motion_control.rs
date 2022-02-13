// SPDX-License-Identifier: GPL-3.0-or-later

use stm32f1xx_hal::{
    prelude::*,
    timer::{Timer, Event, CountDownTimer},
    pac::TIM7,
};

use super::step_generator::StepGenerator;

use crate::consts::zaxis::{
    stepper::*,
    motion_control::*,
};


use super::{
    prelude::*,
    drv8424::{Drv8424, Direction},
};

pub struct MotionControl {
    drv8424: Drv8424,
    step_timer: CountDownTimer<TIM7>,
    stepgen: StepGenerator,
    current_position: Steps,
    target: Steps,
}

impl MotionControl {
    pub fn new(
        drv8424: Drv8424,
        step_timer: Timer<TIM7>, // Any timer will do.
    ) -> Self {
        let stepgen = StepGenerator::new(
            MAX_ACCELERATION.mm().0 as f32,
            MAX_DECELERATION.mm().0 as f32,
            MAX_SPEED.mm().0 as f32,
        );

        let step_timer = step_timer.start_with_tick_freq(STEP_TIMER_FREQ.hz());

        let current_position = Steps(0);
        let target = Steps(0);

        Self { drv8424, step_timer, stepgen, current_position, target }
    }

    pub fn on_interrupt(&mut self) {
        self.step_timer.clear_update_interrupt_flag();

        let next_delay = self.do_step(|stepgen| {
            // We do some useful things while we wait for the 1us delay to pass
            // holding the STEP pin high.
            stepgen.next()
            // XXX If we are running faster than 120Mhz, we would need to
            // introduce an additional delay here.
        });

        if let Some((delay_us, multiplier)) = next_delay {
            self.drv8424.set_step_multiplier(multiplier);

            let arr = if delay_us >= u16::MAX as f32 {
                u16::MAX
            } else {
                // f+0.5 is to round the value to the nearest integer
                // sub(1) is because a value of arr=0 generates an interrupt every 1us.
                ((delay_us + 0.5) as u16).saturating_sub(1)
            };

            self.step_timer.set_arr(arr);
            // Note: if cnt > arr at this point, an interrupt event is generated
            // immediately. This is what we want.
            // But it should not happen because MIN_DELAY_VALUE == 15.
            // This whole interrupt routine takes at most 300 CPU cycles to run.
            // That's 2.5us. That's a x6 margin.
        } else {
            self.hard_stop();
        }
    }

    // If max_speed is None, it goes back to default.
    pub fn set_max_speed(&mut self, max_speed: Steps) {
        self.stepgen.set_max_speed(max_speed.0 as f32);
    }

    pub fn get_max_speed(&self) -> Steps {
        Steps(self.stepgen.get_max_speed() as i32)
    }

    pub fn get_current_position(&self) -> Steps {
        self.current_position
    }

    // relative to current position
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

        self.drv8424.set_direction(dir);
        self.drv8424.set_step_multiplier(1);
        self.drv8424.enable();

        // steps-1 because we are going to do the first step immedately.
        self.stepgen.set_remaining_steps(steps-1);

        // We need to hold the enable pin high for 5us before we can start
        // stepping the motor. That's from the DRV8424 datasheet.
        self.step_timer.set_arr(5);
        self.step_timer.reset();

        self.step_timer.listen(Event::Update);
    }

    pub fn set_origin(&mut self, origin_position: Steps) {
        self.target = self.target + self.current_position - origin_position;
        self.current_position = -origin_position;
    }

    pub fn stop(&mut self) {
        self.stepgen.set_remaining_steps(
            self.stepgen.num_steps_to_stop()
        );
    }

    pub fn hard_stop(&mut self) {
        self.stepgen.set_remaining_steps(0);
        self.target = self.current_position;

        self.step_timer.unlisten(Event::Update);
        self.drv8424.disable();
    }

    pub fn is_idle(&self) -> bool {
        !self.drv8424.is_enabled()
    }

    pub fn do_step<R>(&mut self, mut f: impl FnMut(&mut StepGenerator) -> R) -> R {
        let current_position = &mut self.current_position;
        let stepgen = &mut self.stepgen;

        self.drv8424.do_step(|drv| {
            match drv.get_direction() {
                Direction::Up   => current_position.0 += drv.step_multiplier as i32,
                Direction::Down => current_position.0 -= drv.step_multiplier as i32,
            }
            f(stepgen)
        })
    }
}
