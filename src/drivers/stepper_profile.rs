// SPDX-License-Identifier: GPL-3.0-or-later

// This is an implementation of:
// An algorithm of linear speed control of a stepper motor in real time
// by Mihaylo Y. Stoychitch

pub struct StepperProfile {
    f: f32, // timer frequency

    ra: f32, // acceleration constant like in the paper
    rd: f32, // deceleration constant like in the paper

    f2_over_2d: f32, // used in end_approaching()

    c0: f32, // initial delay, determined by the acceleration
    ci: f32, // previous delay

    n: u32, // the current step, just to determine whether to do corrections during the first 5 steps.

    target_c: f32, // desired speed. Set by f/max_speed.

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
        self_.ci = self_.c0;
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

    pub fn end_approaching(&mut self) -> bool {
        // it takes n = v**2/(2*deceleration) steps to come to a full stop. Current speed is f/ci
        self.remaining_steps as f32 * self.ci * self.ci < self.f2_over_2d
    }

    pub fn num_steps_to_stop(&self) -> u32 {
        (self.f2_over_2d / (self.ci * self.ci) + 0.5) as u32
    }
}

impl Iterator for StepperProfile {
    type Item = u32;

    // On cortex m4, the maximum number of cycles spent is 105 (during rampup). While cruising, it's 70.
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_steps == 0 {
            self.n = 0;
            return None;
        }

        let next_ci = if self.n == 0 {
            self.c0
        } else {
            let ci = self.ci;

            if self.end_approaching() {
                // We must slow down to avoid missing the target while
                // respecting the deceleration constraint
                ci / (1.0 + self.rd*ci*ci)
            } else if self.target_c == ci {
                // We are cruising.
                ci
            } else if self.target_c < ci {
                // We are going too slow.
                let next_ci = ci / (1.0 + self.ra*ci*ci);
                if self.target_c >= next_ci {
                    // Tried to speed up too much
                    self.target_c
                } else {
                    next_ci
                }
            } else {
                // We are going too fast. The max_speed may have been adjusted.
                let next_ci = ci / (1.0 + self.rd*ci*ci);
                if self.target_c <= next_ci {
                    // Tried to slow down too much
                    self.target_c
                } else {
                    next_ci
                }
            }
        };

        let next_ci = {
            // These are the early steps corrections as decribed in the paper.
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

        Some((next_ci + 0.5) as u32)
    }
}

#[inline(always)]
pub fn sqrt(v: f32) -> f32 {
    use core::intrinsics::sqrtf32;
    unsafe { sqrtf32(v) }
}
