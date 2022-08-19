// SPDX-License-Identifier: GPL-3.0-or-later

use embassy_stm32::rcc::Clocks;
use embassy_stm32::time::Hertz;
use gd32f3::gd32f307::{PMU, RCU};

pub fn setup_clock_120m_hxtal() -> Clocks {
    // Transcribed from the GD32F30x Firmware Library

    unsafe {
        let rcu = &*RCU::ptr();
        let pmu = &*PMU::ptr();

        rcu.ctl.modify(|_,w| w.hxtalen().set_bit());
        while rcu.ctl.read().hxtalstb().bit_is_clear() {}

        rcu.apb1en.modify(|_,w| w.pmuen().set_bit());
        pmu.ctl.modify(|_,w| w.ldovs().bits(0b11)); // LDOVS

        /* HXTAL is stable */
        /* AHB = SYSCLK */

        rcu.cfg0.modify(|_,w| w
            .ahbpsc().bits(0) // DIV1
            /* APB2 = AHB/1 */
            .apb2psc().bits(0) // DIV1
            /* APB1 = AHB/2 */
            .apb1psc().bits(4) // DIV2
            /* CK_PLL = (CK_PREDIV0) * 30 = 120 MHz */
            .pllmf_3_0().bits(13)
            .pllmf_5_4().bits(1)
            .pllsel().set_bit()
        );

        // Note: This is so convoluted.
        /* CK_PREDIV0 = (CK_HXTAL)/2 *8 /8 = 4 MHz */
        rcu.cfg1.modify(|_,w| w
            // select HXTAL
            .pllpresel().clear_bit()
            .predv0sel().set_bit()
            .pll1mf().bits(6) //PLL1 x8
            .predv1().bits(1) // The example code is wrong (they have 5)
            .predv0().bits(7)        // The example code is wrong (they have 10)
        );

        /* enable PLL1 */
        rcu.ctl.modify(|_,w| w.pll1en().set_bit());
        /* wait till PLL1 is ready */
        while rcu.ctl.read().pll1stb().bit_is_clear() {}

        /* enable PLL */
        rcu.ctl.modify(|_,w| w.pllen().set_bit());
        /* wait till PLL is ready */
        while rcu.ctl.read().pllstb().bit_is_clear() {}

        /* enable the high-drive to extend the clock frequency to 120 MHz */
        pmu.ctl.modify(|_,w| w.hden().set_bit());
        while pmu.cs.read().hdrf().bit_is_clear() {}

        /* select the high-drive mode */
        pmu.ctl.modify(|_,w| w.hds().set_bit());
        while pmu.cs.read().hdsrf().bit_is_clear() {}

        /* select PLL as system clock */
        rcu.cfg0.modify(|_,w| w.scs().bits(2));
        /* wait until PLL is selected as system clock */
        while rcu.cfg0.read().scss().bits() != 2 {}

        rcu.cfg0.modify(|_,w| w
            // Setup the USB prescaler at x2.5 (120mhz / 2.5 = 48Mhz)
            .usbfspsc_1_0().bits(0b10)
            .usbfspsc().clear_bit()
            // Setup ADC clock prescaler
            .adcpsc_1_0().bits(1) // APB2/4 clock
            .adcpsc_2().clear_bit()
        );
    }

    Clocks {
        sys: Hertz::mhz(120),
        ahb1: Hertz::mhz(120),
        apb1: Hertz::mhz(60),
        apb2: Hertz::mhz(120),
        apb1_tim: Hertz::mhz(120),
        apb2_tim: Hertz::mhz(120),
        adc: Hertz::mhz(30),
    }
}
