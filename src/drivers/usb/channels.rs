// SPDX-License-Identifier: GPL-3.0-or-later

use core::{marker::PhantomData, mem::{self, MaybeUninit}, cell::UnsafeCell};

use stm32f1xx_hal::{
    gpio::*,
    gpio::gpioa::*,
    rcc::Clocks,
    prelude::*,
    rcc::{Enable, Reset},
    time::Hertz,
    pac::{self, usb_otg_host::{FS_HCCHAR0, FS_HCINT0, FS_HCINTMSK0, FS_HCTSIZ0}},
};

use embassy::{
    channel::signal::Signal,
};

use super::UsbResult;

use crate::{drivers::{clock::delay_ms, usb::ensure}, debug};

const OTG_CORE_BASE_ADDR: *mut u8 = 0x5000_0000u32 as *mut u8;

const NUM_CHANNELS: usize = 8;
const NUM_XFER_ATTEMPTS: u8 = 3;

struct ChannelInterruptContext {
    event_signal: Signal<ChannelEvent>,
    buf: Option<&'static mut [MaybeUninit<u8>]>,
}

impl Default for ChannelInterruptContext {
    fn default() -> Self {
        Self {
            event_signal: Signal::new(),
            buf: None,
        }
    }
}

static mut INTERRUPT_CONTEXTS: [MaybeUninit<ChannelInterruptContext>; NUM_CHANNELS] = MaybeUninit::uninit_array();

#[inline(always)]
fn otg_global() -> &'static pac::otg_fs_global::RegisterBlock {
    unsafe { &(*pac::OTG_FS_GLOBAL::ptr()) }
}

#[inline(always)]
fn otg_host() -> &'static pac::usb_otg_host::RegisterBlock {
    unsafe { &(*pac::USB_OTG_HOST::ptr()) }
}

pub struct Channel {
    ch_index: u8,
}

impl Channel {
    unsafe fn steal(ch_index: u8) -> Self {
        Self { ch_index }
    }

    fn get_registers(&self) -> (&FS_HCCHAR0, &FS_HCINT0, &FS_HCINTMSK0, &FS_HCTSIZ0) {
        unsafe {
            let offset = 8*(self.ch_index as usize);
            (
                &*((&otg_host().fs_hcchar0) as *const FS_HCCHAR0).add(offset),
                &*((&otg_host().fs_hcint0) as *const FS_HCINT0).add(offset),
                &*((&otg_host().fs_hcintmsk0) as *const FS_HCINTMSK0).add(offset),
                &*((&otg_host().fs_hctsiz0) as *const FS_HCTSIZ0).add(offset),
            )
        }
    }

    fn get_fifo_ptr(&self) -> *mut u32 {
        const FIFO_SIZE: usize = 0x1000;
        const FIFO_START: usize = 0x1000;
        let offset = FIFO_START + (self.ch_index as usize)*FIFO_SIZE;
        unsafe { OTG_CORE_BASE_ADDR.add(offset) as *mut u32 }
    }

    #[inline(always)]
    pub fn new(ch_index: u8, dev_addr: u8, ep_dir: Direction, ep_index: u8, ep_type: EndpointType, max_packet_size: u16) -> Self {
        let mut c = unsafe { Self::steal(ch_index) };
        c.init(dev_addr, ep_dir, ep_index, ep_type, max_packet_size);
        c
    }

