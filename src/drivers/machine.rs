// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::{
    //ext_flash::ExtFlash,
    display::Display,
    touch_screen::TouchScreen,
    zaxis,
    lcd::Lcd,
    gd32f307_clock,
    CycleCounter,
    touch_screen::*,
    usb::UsbHost,
};

pub struct Machine {
    //pub ext_flash: ExtFlash,
    pub display: Display,
    pub touch_screen: TouchScreen,
    pub stepper: zaxis::MotionControl,
    pub lcd: Lcd,
    pub z_bottom_sensor: zaxis::BottomSensor,
    pub usb_host: UsbHost,
}

use embassy_stm32::{Peripherals, gpio::Input};

impl Machine {
    pub fn new(cp: cortex_m::Peripherals, p: Peripherals) -> Self {
        //--------------------------
        //  Clock configuration
        //--------------------------

        CycleCounter::new(cp.DWT).into_global();

        //--------------------------
        //  External flash
        //--------------------------

        /*
        let ext_flash = ExtFlash::new(
            gpiob.pb12, gpiob.pb13, gpiob.pb14, gpiob.pb15,
            dp.SPI2,
            &clocks, &mut gpiob.crh
        );
        */

        //--------------------------
        //  TFT display
        //--------------------------

        //let _notsure = gpioa.pa6.into_push_pull_output(&mut gpioa.crl);
        let mut display = Display::new(
            p.PC6, p.PA10,
            p.PD4, p.PD5, p.PD7, p.PD11,
            p.PD14, p.PD15, p.PD0, p.PD1, p.PE7, p.PE8,
            p.PE9, p.PE10, p.PE11, p.PE12, p.PE13,
            p.PE14, p.PE15, p.PD8, p.PD9, p.PD10,
            p.FSMC,
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
            p.PD12,
            p.PA4, p.PA5, p.PA6, p.PA7,
            p.SPI1, p.DMA1_CH2, p.DMA1_CH3,
        );

        //--------------------------
        // USB Host
        //--------------------------
        let usb_host = UsbHost::new(p.PA11, p.PA12, p.USB_OTG_FS);

        //--------------------------
        //  Stepper motor (Z-axis)
        //--------------------------

        // Disable JTAG to activate pa15, pb3, and pb4 as regular GPIO.
        unsafe {
            embassy_stm32::pac::AFIO.mapr().modify(|w|
                w.set_swj_cfg(0b010)
            );
        }

        let z_bottom_sensor = zaxis::BottomSensor::new(
            p.PB3,
            // pb4 is normally the top sensor
        );

        let drv8424 = zaxis::Drv8424::new(
            p.PE4, p.PE5, p.PE6, p.PC3, p.PC0, p.PC1, p.PC2,
            p.PA3, p.TIM2,
        );

        let stepper = zaxis::MotionControl::new(drv8424, p.TIM7);

        Self { display, touch_screen, stepper, lcd, z_bottom_sensor, usb_host }
    }
}
