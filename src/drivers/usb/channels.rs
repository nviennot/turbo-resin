// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem::MaybeUninit;

use embassy_stm32::pac::{
    otgfs::{regs, vals},
    common::{Reg, RW},
};

use crate::util::io::{Read, Write, impl_read_obj, impl_write_obj};
use core::future::Future;
use embassy::channel::signal::Signal;
use super::{REGS, UsbResult};

const NUM_CHANNELS: usize = 8;
 // There can be many NAKs. It's okay to do _that_ many retries, it's fast.
const NUM_XFER_ATTEMPTS: usize = 100_000;

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

pub struct Channel {
    ch_index: u8,
}

impl Channel {
    #[inline]
    unsafe fn steal(ch_index: u8) -> Self {
        Self { ch_index }
    }

    #[inline]
    pub fn hcchar(&self) -> Reg<regs::Hcchar, RW> {
        REGS.hcchar(self.ch_index as usize)
    }

    #[inline]
    pub fn hcint(&self) -> Reg<regs::Hcint, RW> {
        REGS.hcint(self.ch_index as usize)
    }

    #[inline]
    pub fn hcintmsk(&self) -> Reg<regs::Hcintmsk, RW> {
        REGS.hcintmsk(self.ch_index as usize)
    }

    #[inline]
    pub fn hctsiz(&self) -> Reg<regs::Hctsiz, RW> {
        REGS.hctsiz(self.ch_index as usize)
    }

    #[inline]
    pub fn get_fifo(&self) -> Reg<regs::Fifo, RW> {
        REGS.fifo(self.ch_index as usize)
    }

    #[inline(always)]
    pub fn new(ch_index: u8, dev_addr: u8, ep_dir: Direction, ep_number: u8, ep_type: EndpointType, max_packet_size: u16) -> Self {
        trace!("new channel: ch_index={}, dev_addr={}, ep_dir={:?}, ep_number={}, ep_type={:?}, mps={}",
                ch_index, dev_addr, ep_dir, ep_number, ep_type, max_packet_size);

        let mut c = unsafe { Self::steal(ch_index) };
        c.init(dev_addr, ep_dir, ep_number, ep_type, max_packet_size);
        c
    }

