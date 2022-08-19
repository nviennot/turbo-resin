use embassy_util::Forever;
use cortex_m::peripheral::DWT;

use crate::consts::system::*;

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
    debug!("Cycles: {}, {}ms", total_cycles, total_cycles/(CLOCK_SPEED_MHZ*1000));
    ret
}
