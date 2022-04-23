use embassy_stm32::{
    gpio::{Level, Input, Output, Speed, Pull, Pin},
    time::Hertz,
};
use num::{Integer, PrimInt};

use crate::consts::system::CLOCK_SPEED_MHZ;

use crate::drivers::delay_ns_compensated;

pub struct Spi<Clk: Pin, Mosi: Pin, Miso: Pin, const SPI_FREQ_HZ: u32> {
    pub clk: Output<'static, Clk>,
    pub mosi: Output<'static, Mosi>,
    pub miso: Input<'static, Miso>,
}

impl<Clk: Pin, Mosi: Pin, Miso: Pin, const SPI_FREQ_HZ: u32> Spi<Clk, Mosi, Miso, SPI_FREQ_HZ> {
    // the *2 is because two clock edges per period. The clock rises and falls for each data bit.
    const CLOCK_EDGE_TO_EDGE_DURATION_NS: u32 = 1_000_000_000 / (SPI_FREQ_HZ*2);
    // This is how many instructions we execute without any delay, between two clock cycles.
    // That puts us at a maximum bps of system.clock / 10.
    const NUM_INSTRUCTIONS_BETWEEN_CLOCK_EDGES: u32 = 5;

    pub fn new(clk: Output<'static, Clk>, mosi: Output<'static, Mosi>, miso: Input<'static, Miso>) -> Self {
        Self { clk, mosi, miso }
    }

    pub fn xfer_bytes<T: PrimInt>(&mut self, buf: &mut [T]) {
        for v in buf {
            *v = self.xfer(*v);
        }
    }

    pub fn send_bytes<T: PrimInt>(&mut self, buf: &[T]) {
        for v in buf {
            self.xfer(*v);
        }
    }

    #[inline]
    fn clk_edge_delay() {
        delay_ns_compensated(
            Self::CLOCK_EDGE_TO_EDGE_DURATION_NS,
            Self::NUM_INSTRUCTIONS_BETWEEN_CLOCK_EDGES
        );
    }

    // no inline because we want to keep the timing consistant when throwing the rx value away
    #[inline(never)]
    pub fn xfer<T: PrimInt>(&mut self, mut tx: T) -> T {
        let mut rx = T::zero();

        // Silly way to get the number of bits.
        // T::BITS would be nicer.
        let bits = T::max_value().count_ones();

        for _ in 0..bits {
            Self::clk_edge_delay();
            self.clk.set_low();

            // MSB first
            tx = tx.rotate_left(1);
            if (tx & T::one()).is_zero() {
                self.mosi.set_low();
            } else {
                self.mosi.set_high();
            }

            Self::clk_edge_delay();
            self.clk.set_high();

            rx = rx << 1;
            if self.miso.is_high() {
                rx = rx | T::one();
            }
        }

        rx
    }

    pub fn free(self) -> (
        Output<'static, Clk>,
        Output<'static, Mosi>,
        Input<'static, Miso>,
    ) {
        (self.clk, self.mosi, self.miso)
    }
}