    #[inline(always)]
    fn init(&mut self, dev_addr: u8, ep_dir: Direction, ep_number: u8, ep_type: EndpointType, max_packet_size: u16) {
        // TODO low_speed: This is used when we talk to a low_speed through a high_speed hub.
        let low_speed = false;

        *self.interrupt_context() = Default::default();
        unsafe {

            // Clear interrupts
            self.hcint().write_value(regs::Hcint(0xFFFFFFFF));

            if ep_type == EndpointType::Isoc {
                self.hcintmsk().write(|w| {
                    w.set_xfrcm(true);
                    w.set_ackm(true);
                    w.set_frmorm(true);
                    w.set_txerrm(ep_dir == Direction::In);
                    w.set_bberrm(ep_dir == Direction::In);
                });
            } else {
                self.hcintmsk().write(|w| {
                    // Transfer complete
                    w.set_xfrcm(true);
                    // Stall response
                    w.set_stallm(true);
                    // Transaction error
                    w.set_txerrm(true);
                    // Data toggle error
                    w.set_dterrm(true);
                    // NAK response
                    w.set_nakm(true);
                    // Frame overrun
                    w.set_frmorm(ep_type == EndpointType::Intr);
                    // Babble error
                    // No need to unmask register, txerr will fire.
                    // .bberrm().bit(ep_dir == Direction::In)
                });
            }

            // Enable the top level host channel interrupt
            REGS.haintmsk().modify(|w|
                w.set_haintm(w.haintm() | 1 << self.ch_index)
            );

            self.hcchar().write(|w| {
                w.set_dad(dev_addr);
                w.set_epnum(ep_number);
                w.set_eptyp(ep_type as u8);
                w.set_mpsiz(max_packet_size);
                w.set_epdir(ep_dir == Direction::In);
                w.set_oddfrm(ep_type == EndpointType::Intr);
                w.set_lsdev(low_speed);
            });
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
        let mut haint = unsafe { REGS.haint().read().haint() };
        while haint != 0 {
            let ch_index = haint.trailing_zeros() as u8;
            // Stealing is okay, the channel has been initialized as we are receiving interrupts.
            unsafe { Channel::steal(ch_index) }.on_ch_interrupt();
            haint &= !(1 << ch_index);
        }
    }

    fn on_ch_interrupt(&mut self) {
        let hcint_reg = self.hcint();
        let hcint = unsafe { hcint_reg.read() };
        // Ack interrupts that we are seeing.
        unsafe { hcint_reg.write_value(hcint) };

        if hcint.xfrc() {
            trace!("  Transfer complete ch={}", self.ch_index);
            self.signal_event(ChannelEvent::Complete);
        }

        if hcint.nak() {
            //trace!("  NAK ch={}", self.ch_index);
            self.signal_event(ChannelEvent::RetryTransaction);
        }

        if hcint.txerr() {
            trace!("  Transaction error ch={}", self.ch_index);
            self.signal_event(ChannelEvent::RetryTransaction);
        }

        if hcint.stall() {
            trace!("  Stall response ch={}", self.ch_index);
            self.signal_event(ChannelEvent::FatalError);
        }

        if hcint.dterr() {
            trace!("  Data toggle error ch={}", self.ch_index);
            self.signal_event(ChannelEvent::FatalError);
        }

        if hcint.frmor() {
            trace!("  Frame overrrun ch={}", self.ch_index);
        }

        if hcint.bberr() {
            // transaction error flag will be set
            trace!("  Babble error ch={}", self.ch_index);
        }
    }

    pub fn on_host_rx_interrupt() {
        unsafe {
            let rx_status = REGS.grxstsp_host().read();
            let ch_index = rx_status.chnum();
            Channel::steal(ch_index).on_rx_interrupt(rx_status);
        }
    }

    fn on_rx_interrupt(&mut self, rx_status: regs::GrxstsHost) {
        match rx_status.pktsts() {
            vals::Pktstsh::IN_DATA_RX => {
                let len = rx_status.bcnt() as usize;
                trace!("ch={}, RX data received: len={}", self.ch_index, len);
                self.on_data_rx(len);
                // Nothing to signal.
                // A `IN_DATA_DONE` or `CHANNEL_HALTED` event will come next.
            }
            vals::Pktstsh::IN_DATA_DONE => {
                trace!("ch={}, RX done", self.ch_index);
                self.signal_event(ChannelEvent::Complete);
            }
            // Data toggle error
            vals::Pktstsh::DATA_TOGGLE_ERR => {
                trace!("ch={}, Data toggle error", self.ch_index);
                // This can happen if we miss a packet, it's best to retry.
                self.signal_event(ChannelEvent::RetryTransaction);
            }
            // Channel halted
            vals::Pktstsh::CHANNEL_HALTED => {
                trace!("ch={}, Channel halted", self.ch_index);
                self.signal_event(ChannelEvent::FatalError);
            }
            _ => unreachable!(),
        }
    }

    fn on_data_rx(&mut self, len: usize) {
        if len == 0 {
            return;
        }

        let ctx = self.interrupt_context();
        let mut buf = ctx.buf.take().unwrap();

        if len > buf.len() {
            // Extra data received. Oops.
            debug!("ch={}, Too many bytes received (len={}), shutting down channel", self.ch_index, len);
            // Flush the RX fifo
            for _ in 0..(len+3)/4 {
                unsafe { self.get_fifo().read() };
            }
            unsafe { self.hcchar().modify(|w| w.set_chena(false)); }
            // We don't `signal_event`. Either a signal `Complete` (and the
            // buffer length will be checked) or a `FatalError` will be sent
            // because we disabled the channel.
        } else {
            // Read and advance the buffer pointer.
            self.read_from_fifo(&mut buf[0..len]);
            buf = &mut buf[len..];

            // Re-enable channel if more data is to come.
            if !buf.is_empty() {
                unsafe { self.hcchar().modify(|w| w.set_chdis(false)); }
            }
        }
        ctx.buf = Some(buf);
    }

    pub fn on_host_disconnect_interrupt() {
        let mut enabled_channels = unsafe { REGS.haintmsk().read().haintm() };
        unsafe { REGS.haintmsk().write_value(regs::Haintmsk(0)) };

        while enabled_channels != 0 {
            let ch_index = enabled_channels.trailing_zeros() as u8;
            // Stealing is okay, the channel has been initialized as we are receiving interrupts.
            unsafe { Channel::steal(ch_index) }.stop_pending_xfer();
            enabled_channels &= !(1 << ch_index);
        }
    }

    fn stop_pending_xfer(&self) {
        unsafe {
            self.hcchar().modify(|w| {
                w.set_chdis(true);
                w.set_chena(false);
            });
        }

        self.signal_event(ChannelEvent::FatalError);
    }

    pub fn with_pid(&mut self, packet_type: PacketType) -> ChannelAccessor {
        ChannelAccessor {
            channel: self,
            packet_type: Some(packet_type),
        }
    }

    pub fn with_data_toggle(&mut self) -> ChannelAccessor {
        ChannelAccessor {
            channel: self,
            packet_type: None,
        }
    }

    pub async fn read(&mut self, packet_type: Option<PacketType>, buf: &mut [MaybeUninit<u8>]) -> UsbResult<()> {
        //debug!("read_bytes, size: {}", dst.len());
        let r = self.wait_for_completion(|self_| {
            let ctx = self_.interrupt_context();
            // transmute to because ctx.buf has a static lifetime.
            // It's a lie, but we'll cleanup the reference right after.
            ctx.buf = Some(unsafe { core::mem::transmute(&mut buf[..]) });
            self_.prepare_channel_xfer(packet_type, buf.len(), Direction::In);
        }).await;
        self.interrupt_context().buf = None;
        r
    }

    pub async fn write(&mut self, packet_type: Option<PacketType>, buf: &[u8]) -> UsbResult<()> {
        self.wait_for_completion(|self_| {
            self_.prepare_channel_xfer(packet_type, buf.len(), Direction::Out);
            // We are writing, and it's blocking when the FIFO is full. Fine for now.
            self_.write_to_non_periodic_fifo(buf);
        }).await
    }

    async fn wait_for_completion(&mut self, mut f: impl FnMut(&mut Self)) -> UsbResult<()> {
        // Ensure the channel is not being used
        debug_assert!(unsafe { self.hcchar().read().chena() == false });

        let mut num_attempts_left = NUM_XFER_ATTEMPTS;

        loop {
            f(self);

            // If the port is disabled (due to a USB disconnection for example),
            // make sure we don't wait as the stop_pending_xfer() function won't
            // trigger a signal (it's been reset).
            if unsafe { REGS.hprt().read().pena() == false } {
                return Err(());
            }

            match self.interrupt_context().event_signal.wait().await {
                ChannelEvent::Complete => return Ok(()),
                ChannelEvent::FatalError => return Err(()),
                ChannelEvent::RetryTransaction => {
                    num_attempts_left -= 1;
                    if num_attempts_left == 0 {
                        debug!("Data transfer retried too many times. Aborting.");
                        return Err(());
                    }
                }
            }
        }
    }

    fn read_from_fifo(&mut self, mut dst: &mut [MaybeUninit<u8>]) {
        let src = self.get_fifo();

        while !dst.is_empty() {
            unsafe {
                let v = src.read().data();

                if dst.len() <= 3 {
                    if dst.len() >= 1 { *dst[0].as_mut_ptr() = (v      ) as u8 }
                    if dst.len() >= 2 { *dst[1].as_mut_ptr() = (v >>  8) as u8 }
                    if dst.len() >= 3 { *dst[2].as_mut_ptr() = (v >> 16) as u8 }
                    return;
                } else {
                    (dst.as_mut_ptr() as *mut u32).write_unaligned(v);
                }
            }
            dst = &mut dst[core::mem::size_of::<u32>()..];
        }
    }


    fn write_to_non_periodic_fifo(&mut self, mut src: &[u8]) {
        let dst = self.get_fifo();

        // We'll be spinning until we have all the data written to the fifo.
        // Other USB stacks don't look at error. We keep pushing bytes, hopefully it doesn't block.
        while !src.is_empty() {
            let mut space_available_in_words = unsafe { REGS.gnptxsts().read().nptxfsav() };
            while !src.is_empty() && space_available_in_words > 0 {
                unsafe {
                    // We'll be transfering garbage if src.len mod 4 != 0,
                    // but that's okay, it's never leaving the device.
                    let v = (src.as_ptr() as *const u32).read_unaligned();
                    dst.write_value(regs::Fifo(v))
                }
                space_available_in_words -= 1;
                src = &src[core::mem::size_of::<u32>().min(src.len())..];
            }
        }
    }

    fn prepare_channel_xfer(&mut self, packet_type: Option<PacketType>, size: usize, dir: Direction) {
        unsafe {
            // Ensure the channel is not being used
            debug_assert!(self.hcchar().read().chena() == false);

            // Ensure we have the right direction
            debug_assert!(self.hcchar().read().epdir() == (dir == Direction::In));

            // 1023 because the pktcnt is 10 bits in the hcchar register
            const MAX_PACKET_COUNT: usize = 1023;

            let max_packet_size = self.hcchar().read().mpsiz() as usize;
            let pkt_cnt = div_round_up(size, max_packet_size).clamp(1, MAX_PACKET_COUNT);

            // Apparently, odd frames are used for periodic endpoints,
            // but the STM32 SDK does it for all endpoints.
            let oddfrm = REGS.hfnum().read().frnum() & 1 == 1;

            trace!("Prepare XFER ch: {}, dir: {:?}, pid: {:?}, pktcnt: {}, size: {}",
              self.ch_index, dir, packet_type, pkt_cnt, size);

            // Per the documentation, in receive mode, we must configure a
            // multiple of the max_packet_size. That's kinda sucky because we
            // are going to have to deal with the corner case of receiving more
            // bytes than what our receive buffer can hold.
            let size = if dir == Direction::In {
                pkt_cnt * max_packet_size
            } else {
                size
            };

            self.hctsiz().modify(|w| {
                // When the dpid field is left alone, DATA0/1 toggle is done
                // automatically.
                if let Some(packet_type) = packet_type {
                    w.set_dpid(packet_type as u8);
                }

                w.set_pktcnt(pkt_cnt as u16);
                w.set_xfrsiz(size as u32);
            });

            self.interrupt_context().event_signal.reset();

            self.hcchar().modify(|w| {
                w.set_oddfrm(oddfrm);
                w.set_chdis(false);
                w.set_chena(true);
            });
        }
    }
}

pub struct ChannelAccessor<'b> {
    channel: &'b mut Channel,
    packet_type: Option<PacketType>,
}

impl<'b> Read for ChannelAccessor<'b> {
    type Error = ();
    type ReadFuture<'a> = impl Future<Output = Result<&'a [u8], Self::Error>> + 'a where Self: 'a;

    fn read<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadFuture<'a> {
        async move {
            self.channel.read(self.packet_type, buf).await?;
            Ok(unsafe { MaybeUninit::slice_assume_init_ref(buf) })
        }
    }
}

impl<'b> Write for ChannelAccessor<'b> {
    type Error = ();
    type WriteFuture<'a> = impl Future<Output = Result<(), Self::Error>> + 'a where Self: 'a;

    fn write<'a>(&'a mut self, buf: &'a [u8]) -> Self::WriteFuture<'a> {
        self.channel.write(self.packet_type, buf)
    }
}

impl<'b> ChannelAccessor<'b> {
    // Because async trait with default impl isn't a thing.
    impl_read_obj!(ChannelAccessor<'b>);
    impl_write_obj!(ChannelAccessor<'b>);
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
    Out = 0,
    In = 0x80,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChannelEvent {
    // Transfer complete
    Complete,
    // Retry this request
    RetryTransaction,
    // No retry on this one
    FatalError,
}
