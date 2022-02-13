// SPDX-License-Identifier: GPL-3.0-or-later

use crate::consts::zaxis::hardware::*;

// We describe distances in mm as integers, in number of stepper moter steps to
// not loose accuracy with floating points.

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct Steps(pub i32);

const STEPS_PER_MM: f32 = (DRIVER_MICROSTEPS * FULL_STEPS_PER_REVOLUTION) as f32 / SCREW_THREAD_PITCH_MM;

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
    pub use super::Steps;

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
