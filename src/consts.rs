// SPDX-License-Identifier: GPL-3.0-or-later

pub mod system {
    pub const HEAP_SIZE: usize = 10*1024;
    pub const SYSTICK_HZ: u32 = 1_000; // 1ms ticks
}

pub mod ext_flash {
    const FLASH_SIZE: u32 = 16*1024*1024; // 16MB
}

pub mod display {
    pub const WIDTH: u16 = 320;
    pub const HEIGHT: u16 = 240;
    pub const LVGL_BUFFER_LEN: usize = 7680; // 1/10th of the display size
}

pub mod stepper {
    // It's not ideal to have small delay values because we'll lose precision on
    // the speed requirements. Also, small delays means that we'll spend too
    // much time spending CPU cycles stepping the motor.
    pub const MIN_DELAY_VALUE: f32 = 20.0;
    pub const STEP_TIMER_FREQ: u32 = 1_000_000;

    pub const DRIVER_MICROSTEPS: u32 = 256;
    pub const FULL_STEPS_PER_REVOLUTION: u32 = 200;
    pub const SCREW_THREAD_PITCH_MM: f32 = 2.0;

    pub const DEFAULT_MAX_SPEED: f32 = 30.0; // mm/s
    pub const MAX_ACCELERATION: f32 = 25.0; // mm/s^2
    pub const MAX_DECELERATION: f32 = 60.0; // mm/s^2

    pub const POWER_PERCENT: u32 = 70; // 70% of power
}

pub mod touch_screen {
    // The higher the more sensitive to touches.
    // Under full pressure, pressure == 2.0
    // Under light touch, pressure == 6.0
    pub const PRESSURE_THRESHOLD: f32 = 5.0;

    // Number of consequtive samples to validate
    pub const STABLE_X_Y_VALUE_TOLERANCE: u16 = 8; // in pixels
    pub const NUM_STABLE_SAMPLES: u8 = 8;
    pub const DEBOUNCE_INT_DELAY_MS: u8 = 1;
    pub const SAMPLE_DELAY_MS: u8 = 1;
}
