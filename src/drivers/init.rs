// SPDX-License-Identifier: GPL-3.0-or-later

use stm32f1xx_hal::{
    prelude::*,
    pac,
    timer::Timer,
    delay::Delay,
};

use crate::drivers::{
    ext_flash::ExtFlash,
    display::Display,
    touch_screen::TouchScreen,
    stepper::Stepper,
    lcd::Lcd,
    clock,
};

use crate::consts::system::*;

use super::zsensor::ZSensor;

pub type Systick = systick_monotonic::Systick<{ SYSTICK_HZ }>;
pub mod prelude {
    pub use systick_monotonic::ExtU64;
}

pub struct Machine {
    pub ext_flash: ExtFlash,
    pub display: Display,
    pub touch_screen: TouchScreen,
    pub stepper: Stepper,
    pub systick: Systick,
    pub lcd: Lcd,
    pub zsensor: ZSensor,
}

impl Machine {
    pub fn new(cp: cortex_m::Peripherals, dp: pac::Peripherals) -> Self {
        let mut gpioa = dp.GPIOA.split();
        let mut gpiob = dp.GPIOB.split();
        let mut gpioc = dp.GPIOC.split();
        let mut gpiod = dp.GPIOD.split();
        let mut gpioe = dp.GPIOE.split();

        let mut afio = dp.AFIO.constrain();
        let exti = dp.EXTI;

        // Note, we can't use separate functions, because we are consuming (as
        // in taking ownership of) the device peripherals struct, and so we
        // cannot pass it as arguments to a function, as it would only be
        // partially valid.

        //--------------------------
        //  Clock configuration
        //--------------------------

        // Can't use the HAL. The GD32 is too different.
        let clocks = clock::setup_clock_120m_hxtal(dp.RCC);
        let mut delay = Delay::new(cp.SYST, clocks);

        //--------------------------
        //  External flash
        //--------------------------

        let ext_flash = ExtFlash::new(
            gpiob.pb12, gpiob.pb13, gpiob.pb14, gpiob.pb15,
            dp.SPI2,
            &clocks, &mut gpiob.crh
        );

        //--------------------------
        //  TFT display
        //--------------------------

        //let _notsure = gpioa.pa6.into_push_pull_output(&mut gpioa.crl);
        let mut display = Display::new(
            gpioc.pc6, gpioa.pa10,
            gpiod.pd4, gpiod.pd5, gpiod.pd7, gpiod.pd11,
            gpiod.pd14, gpiod.pd15, gpiod.pd0, gpiod.pd1, gpioe.pe7, gpioe.pe8,
            gpioe.pe9, gpioe.pe10, gpioe.pe11, gpioe.pe12, gpioe.pe13,
            gpioe.pe14, gpioe.pe15, gpiod.pd8, gpiod.pd9, gpiod.pd10,
            dp.FSMC,
            &mut gpioa.crh, &mut gpioc.crl, &mut gpiod.crl, &mut gpiod.crh, &mut gpioe.crl, &mut gpioe.crh,
        );
        display.init(&mut delay);

        //--------------------------
        //  Touch screen
        //--------------------------

        let touch_screen = TouchScreen::new(
            gpioc.pc7, gpioc.pc8, gpioc.pc9, gpioa.pa8, gpioa.pa9,
            &mut gpioa.crh, &mut gpioc.crl, &mut gpioc.crh, &mut afio, &exti,
        );

        //--------------------------
        //  Stepper motor (Z-axis)
        //--------------------------

        let (pa15, pb3, pb4) = afio.mapr.disable_jtag(gpioa.pa15, gpiob.pb3, gpiob.pb4);

        let zsensor = ZSensor::new(
            pb3,
            // pb4,
            &mut gpiob.crl,
        );

        let stepper = Stepper::new(
            gpioe.pe4, gpioe.pe5, gpioe.pe6,
            gpioc.pc3, gpioc.pc0,
            gpioc.pc1, gpioc.pc2,
            gpioa.pa3,
            Timer::new(dp.TIM2, &clocks), Timer::new(dp.TIM7, &clocks),
            &mut gpioa.crl, &mut gpioc.crl, &mut gpioe.crl, &mut afio.mapr,
        );

        //--------------------------
        // LCD Panel
        //--------------------------
        let lcd = Lcd::new(
            gpiod.pd12,
            gpioa.pa4, gpioa.pa5, gpioa.pa6, gpioa.pa7,
            dp.SPI1,
            &clocks, &mut gpioa.crl, &mut gpiod.crh, &mut afio.mapr,
        );

        //--------------------------
        // Systicks for RTIC
        //--------------------------

        let syst = delay.free();
        let systick = Systick::new(syst, clocks.sysclk().0);

        Self { ext_flash, display, touch_screen, stepper, lcd, zsensor, systick }
    }
}
