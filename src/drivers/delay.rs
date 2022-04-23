// SPDX-License-Identifier: GPL-3.0-or-later

use crate::consts::system::CLOCK_SPEED_MHZ;

#[inline(always)]
pub fn delay_cycles(cycles: u32) {
    // The official crate overshoots on Cortex-M4
    let cycles = (cycles*2)/3;
    cortex_m::asm::delay(cycles);
}

#[inline(always)]
pub fn delay_ns_compensated(duration_ns: u32, cycles_to_skip: u32) {
    let cycles = (duration_ns * CLOCK_SPEED_MHZ) / 1000;
    let cycles = cycles.saturating_sub(cycles_to_skip);
    if cycles > 0 {
        delay_cycles(cycles);
    }
}

#[inline(always)]
pub fn delay_ns(duration_ns: u32) {
    delay_ns_compensated(duration_ns, 0)
}

#[inline(always)]
pub fn delay_us(duration_us: u32) {
    let cycles = duration_us * CLOCK_SPEED_MHZ;
    if cycles > 0 {
        delay_cycles(cycles);
    }
}

#[inline(always)]
pub fn delay_ms(duration_ms: u32) {
    delay_us(duration_ms*1000)
}