    #[inline(always)]
    fn init(&mut self, dev_addr: u8, ep_dir: Direction, ep_index: u8, ep_type: EndpointType, max_packet_size: u16) {
        let low_speed = false;
        *self.interrupt_context() = ChannelInterruptContext::default();
        unsafe {
            let (hcchar, hcint, hcintmsk, hctsiz) = self.get_registers();

            // Clear old interrupts
            hcint.write(|w| w.bits(0xFFFFFFFF));

            if ep_type == EndpointType::Isoc {
                hcintmsk.write(|w| w
                    .xfrcm().set_bit()
                    .ackm().set_bit()
                    .frmorm().set_bit()
                    .txerrm().bit(ep_dir == Direction::In)
                    .bberrm().bit(ep_dir == Direction::In)
                );
            } else {
                hcintmsk.write(|w| w
                    // Transfer complete
                    .xfrcm().set_bit()
                    // Stall response
                    .stallm().set_bit()
                    // Transaction error
                    .txerrm().set_bit()
                    // Data toggle error
                    .dterrm().set_bit()
                    // NAK response
                    .nakm().set_bit()
                    // Frame overrun
                    .frmorm().bit(ep_type == EndpointType::Intr)
                    // Babble error
                    // No need to unmask register, txerr will fire.
                    // .bberrm().bit(ep_dir == Direction::In)
                );
            }

            // Enable the top level host channel interrupt
            otg_host().haintmsk.modify(|r,w| w
                .haintm().bits(r.haintm().bits() | 1 << self.ch_index)
            );

            hcchar.write(|w| w
                .dad().bits(dev_addr)
                .epnum().bits(ep_index)
                .eptyp().bits(ep_type as u8)
                .mpsiz().bits(max_packet_size)
                .epdir().bit(ep_dir == Direction::In)
                .oddfrm().bit(ep_type == EndpointType::Intr)
                .lsdev().bit(low_speed)
            );
        }
    }

    pub fn disable_all() {
        unsafe {
            for i in 0..NUM_CHANNELS {
                Channel::steal(i as u8)
                    .get_registers().0.modify(|_,w| w
                        .chdis().set_bit());
            }
        }
    }

