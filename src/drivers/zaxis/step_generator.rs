// SPDX-License-Identifier: GPL-3.0-or-later

// This is an implementation of:
// An algorithm of linear speed control of a stepper motor in real time
// by Mihaylo Y. Stoychitch
// See http://annals.fih.upt.ro/pdf-full/2013/ANNALS-2013-3-06.pdf
// I prefer it to compared to https://www.embedded.com/generate-stepper-motor-speed-profiles-in-real-time/

use crate::consts::zaxis::{
    hardware::*,
    stepper::*,
};

const TIMER_FREQ: f32 = STEP_TIMER_FREQ as f32;
const MAX_STEP_MULTIPLIER: u32 = DRIVER_MICROSTEPS;
// MIN_DELAY_VALUE is most of the time respected. It can be that for a single
// step, the delay is going to be smaller, but immediately after, the step
// multiplier will be corrected.
// We could do a better implementation.
const MIN_DELAY_VALUE: f32 = STEP_TIMER_MIN_DELAY_VALUE;

// The DRV8424 doesn't allow 1/64 microstepping because of the pin configuration
const FORBIDDEN_MULTIPLIER: u32 = 4;

pub struct StepGenerator {
    ra: f32, // acceleration constant like in the paper
    rd: f32, // deceleration constant like in the paper

    f2_over_2d: f32, // used in end_approaching()

    c0: f32, // initial delay, determined by the acceleration
    ci: f32, // previous delay

    target_c: f32, // delay at desired speed. Set by f/max_speed.

    n: u32, // the current step

    remaining_steps: u32, // remaining steps. This is how we know that we need to move.

    // We want to change micro-stepping dynamically. This is the current step
    // multiplier.  We start with 1, and can go up to MAX_STEP_MULTIPLIER=256,
    // in increment of powers of two.
    step_multiplier: u32,
}

impl StepGenerator {
    pub fn new(acceleration: f32, deceleration: f32, max_speed: f32) -> Self {
        let mut self_ = Self {
            // We set all the values to 0.0, and set them with the set_* functions
            // to avoid duplicating code.
            ra: 0.0, rd: 0.0, c0: 0.0, ci: 0.0, target_c: 0.0, f2_over_2d: 0.0,
            n: 0, remaining_steps: 0, step_multiplier: 1,
        };

        self_.set_acceleration(acceleration);
        self_.set_deceleration(deceleration);
        self_.set_max_speed(max_speed);
        self_
    }

    pub fn set_acceleration(&mut self, acceleration: f32) {
        let f = TIMER_FREQ;
        self.c0 = f*sqrt(2.0/acceleration);
        self.ra = acceleration/(f*f);
    }

    pub fn set_deceleration(&mut self, deceleration: f32) {
        let f = TIMER_FREQ;
        self.rd = -deceleration/(f*f);
        self.f2_over_2d = (f*f)/(2.0*deceleration);
    }

    pub fn set_max_speed(&mut self, max_speed: f32) {
        self.target_c = TIMER_FREQ/max_speed;
    }

    pub fn get_max_speed(&self) -> f32 {
        TIMER_FREQ/self.target_c
    }

    pub fn set_remaining_steps(&mut self, steps: u32) {
        self.remaining_steps = steps;
    }

    pub fn end_approaching(&self) -> bool {
        // The current speed is v=f/ci
        // it takes n = v**2/(2*deceleration) steps to come to a full stop.
        // We avoid using num_steps_to_stop(), because there's a division, and
        // that's 14 cycles. A multiplication is a single cycle.
        self.remaining_steps as f32 * self.ci * self.ci <= self.f2_over_2d
    }

    pub fn num_steps_to_stop(&self) -> u32 {
        let n = self.f2_over_2d / (self.ci * self.ci);
        // We round a to avoid problems with end_approaching(). Note that if we
        // do an extra step while decelerating, it's not really a big deal.
        (n+0.5) as u32
    }

    pub fn adjust_step_multiplier(&mut self) {
        let m = self.step_multiplier;
        let ci = self.ci;
        let effective_ci = ci*(m as f32);

        let increase_rate = if m*2 == FORBIDDEN_MULTIPLIER { 4 } else { 2 };
        let decrease_rate = if m/2 == FORBIDDEN_MULTIPLIER { 4 } else { 2 };

        if self.n == 0 {
            self.step_multiplier = 1;
        } else if self.remaining_steps < self.step_multiplier {
            self.step_multiplier /= decrease_rate;
        } else if effective_ci < MIN_DELAY_VALUE && m != MAX_STEP_MULTIPLIER {
            // If the delay value becomes too small, we won't be able to keep up
            // sending pulses fast enough. We must rise the step multiplier.
            //  But we can only do so if the
            // current step position is at a multiple of the step multipler.
            // Otherwise, the hardware driver will just skip some steps in order to
            // snap to the nearest microstepping setting.
            // Also we assume that the caller does a single step before invoking
            // the first next() call, hence the +1. It doesn't really change much,
            // a 1/256 microstep is so small.
            let next_multiplier = m*increase_rate;
            if (self.n+1) % next_multiplier == 0 {
               self.step_multiplier = next_multiplier;
            }
        } else if m != 1 && effective_ci > MIN_DELAY_VALUE*(decrease_rate as f32) + 0.01 {
            // We add 0.01 to the condition to avoid flip flopping between two
            // multipliers because of potential rounding errors. This condition
            // hasn't been verified, I'm just being paranoid.
            self.step_multiplier /= decrease_rate;
        }
    }
}

