// SPDX-License-Identifier: GPL-3.0-or-later

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
