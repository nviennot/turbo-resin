// SPDX-License-Identifier: GPL-3.0-or-later


use stm32f1xx_hal as _;
use stm32f1xx_hal::timer::Timer;

use crate::drivers::{
    ext_flash::ExtFlash,
    display::Display,
    touch_screen::TouchScreen,
    zaxis,
    lcd::Lcd,
    clock,
    touch_screen::*,
    usb::UsbHost,
};

pub struct Machine {
    pub ext_flash: ExtFlash,
    pub display: Display,
    pub touch_screen: TouchScreen,
    pub stepper: zaxis::MotionControl,
    pub lcd: Lcd,
    pub z_bottom_sensor: zaxis::BottomSensor,
    pub usb_host: UsbHost,
}

use embassy_stm32::Peripherals;
use stm32f1xx_hal::prelude::*;

impl Machine {
    pub fn new(cp: cortex_m::Peripherals, p: Peripherals) -> Self {
        // Okay, so what we are doing is really sad. Embassy doesn't have well
        // enough support for the things we need to do. For example running a
        // PWM on PA3 is not implemented.
        // So we are going to use both HALs. Embassy's and the usual one.

        let dp = unsafe { stm32f1xx_hal::pac::Peripherals::steal() };
        let mut gpioa = dp.GPIOA.split();
        let mut gpiob = dp.GPIOB.split();
        let mut gpioc = dp.GPIOC.split();
        let mut gpiod = dp.GPIOD.split();
        let mut gpioe = dp.GPIOE.split();

        let mut afio = dp.AFIO.constrain();
        let clocks = super::clock::get_120mhz_clocks_config();

        // Note, we can't use separate functions, because we are consuming (as
        // in taking ownership of) the device peripherals struct, and so we
        // cannot pass it as arguments to a function, as it would only be
        // partially valid.

        //--------------------------
        //  Clock configuration
        //--------------------------

        // Can't use the HAL. The GD32 is too different.
        //let clocks = clock::setup_clock_120m_hxtal();
        clock::CycleCounter::new(cp.DWT).into_global();

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
        display.init();

        //--------------------------
        //  Touch screen
        //--------------------------
        let touch_screen = TouchScreen::new(
            ADS7846::new(p.PC7, p.PC8, p.PC9, p.PA8, p.PA9, p.EXTI9)
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
        // USB Host
        //--------------------------
        gpioa.pa9.into_pull_up_input(&mut gpioa.crh);
        let usb_host = UsbHost::new(gpioa.pa11, gpioa.pa12,
            dp.OTG_FS_GLOBAL, dp.USB_OTG_HOST, dp.OTG_FS_PWRCLK, &mut gpioa.crh);

        //--------------------------
        //  Stepper motor (Z-axis)
        //--------------------------

        let (_pa15, pb3, _pb4) = afio.mapr.disable_jtag(gpioa.pa15, gpiob.pb3, gpiob.pb4);

        let z_bottom_sensor = zaxis::BottomSensor::new(
            pb3,
            // pb4,
            &mut gpiob.crl,
        );

        let drv8424 = zaxis::Drv8424::new(
            gpioe.pe4, gpioe.pe5, gpioe.pe6,
            gpioc.pc3, gpioc.pc0,
            gpioc.pc1, gpioc.pc2,
            gpioa.pa3,
            Timer::new(dp.TIM2, &clocks),
            &mut gpioa.crl, gpioc.crl, &mut gpioe.crl, &mut afio.mapr,
        );

        let stepper = zaxis::MotionControl::new(drv8424, Timer::new(dp.TIM7, &clocks));

        Self { ext_flash, display, touch_screen, stepper, lcd, z_bottom_sensor, usb_host }
    }
}
