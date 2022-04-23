// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::{
    ext_flash::ExtFlash,
    display::Display,
    touch_screen::TouchScreen,
    zaxis,
    lcd::{Lcd, LcdFpga},
    CycleCounter,
    touch_screen::*,
    usb::UsbHost, delay_ms,
};

pub struct Machine {
    pub ext_flash: ExtFlash,
    pub display: Display,
    pub touch_screen: TouchScreen,
    pub lcd: Lcd,
    //pub stepper: zaxis::MotionControl,
    //pub z_bottom_sensor: zaxis::BottomSensor,
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

        let ext_flash = ExtFlash::new(
            p.PG15, p.PB3, p.PB4, p.PB5, p.SPI3, p.DMA1_CH2, p.DMA1_CH5
        ).expect("Failed to initialize the external spi flash");


        /*
            use embassy_stm32::gpio::{Level, Input, Output, Speed, Pull};
            core::mem::forget(Output::new(p.PA0 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PA4 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PA5 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PA6 ,  Level::Low, Speed::Low));
            //core::mem::forget(Output::new(p.PA15,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PB1 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PB7 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PB8 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PB9 ,  Level::High, Speed::Low));
            //core::mem::forget(Output::new(p.PB12,  Level::High, Speed::Low));
            //core::mem::forget(Output::new(p.PC7 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PD3 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PD6 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PD7 ,  Level::High, Speed::Low));
            //core::mem::forget(Output::new(p.PD11,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PD12,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PD13,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PE0 ,  Level::High, Speed::Low));
            //core::mem::forget(Output::new(p.PF8 ,  Level::Low, Speed::Low));
            //core::mem::forget(Output::new(p.PF9 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PF10,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PF13,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PF14,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PF15,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PG0 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PG1 ,  Level::Low, Speed::Low));
            //core::mem::forget(Output::new(p.PG3 ,  Level::Low, Speed::Low));
            //core::mem::forget(Output::new(p.PG4 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PG5 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PG7 ,  Level::Low, Speed::Low));
            //core::mem::forget(Output::new(p.PG8 ,  Level::High, Speed::Low));
            core::mem::forget(Output::new(p.PG9 ,  Level::Low, Speed::Low));
            core::mem::forget(Output::new(p.PG10,  Level::Low, Speed::Low));
            //core::mem::forget(Output::new(p.PG15,  Level::High, Speed::Low));
            */


        //--------------------------
        //  TFT display
        //--------------------------

        //let _notsure = gpioa.pa6.into_push_pull_output(&mut gpioa.crl);
        let mut display = Display::new(
            p.PB12, p.PG8,
            p.PD4, p.PD5, p.PG12, p.PG2,
            p.PD14, p.PD15, p.PD0, p.PD1, p.PE7, p.PE8,
            p.PE9, p.PE10, p.PE11, p.PE12, p.PE13,
            p.PE14, p.PE15, p.PD8, p.PD9, p.PD10,
            p.FSMC,
        );
        display.init();
        display.backlight.set_high();

        //--------------------------
        //  Touch screen
        //--------------------------
        let touch_screen = TouchScreen::new(
            ADS7846::new(p.PD11, p.PB13, p.PB14, p.PB15, p.SPI2, p.DMA1_CH3, p.DMA1_CH4)
        );

        //--------------------------
        // LCD Panel
        //--------------------------
        let lcd_fpga = LcdFpga::new(p.PF9, p.PF8, p.PG4, p.PE2, p.PE5);
        //lcd_fpga.upload_bitstream(&mut ext_flash);
        let lcd = Lcd::new(p.PA15, p.PC7, p.PC6, p.PG3);

        //--------------------------
        // USB Host
        //--------------------------
        let usb_host = UsbHost::new(p.PA11, p.PA12, p.USB_OTG_FS);

        /*
        //--------------------------
        //  Stepper motor (Z-axis)
        //--------------------------

        // Disable JTAG to activate pa15, pb3, and pb4 as regular GPIO.
        #[cfg(feature="gd32f307ve")]
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
        */

        Self { display , touch_screen, lcd, ext_flash, usb_host, /*stepper, z_bottom_sensor*/ }
    }
}
