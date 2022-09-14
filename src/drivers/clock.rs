use embassy_stm32::{Config, time::Hertz};

pub fn init() -> embassy_stm32::Peripherals {
    #[allow(unused_mut)]
    let mut config = Config::default();
    #[cfg(feature="gd32f307ve")]
    {
        // We are doing the clock init here because of the gigadevice differences.
        let clk = crate::drivers::gd32f307_clock::setup_clock_120m_hxtal();
        unsafe { embassy_stm32::rcc::set_freqs(clk) };
    }

    #[cfg(feature="stm32f407ze")]
    {
        config.rcc.hse = Some(Hertz::mhz(20));
        config.rcc.sys_ck = Some(Hertz::mhz(168).into());
        // apb1 max speed is 42 mhz
        // apb2 max speed is 84 mhz
        config.rcc.pll48 = true;
    }
    // Note: TIM3 is taken for time accounting. It's configurable in Cargo.toml
    embassy_stm32::init(config)
}
