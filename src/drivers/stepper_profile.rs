// SPDX-License-Identifier: GPL-3.0-or-later

// This is an implementation of:
// An algorithm of linear speed control of a stepper motor in real time
// by Mihaylo Y. Stoychitch
// I prefer it to https://www.embedded.com/generate-stepper-motor-speed-profiles-in-real-time/

// It's not ideal to have small delay values because we'll lose precision on the
// speed requirements. For example, if the desired delay between two steps is
// 3.49, it will get rounded down to 3. and that's a 16% error of the desired
// speed. 0.5/MIN_DELAY_VALUE is the maximum speed error that we'll encouted.
// 5% is the most that we are willing to endure. So the minimum delay we are
// willing to tolerate is 10.
const MIN_DELAY_VALUE: u32 = 10;

pub struct StepperProfile {
    f: f32, // timer frequency

    ra: f32, // acceleration constant like in the paper
    rd: f32, // deceleration constant like in the paper

    f2_over_2d: f32, // used in end_approaching()

    c0: f32, // initial delay, determined by the acceleration
    ci: f32, // previous delay

    target_c: f32, // delay at desired speed. Set by f/max_speed.

    n: u32, // the current step, just to determine whether
            // to do corrections during the first 5 steps.

    remaining_steps: u32,
}

impl StepperProfile {
    pub fn new(timer_freq: f32, acceleration: f32, deceleration: f32, max_speed: f32) -> Self {
        let mut self_ = Self {
            f: timer_freq, remaining_steps: 0,
            // We set all the values to 0, and set them with the set_* functions to avoid duplicating code.
            ra: 0.0, rd: 0.0, c0: 0.0, ci: 0.0, target_c: 0.0, f2_over_2d: 0.0, n: 0,
        };

        self_.set_acceleration(acceleration);
        self_.set_decceleration(deceleration);
        self_.set_max_speed(max_speed);
        self_
    }

    pub fn set_acceleration(&mut self, acceleration: f32) {
        let f = self.f;
        self.c0 = f*sqrt(2.0/acceleration);
        self.ra = acceleration/(f*f);
    }

    pub fn set_decceleration(&mut self, deceleration: f32) {
        let f = self.f;
        self.rd = -deceleration/(f*f);
        self.f2_over_2d = (f*f)/(2.0*deceleration);
    }

    pub fn set_max_speed(&mut self, max_speed: f32) {
        self.target_c = self.f/max_speed;
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
}

impl Iterator for StepperProfile {
    type Item = f32;

    // On a Cortex-m4, the maximum number of cycles spent is 105 (during rampup).
    // While cruising, it's 70.
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_steps == 0 {
            self.n = 0;
            return None;
        }

        let next_ci = if self.n == 0 {
            self.c0
        } else {
            // Returns the next ci after applying some acceleration
            // inline to use as little cycles as possible.
            #[inline(always)]
            fn apply_acceleration(ci: f32, rate: f32) -> f32 {
                ci / (1.0 + rate*ci*ci)
            }

            // The if/elses make it slighly more complicated than what the paper
            // suggests we assume acceleration/deceleration/max_speed/remaining_steps
            // to be changing between two steps.

            let ci = self.ci;
            if self.end_approaching() {
                // We must slow down to avoid missing the target while
                // respecting the deceleration constraint
                apply_acceleration(ci, self.rd)
            } else if self.target_c == ci {
                // We are cruising.
                ci
            } else if self.target_c < ci {
                // We are going too slow. Accelerate, but don't go over self.target_c.
                max(apply_acceleration(ci, self.ra), self.target_c)
            } else {
                // We are going too fast. The max_speed may have been adjusted.
                // Deccelerate, but don't go under self.target_c.
                min(apply_acceleration(ci, self.rd), self.target_c)
            }
        };

        // These are the early/late step corrections as decribed in the paper.
        // Not sure how critical this is, but it's fairly cheap to implement.
        let next_ci = {
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

        self.ci = next_ci;
        self.remaining_steps -= 1;
        self.n += 1;

        Some(next_ci)
    }
}

#[inline(always)]
fn sqrt(v: f32) -> f32 {
    unsafe { core::intrinsics::sqrtf32(v) }
}

// Here we don't use the f32::min, because it's slower. It doesn't inline, and
// does a bunch of extra stuff (see the assembly).
#[inline(always)]
fn min(a: f32, b: f32) -> f32 {
    if a <= b { a } else { b }
}

#[inline(always)]
fn max(a: f32, b: f32) -> f32 {
    if a >= b { a } else { b }
}
