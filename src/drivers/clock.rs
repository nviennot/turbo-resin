// SPDX-License-Identifier: GPL-3.0-or-later

use cortex_m::peripheral::DWT;
use stm32f1xx_hal::{
    time::Hertz,
    rcc::Clocks,
    prelude::*,
};

use gd32f3::gd32f307::{
    PMU,
    RCU,
};

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

    get_120mhz_clocks_config()
}

pub fn get_120mhz_clocks_config() -> Clocks {
    // The stm32 create has the Clocks struct, but all the fields are private.
    // We are left doing this hack with transmuting memory.
    // Not the safest thing, but good enough for now.
    struct ClocksDup {
        _hclk: Hertz,
        _pclk1: Hertz,
        _pclk2: Hertz,
        _ppre1: u8,
        _ppre2: u8,
        _sysclk: Hertz,
        _adcclk: Hertz,
        _usbclk_valid: bool,
    }

    let clocks = ClocksDup {
        _hclk: 120.mhz().into(),
        _pclk1: 60.mhz().into(),
        _pclk2: 120.mhz().into(),

        _ppre1: 0, // TODO Not sure what's that is used for
        _ppre2: 0, // TODO Not sure what's that is used for

        _sysclk: 120.mhz().into(),
        _adcclk: 30.mhz().into(),

        _usbclk_valid: true,
    };

    unsafe { core::mem::transmute(clocks) }
}

pub fn embassy_stm32_clock_from(clk: &Clocks) -> embassy_stm32::rcc::Clocks {
    use embassy_stm32::time::Hertz;
    embassy_stm32::rcc::Clocks {
        sys: Hertz(clk.sysclk().0),
        apb1: Hertz(clk.pclk1().0),
        apb2: Hertz(clk.pclk2().0),
        apb1_tim: Hertz(clk.pclk1_tim().0),
        apb2_tim: Hertz(clk.pclk2_tim().0),
        ahb1: Hertz(clk.hclk().0),
        adc: Hertz(clk.adcclk().0),
    }
}


// 3 clock cycles is 25ns at 120MHz
#[inline(always)]
pub fn delay_ns(duration_ns: u32) {
    let cycles = (3 * duration_ns) / 25;
    cortex_m::asm::delay(cycles);
}

#[inline(always)]
pub fn delay_us(duration_us: u32) {
    delay_ns(duration_us*1000)
}

#[inline(always)]
pub fn delay_ms(duration_ms: u32) {
    delay_us(duration_ms*1000)
}



use embassy::util::Forever;

static CYCLE_COUNTER: Forever<CycleCounter> = Forever::new();

pub struct CycleCounter {
   dwt: DWT,
}

impl CycleCounter {
    pub fn new(mut dwt: DWT) -> Self {
        //DWT::unlock();
        dwt.enable_cycle_counter();
        Self { dwt }
    }

    pub fn cycles(&self) -> u32 {
        self.dwt.cyccnt.read()
    }

    pub fn into_global(self) {
        CYCLE_COUNTER.put(self);
    }
}

#[inline(always)]
pub fn read_cycles() -> u32 {
    unsafe { CYCLE_COUNTER.steal().cycles() }
}


pub fn count_cycles<R>(mut f: impl FnMut() -> R) -> R {
    let start_cycles = read_cycles();
    let ret = f();
    let end_cycles = read_cycles();
    let total_cycles = end_cycles.wrapping_sub(start_cycles);
    crate::debug!("Cycles: {}", total_cycles);
    ret
}
