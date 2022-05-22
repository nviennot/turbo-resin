// SPDX-License-Identifier: GPL-3.0-or-later

pub mod system {
    pub const CLOCK_SPEED_MHZ: u32 = 168;
}

pub mod ext_flash {
    pub const FLASH_SIZE: u32 = 16*1024*1024; // 16MB
    pub const SPI_FREQ_HZ: u32 = 20_000_000;
}

pub mod display {
    pub const WIDTH: u16 = 480;
    pub const HEIGHT: u16 = 320;
    pub const LVGL_BUFFER_LEN: usize = 7680; // 1/10th of the display size
}

pub mod lcd {
    pub const WIDTH: u32 = 3840;
    pub const HEIGHT: u32 = 2400;
    // The original firmware uses 2Mhz, we'll bump that up a little
    pub const SPI_FREQ_HZ: u32 = 5_000_000;

    pub const BITSTREAM_HEADER_OFFSET: u32 = 0x79000;
    pub const BITSTREAM_MAGIC: u32 = 0x12FD0022;
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
        // The 10mm/s gives us a 0.85mm overshoot (measured) when we pass the
        // sensor with the deceleration at 60 mm/s^2. It's fine. We allow 2 mm.
        // Note: The overshoot formula is MAX_SPEED**2/DECELERATION/2.
        pub const PHASE1_HOMING_SPEED_MM_PER_SEC: f32 = 10.0;
        // Phase 2 speed: We rise up above the z-axis bottom sensor at a moderate speed.
        pub const PHASE2_HOMING_SPEED_MM_PER_SEC: f32 = 2.0;
        // Phase 3 speed: This is the speed that matters to find precisely where
        // the bottom sensor activates. We are going at slow speed, but we are
        // going through a small distance.
        pub const PHASE3_HOMING_SPEED_MM_PER_SEC: f32 = 0.2;
    }
}

pub mod io {
    // This should be at least one block_size = 512 to avoid degrading perfs
    pub const FILE_READER_BUFFER_SIZE: usize = 1024;
}

pub mod touch_screen {
    // The higher the more sensitive to touches.
    // Under full pressure, pressure == 2.0
    // Under light touch, pressure == 6.0
    pub const PRESSURE_THRESHOLD: f32 = 2.5;

    pub const STABLE_X_Y_VALUE_TOLERANCE: u16 = 8; // in pixels
    // Number of consequtive samples to validate
    pub const NUM_STABLE_SAMPLES: u8 = 8;
    pub const SAMPLE_DELAY_MS: u64 = 1;
    pub const SLEEP_DELAY_MS: u64 = 20;

    pub const TOP_LEFT: (u16, u16) = (2230, 100);
    pub const BOTTOM_RIGHT: (u16, u16) = (4000, 1870);

    // Original firmware uses 650kHz, but that seems a bit low
    pub const SPI_FREQ_HZ: u32 = 2_000_000;
}
