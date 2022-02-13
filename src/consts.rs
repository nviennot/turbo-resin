// SPDX-License-Identifier: GPL-3.0-or-later

pub mod ext_flash {
    const FLASH_SIZE: u32 = 16*1024*1024; // 16MB
}

pub mod display {
    pub const WIDTH: u16 = 320;
    pub const HEIGHT: u16 = 240;
    pub const LVGL_BUFFER_LEN: usize = 7680; // 1/10th of the display size
}

pub mod zaxis {
    pub mod hardware {
        pub const DRIVER_MICROSTEPS: u32 = 256;
        pub const FULL_STEPS_PER_REVOLUTION: u32 = 200;
        pub const SCREW_THREAD_PITCH_MM: f32 = 2.0;
        pub const MOTOR_CURRENT_PERCENT: u32 = 70;
    }

    pub mod motion_control {
        pub const MAX_SPEED: f32 = 20.0; // mm/s
        pub const MAX_ACCELERATION: f32 = 25.0; // mm/s^2
        pub const MAX_DECELERATION: f32 = 60.0; // mm/s^2
    }

    pub mod stepper {
        // Here we go with a 1us timer. Precise enough for our purposes.
        pub const STEP_TIMER_FREQ: u32 = 1_000_000;
        // It's not ideal to have small delay values because we'll lose
        // precision on the speed requirements. Also, small delays means that
        // we'll spend too much time spending CPU cycles stepping the motor. Too
        // large of a minimum delay value, and the stepper motor will have more
        // chance to be noisy.
        // With 15 minimal delay value, we get a 0.5/15 = 3% speed error at most.
        pub const STEP_TIMER_MIN_DELAY_VALUE: f32 = 15.0;
    }

    pub mod origin_calibration {
        // We consider Z=2mm the position where the bottom sensor activates.
        // This difference is good so that when we try to find the origin next
        // time, we don't crash into the LCD panel because decelerating takes time.
        pub const BOTTOM_SENSOR_POSITION_MM: f32 = 2.0;
        // Phase 1 speed: We are going down from an arbitrary place to reach the
        // bottom where the bottom sensor activates.
        pub const PHASE1_HOMING_SPEED_MM_PER_SEC: f32 = 10.0;
        // Phase 2 speed: We rise up above the z-axis bottom sensor at a moderate speed.
        pub const PHASE2_HOMING_SPEED_MM_PER_SEC: f32 = 2.0;
        // Phase 3 speed: This is the speed that matters to find precisely where
        // the bottom sensor activates. We are going at slow speed, but we are
        // going through a small distance.
        pub const PHASE3_HOMING_SPEED_MM_PER_SEC: f32 = 0.2;
    }
}

pub mod touch_screen {
    // The higher the more sensitive to touches.
    // Under full pressure, pressure == 2.0
    // Under light touch, pressure == 6.0
    pub const PRESSURE_THRESHOLD: f32 = 5.0;

    pub const STABLE_X_Y_VALUE_TOLERANCE: u16 = 8; // in pixels
    // Number of consequtive samples to validate
    pub const NUM_STABLE_SAMPLES: u8 = 8;
    pub const SAMPLE_DELAY_MS: u64 = 1;
}