impl Iterator for StepGenerator {
    type Item = (f32, u32);

    // On a Cortex-m4, This takes between 113 cycles and 150 cycles.
    // Use the test() function below to see this in action.
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_steps == 0 {
            // Respect the lower bound of the number of cycles this function takes.
            // It's useful to do the computation during the pulse of the STEP
            // pin, which has a minimum timing constraint.
            cortex_m::asm::delay(45);
            self.n = 0;
            return None;
        }

        self.adjust_step_multiplier();
        let m = self.step_multiplier;

        let next_ci = if self.n == 0 {
            // See comment above for an explaination of this delay.
            cortex_m::asm::delay(30);
            // self.step_multiplier is always 1 when starting, so this is correct.
            self.c0
        } else {
            // Returns the next ci after applying some acceleration
            // inline to use as little cycles as possible.
            #[inline(always)]
            fn apply_acceleration(ci: f32, rate: f32) -> f32 {
                // For some reason, the formula of the paper isn't that good.
                // For example, when decelerating, we could find a way to divide
                // by 0. That's not good. This is a workaround, but it would be
                // nice to have a correct formula.
                ci / (1.0 + rate*ci*ci).clamp(0.01, 100.0)
            }

            // The if/elses make it slighly more complicated than what the paper
            // suggests. Here we assume that acceleration, deceleration,
            // max_speed, remaining_steps to be changing between two steps.

            let ci = self.ci;
            let m = m as f32;
            if self.end_approaching() {
                // We must slow down to avoid missing the target while
                // respecting the deceleration constraint
                apply_acceleration(ci, m*self.rd)
            } else if self.target_c == ci {
                // We are cruising.
                ci
            } else if self.target_c < ci {
                // We are going too slow. Accelerate, so decrease ci.
                // But don't go lower than self.target_c.
                max(apply_acceleration(ci, m*self.ra), self.target_c)
            } else {
                // We are going too fast. The max_speed may have been adjusted.
                // Deccelerate, so increase ci, but don't go above self.target_c.
                min(apply_acceleration(ci, m*self.rd), self.target_c)
            }
        };

        // These are the early/late step corrections as decribed in the paper.
        // Not sure how critical this is, but it's fairly cheap to implement.
        let next_ci = {
            // We'll most likely be at the maximum microstepping resolution for
            // these, which is to say, self.step_multiplier == 1. So we don't need
            // to worry about microstepping here.
            static CORRECTION: [f32; 5] = [
                1.0 + 0.08/1.0,
                1.0 + 0.08/2.0,
                1.0 + 0.08/3.0,
                1.0 + 0.08/4.0,
                1.0 + 0.08/5.0,
            ];

            match (self.n, self.remaining_steps) {
                (n@1..=5, _) | (_, n@1..=5) => next_ci * CORRECTION[(n-1) as usize],
                _ => next_ci
            }
        };

        self.remaining_steps = self.remaining_steps.checked_sub(m).unwrap();
        self.n += m;
        self.ci = next_ci;

        let effective_ci = next_ci * (m as f32);

        // FIXME effective_ci may be smaller than MIN_DELAY_VALUE, just for one
        // or two iterations. The delay will be in the right range, as the
        // multiplier gets fixed. It's not great.
        // There's not much harm done though.
        // Having said that, there will be harm if effective_ci gets rounded to 0.
        assert!(effective_ci > 1.0);

        Some((effective_ci, m))
    }
}

#[inline(always)]
fn sqrt(v: f32) -> f32 {
    unsafe { core::intrinsics::sqrtf32(v) }
}

// Here we don't use the f32::min, because it's slower. It doesn't inline, and
// does a bunch of extra stuff.
#[inline(always)]
fn min(a: f32, b: f32) -> f32 {
    if a <= b { a } else { b }
}

#[inline(always)]
fn max(a: f32, b: f32) -> f32 {
    if a >= b { a } else { b }
}

/*
pub fn test(s: &mut StepGenerator) {
    s.set_max_speed(1_000_00.0);
    s.set_acceleration(1_000_000_00.0);
    s.set_deceleration(1_000_000_00.0);

    s.set_remaining_steps(120);

    let mut step = 0;
    let mut multiplier = 1;
    let mut time = 0.0;
    loop {
        step += multiplier;
        crate::drivers::clock::delay_ns(20_000_000); // give the debugging buffer some time to be flushed

        let result = crate::drivers::clock::count_cycles(|| s.next());
        //let result = self.next();

        if let Some((delay_us, m)) = result {
            //crate::debug!("step:{:3}, m:{:3}, delay:{:5.0}us, time:{:8.5} speed:{:5.1} steps/s",
                //step, m, delay_us, time/TIMER_FREQ, (m as f32)*TIMER_FREQ/delay_us);
            multiplier = m;
            time += delay_us;
        } else {
            break;
        }
    }

    crate::debug!("Done");
}
*/
