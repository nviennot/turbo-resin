// SPDX-License-Identifier: GPL-3.0-or-later

use stm32f1xx_hal::{
    gpio::*,
    gpio::gpiob::*,
};

pub struct BottomSensor {
    pin: PB3<Input<PullUp>>,
}

impl BottomSensor {
    pub fn new(
        pin: PB3<Input<Floating>>,
        gpiob_crl: &mut Cr<CRL, 'B'>,
    ) -> Self {
        let pin = pin.into_pull_up_input(gpiob_crl);
        Self { pin }
    }

    pub fn active(&self) -> bool {
        self.pin.is_low()
    }
}
