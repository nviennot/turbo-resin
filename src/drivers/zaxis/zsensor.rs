// SPDX-License-Identifier: GPL-3.0-or-later

use stm32f1xx_hal::{
    prelude::*,
    gpio::*,
    gpio::gpiob::*,
};

use crate::debug;
use super::prelude::*;
use super::stepper::Stepper;

pub struct ZSensor {
    bottom: PB3<Input<PullUp>>,
}

const BOTTOM_SENSOR_POSITION_MM: f32 = 2.0;

const QUICK_HOMING_SPEED_MM_PER_SEC: f32 = 10.0;

// Position at which we start the fine homing
const FINE_HOMING_START_POSITION_MM: f32 = 0.5;

// < 2mm/s seems to produce precise results
// 0.5mm/s is very conservative
const FINE_HOMING_SPEED_MM_PER_SEC: f32 = 0.5;

impl ZSensor {
    pub fn new(
        bottom: PB3<Input<Floating>>,
        gpiob_crl: &mut Cr<CRL, 'B'>,
    ) -> Self {
        let bottom = bottom.into_pull_up_input(gpiob_crl);
        Self { bottom }
    }

    pub fn at_bottom(&self) -> bool {
        self.bottom.is_low()
    }

    pub fn calibrate(&mut self, stepper: &mut impl rtic::Mutex<T=Stepper>) {
        // max_speed here should be an argument.
        // We could be homing for build plate setup
        // Or we could be homing for the start of the print, and be submersed in resin
        // Different retract speed are needed.
        let max_speed = QUICK_HOMING_SPEED_MM_PER_SEC.mm();

        debug!("Calibrating start");

        debug!("0 Stopping if needed");
        stepper.lock(|s| s.controlled_stop());
        while !stepper.lock(|s| s.is_idle()) {}

        stepper.lock(|s| {
            s.set_max_speed(max_speed);
        });

        debug!("1 Going to the bottom FAST");
        if !self.at_bottom() {
            stepper.lock(|s| {
                s.set_target(Steps::MIN);
            });
            while !self.at_bottom() {}
            debug!("2 Reached the bottom");
            let bottom_position = stepper.lock(|s| s.current_position);

            debug!("3 Stopping");
            stepper.lock(|s| s.controlled_stop());
            while !stepper.lock(|s| s.is_idle()) {}

            let current_position = stepper.lock(|s| s.current_position);
            let overshoot_distance = bottom_position - current_position;
            debug!("overshoot = {}", overshoot_distance.as_mm());
        }

        debug!("4 Going back up a little");
        stepper.lock(|s| {
            // Fairly arbitrary. Too fast and it overshoots, and we wait longer to go down slow.
            s.set_max_speed((4.0*FINE_HOMING_SPEED_MM_PER_SEC).mm());
            s.set_target(Steps::MAX);
        });

        while self.at_bottom() {}
        stepper.lock(|s| {
            s.set_target_relative(FINE_HOMING_START_POSITION_MM.mm());
        });

        while !stepper.lock(|s| s.is_idle()) {}
        debug!("5 Reached the top");

        debug!("6 Going back down, SLOW");
        stepper.lock(|s| {
            s.set_max_speed(FINE_HOMING_SPEED_MM_PER_SEC.mm());
            s.set_target(Steps::MIN);
        });

        while !self.at_bottom() {}

        let sensor_position = stepper.lock(|s| {
            let current_position = s.current_position;
            s.set_origin(-BOTTOM_SENSOR_POSITION_MM.mm());
        });

        debug!("9 Done");

        // Going to 0 for example.
        let sensor_position = stepper.lock(|s| {
            s.set_max_speed(Steps(max_speed.0/2));
            s.set_target(0.mm());
        });
        while !stepper.lock(|s| s.is_idle()) {}
    }
}

