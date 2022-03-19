// SPDX-License-Identifier: GPL-3.0-or-later

use embassy_stm32::gpio::{Input, Pull};
use embassy_stm32::peripherals as p;

pub struct BottomSensor {
    pin: Input<'static, p::PB3>,
}

impl BottomSensor {
    pub fn new(
        pin: p::PB3,
    ) -> Self {
        let pin = Input::new(pin, Pull::Up);
        Self { pin }
    }

    pub fn active(&self) -> bool {
        self.pin.is_low()
    }
}