    #[inline(always)]
    fn interrupt_context(&self) -> &'static mut ChannelInterruptContext {
        unsafe { INTERRUPT_CONTEXTS[self.ch_index as usize].assume_init_mut() }
    }

    fn signal_event(&self, event: ChannelEvent) {
        self.interrupt_context().event_signal.signal(event);
    }

    pub fn on_host_ch_interrupt() {
        let mut haint = otg_host().haint.read().haint().bits();
        while haint != 0 {
            let ch_index = haint.trailing_zeros() as u8;
            // Stealing is okay, the channel has been initialized as we are receiving interrupts.
            unsafe { Channel::steal(ch_index) }.on_ch_interrupt();
            haint &= !(1 << ch_index);
        }
    }

    fn on_ch_interrupt(&mut self) {
        let (hcchar, hcint_reg, hcintmsk, hctsiz) = self.get_registers();

        let hcint = hcint_reg.read();
        unsafe { hcint_reg.write(|w| w.bits(hcint.bits())) };

        if hcint.xfrc().bit_is_set() {
            //debug!("  Transfer complete");
            self.signal_event(ChannelEvent::Complete);
        }

        if hcint.stall().bit_is_set() {
            debug!("  Stall response");
            self.signal_event(ChannelEvent::FatalError);
        }

        if hcint.txerr().bit_is_set() {
            debug!("  Transaction error");
            self.signal_event(ChannelEvent::RetriableError);
        }

        if hcint.dterr().bit_is_set() {
            debug!("  Data toggle error");
            self.signal_event(ChannelEvent::RetriableError);
        }

        if hcint.nak().bit_is_set() {
            //debug!("  NAK");
            self.signal_event(ChannelEvent::Retry);
        }

        if hcint.frmor().bit_is_set() {
            debug!("  Frame overrun");
        }

        // During inputs
        if hcint.bberr().bit_is_set() {
            // transaction error will be set
            debug!("  Babble error");
        }

        if hcint.ack().bit_is_set() {
        }
    }

    pub fn on_host_rx_interrupt() {
        unsafe {
            // There's two gsrxsts registers. One read-only and one read-and-pop.
            // We want the read-and-pop, and it's located right after the read one.
            // Sadly, it's not defined in the register definitions.
            // We'll do some pointer arithmetic.
            let grxstsp = otg_global().fs_grxstsr_host().as_ptr().add(1);
            let grxstsp = &*(grxstsp as *const pac::otg_fs_global::FS_GRXSTSR_HOST);
            let rx_status = grxstsp.read();

            let ch_index = rx_status.epnum().bits();
            Channel::steal(ch_index).on_rx_interrupt(rx_status);
        }
    }

    fn on_rx_interrupt(&mut self, rx_status: pac::otg_fs_global::fs_grxstsr_host::R) {
        let ctx = self.interrupt_context();

        /*
        debug!("on RX fifo, ch:{}, framenum: {}, bcnt: {}, dpid: {}, pktsts: {}",
            rx_status.epnum().bits(),
            rx_status.frmnum().bits(), rx_status.bcnt().bits(),
            rx_status.dpid().bits(), rx_status.pktsts().bits());
            */

        match rx_status.pktsts().bits() {
            // IN data packet received
            0b0010 => {
                let size = rx_status.bcnt().bits() as usize;
                if size > 0 {
                    let buf = ctx.buf.take().unwrap();
                    // We don't want to read more bytes that we are supposed to.
                    let to_read = size.min(buf.len());
                    self.read_from_fifo(&mut buf[0..to_read]);
                    ctx.buf = Some(&mut buf[to_read..]);

                    // There's data leftover, we don't want it. Flush it away.
                    let left_over = size - to_read;
                    if left_over > 0 {
                        for _ in 0..(left_over+3)/4 {
                            unsafe { self.get_fifo_ptr().read_volatile() };
                        }
                    }
                }
            }
            // IN transfer completed (triggers an interrupt)
            0b0011 => {
                self.signal_event(ChannelEvent::Complete);
            }
            // Data toggle error (triggers an interrupt)
            0b0101 => {
                self.signal_event(ChannelEvent::RetriableError);
            }
            // Channel halted (triggers an interrupt)
            0b0111 => {
                self.signal_event(ChannelEvent::FatalError);
            }
            v @ _ => {
                panic!("Unknown packet status: {:x}", v);
            }
        }
    }

    pub async fn write<T>(&mut self, packet_type: PacketType, src: &T) -> UsbResult<()> {
        let src = unsafe {
            core::slice::from_raw_parts(
                (src as *const T) as *const u8,
                mem::size_of::<T>(),
            )
        };
        self.write_bytes(packet_type, src).await
    }

    pub async fn write_bytes(&mut self, packet_type: PacketType, src: &[u8]) -> UsbResult<()> {
        let ctx = self.interrupt_context();
        let mut num_attempts_left = NUM_XFER_ATTEMPTS;

        // Ensure the channel is not being used
        assert!(self.get_registers().0.read().chena().bit_is_clear());

        loop {
            self.prepare_channel_xfer(packet_type, src.len(), Direction::Out);

            // This is the space left in the TX FIFO:
            // otg_global().fs_gnptxsts.read().nptxfsav()
            // otg_global().fs_gintmsk.modify(|_,w| w.nptxfem() )
            // TODO configure interrupts if size of fifo is too small for the content to send.

            self.write_to_fifo(src);

            match ctx.event_signal.wait().await {
                ChannelEvent::Complete => return Ok(()),
                ChannelEvent::FatalError => return Err(()),
                ChannelEvent::Retry => {
                    num_attempts_left = NUM_XFER_ATTEMPTS;
                }
                ChannelEvent::RetriableError => {
                    num_attempts_left -= 1;
                    ensure!(num_attempts_left > 0);
                }
            }
        }
    }

    #[inline(never)]
    fn write_to_fifo(&mut self, mut src: &[u8]) {
        let dst = self.get_fifo_ptr();

        while !src.is_empty() {
            unsafe {
                let v = (src.as_ptr() as *const u32).read_unaligned();
                dst.write_volatile(v);
            }
            src = &src[mem::size_of::<u32>()..];
        }
    }

    pub async fn read<T>(&mut self, packet_type: PacketType) -> UsbResult<T> {
        let mut result = MaybeUninit::<T>::uninit();
        let dst = result.as_bytes_mut();

        self.read_bytes(packet_type, dst).await?;
        Ok(unsafe { result.assume_init() })
    }

    pub async fn read_bytes(&mut self, packet_type: PacketType, dst: &mut [MaybeUninit<u8>]) -> UsbResult<()> {
        let ctx = self.interrupt_context();
        let mut num_attempts_left = NUM_XFER_ATTEMPTS;

        loop {
            // transmute to because ctx.buf has a static lifetime.
            // It's a lie, but we'll cleanup the reference right after.
            ctx.buf = Some(unsafe { core::mem::transmute(&mut dst[..]) });
            self.prepare_channel_xfer(packet_type, dst.len(), Direction::In);
            let signal = ctx.event_signal.wait().await;
            let buffer_fully_filled = ctx.buf.take().unwrap().len() == 0;

            match signal {
                ChannelEvent::Complete if buffer_fully_filled => return Ok(()),
                ChannelEvent::FatalError => return Err(()),
                ChannelEvent::Retry => {
                    num_attempts_left = NUM_XFER_ATTEMPTS;
                }
                ChannelEvent::Complete | ChannelEvent::RetriableError => {
                    num_attempts_left -= 1;
                    ensure!(num_attempts_left > 0);
                }
            }
        }
    }

    fn read_from_fifo(&mut self, mut dst: &mut [MaybeUninit<u8>]) {
        let src = self.get_fifo_ptr();

        while !dst.is_empty() {
            unsafe {
                let v = src.read_volatile();

                if dst.len() <= 3 {
                    if dst.len() >= 1 { *dst[0].as_mut_ptr() = (v      ) as u8 }
                    if dst.len() >= 2 { *dst[1].as_mut_ptr() = (v >>  8) as u8 }
                    if dst.len() >= 3 { *dst[2].as_mut_ptr() = (v >> 16) as u8 }
                    return;
                } else {
                    (dst.as_mut_ptr() as *mut u32).write_unaligned(v);
                }
            }
            dst = &mut dst[mem::size_of::<u32>()..];
        }
    }

    fn prepare_channel_xfer(&mut self, packet_type: PacketType, size: usize, dir: Direction) {
        unsafe {
            let (hcchar, hcint, hcintmsk, hctsiz) = self.get_registers();

            // Ensure the channel is not being used
            assert!(hcchar.read().chena().bit_is_clear());
            self.interrupt_context().event_signal.reset();

            // 1023 because the pktcnt is 10 bits in the hcchar register
            const MAX_PACKET_COUNT: usize = 1023;

            let max_packet_size = hcchar.read().mpsiz().bits() as usize;
            let pkt_cnt = div_round_up(size, max_packet_size).clamp(1, MAX_PACKET_COUNT);

            // Apparently, odd frames are used for periodic endpoints,
            // but the STM32 SDK does it for all endpoints.
            let oddfrm = otg_host().fs_hfnum.read().frnum().bits() & 1 == 1;

            // Per the documentation, in receive mode, we must configure a
            // multiple of the max_packet_size. That's kinda sucky because we
            // are going to have to deal with devices that could send more bytes
            // than we want.
            let size = if dir == Direction::In {
                pkt_cnt * max_packet_size
            } else {
                size
            };

            /*
            delay_ms(50);
            debug!("Prepare XFER ch: {}, dir: {:?}, pid: {:?}, pktcnt: {}, size: {}",
              self.ch_index, dir, packet_type, pkt_cnt, size);
              */

            hctsiz.write(|w| w
                .dpid().bits(packet_type as u8)
                .pktcnt().bits(pkt_cnt as u16)
                .xfrsiz().bits(size as u32)
            );
            hcchar.modify(|_,w| w
                .oddfrm().bit(oddfrm)
                .chdis().clear_bit()
                .chena().set_bit()
            );
        }
    }
}

#[inline(always)]
fn div_round_up(v: usize, denom: usize) -> usize {
    (v + denom - 1)/denom
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndpointType {
    Control = 0,
    Isoc = 1,
    Bulk = 2,
    Intr = 3,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PacketType {
    Data0 = 0,
    Data2 = 1,
    Data1 = 2,
    Setup = 3,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Direction {
    In,
    Out,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChannelEvent {
    // Transfer complete
    Complete,
    // Retry this request indefinitely
    Retry,
    // Some error happened, but we should retry the transmission
    RetriableError,
    // No retry on this one
    FatalError,
}
