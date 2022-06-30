// SPDX-License-Identifier: Apache-2.0 OR MIT

use core::{mem::MaybeUninit, convert::From};

use embassy_stm32::pac::{
    otgfs::{regs, vals},
    common::{Reg, RW},
};

use crate::util::io::{Read, Write, impl_read_obj, impl_write_obj};
use core::future::Future;
use embassy::{channel::signal::Signal, time::{Timer, Duration}};
use super::{REGS, UsbResult, UsbError};

const NUM_CHANNELS: usize = 8;
 // There can be many NAKs. It's okay to do _that_ many retries, it's fast.
const NUM_XFER_NAK_ATTEMPTS: usize = 100_000;
const NUM_XFER_ATTEMPTS: usize = 5;

struct ChannelInterruptContext {
    xfer_signal: Signal<UsbResult<()>>,
    buf: Option<&'static mut [MaybeUninit<u8>]>,
}

impl Default for ChannelInterruptContext {
    fn default() -> Self {
        Self {
            xfer_signal: Signal::new(),
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
        assert!((ch_index as usize) < NUM_CHANNELS);
        let mut c = unsafe { Self::steal(ch_index) };
        c.init(dev_addr, ep_dir, ep_number, ep_type, max_packet_size);
        c
    }

    #[inline(always)]
    fn init(&mut self, dev_addr: u8, ep_dir: Direction, ep_number: u8, ep_type: EndpointType, max_packet_size: u16) {
        trace!("new channel: ch_index={}, dev_addr={}, ep_dir={:?}, ep_number={}, ep_type={:?}, mps={}",
                self.ch_index, dev_addr, ep_dir, ep_number, ep_type, max_packet_size);

        // TODO low_speed: This is used when we talk to a low_speed through a high_speed hub.
        let low_speed = false;

        *self.interrupt_context() = Default::default();
        unsafe {
            // If we were to re-use this channel, disabling it ensures that this channel is ready.
            self.disable();
            self.wait_disabled();

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

            self.hctsiz().write(|w| {
                w.set_dpid(PacketType::Data0 as u8)
            });

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

    fn disable(&self) {
        unsafe {
            self.hcchar().modify(|w| {
                w.set_chdis(true);
                // Yes, we must activate the channel to be able to disable it.
                w.set_chena(true);
            });
        }
    }

    fn wait_disabled(&self) {
        unsafe { while self.hcchar().read().chena() {} }
    }

    fn signal_xfer_result(&self, event: UsbResult<()>) {
        self.interrupt_context().xfer_signal.signal(event);
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

        let result = if hcint.xfrc() {
            trace!("  ch={} Transfer complete", self.ch_index);
            Ok(())
        } else {
            let mut err: Option<UsbError> = None;

            // It's a possibility that multiple error flags are set.
            // For example, the babble error comes with a transaction error.
            // The other of the if statements matter so we get the most specific error.

            if hcint.nak() {
                trace!("  ch={} NAK", self.ch_index);
                err = Some(UsbError::Nak);
            }

            if hcint.txerr() {
                trace!("  ch={} Transaction error", self.ch_index);
                err = Some(UsbError::TransactionError);
            }

            if hcint.stall() {
                trace!("  ch={} Stall response", self.ch_index);
                err = Some(UsbError::Stall);
            }

            if hcint.dterr() {
                trace!("  ch={} Data toggle error", self.ch_index);
                err = Some(UsbError::DataToggleError);
            }

            if hcint.frmor() {
                trace!("  ch={} Frame overrrun", self.ch_index);
                err = Some(UsbError::FrameOverrun);
            }

            if hcint.bberr() {
                trace!("  ch={} Babble error", self.ch_index);
                err = Some(UsbError::BabbleError);
            }

            Err(err.expect("Missing channel error"))
        };

        self.signal_xfer_result(result);
    }

    pub fn on_host_rx_interrupt() {
        unsafe {
            let rx_status = REGS.grxstsp_host().read();
            let ch_index = rx_status.chnum();
            Channel::steal(ch_index).on_rx_interrupt(rx_status);
        }
    }

    fn on_rx_interrupt(&mut self, rx_status: regs::GrxstsHost) {
        // In this function, we don't self.signal_event(ChannelEvent::XXX), it's
        // because on_ch_interrupt() takes care of the signaling. We are forced
        // to look at these events (IN_DATA_DONE, etc.) because these events are
        // stored on the RX Fifo, and there's no way around not popping these.
        match rx_status.pktsts() {
            vals::Pktstsh::IN_DATA_RX => {
                let len = rx_status.bcnt() as usize;
                trace!("  ch={} RX data received: len={}", self.ch_index, len);
                self.on_data_rx(len);
            }
            vals::Pktstsh::IN_DATA_DONE => {
                trace!("  ch={} RX done", self.ch_index);
            }
            vals::Pktstsh::DATA_TOGGLE_ERR => {
                trace!("  ch={} RX Data toggle error", self.ch_index);
            }
            vals::Pktstsh::CHANNEL_HALTED => {
                trace!("  ch={} RX Channel halted", self.ch_index);
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
            debug!("ch={}, Too many bytes received (len={}), disabling channel", self.ch_index, len);
            // Flush the RX fifo
            for _ in 0..(len+3)/4 {
                unsafe { self.get_fifo().read() };
            }

            self.disable();
            self.signal_xfer_result(Err(UsbError::BabbleError));
        } else {
            // Read and advance the buffer pointer.
            self.read_from_fifo(&mut buf[0..len]);
            buf = &mut buf[len..];

            if !buf.is_empty() {
                // Re-enable channel if more data is to come.
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
            let channel = unsafe { Channel::steal(ch_index) };
            // We don't wait for the disable to finish. It's unclear
            // whether it's a good idea to wait within the interrupt routine.
            channel.disable();
            channel.signal_xfer_result(Err(UsbError::DeviceDisconnected));
            enabled_channels &= !(1 << ch_index);
        }
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
        let r = self.wait_for_completion(|self_| {
            let ctx = self_.interrupt_context();
            // transmute because ctx.buf has a static lifetime.
            // It's a lie, but we cleanup the reference immediately after.
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
            self_.write_to_fifo(buf);
        }).await
    }

    async fn wait_for_completion(&mut self, mut f: impl FnMut(&mut Self)) -> UsbResult<()> {
        // Perhaps we could call self.disable() if it is being used, but for now, let's panic.
        debug_assert!(unsafe { self.hcchar().read().chena() == false });

        let mut num_nak_attempts_left = NUM_XFER_NAK_ATTEMPTS;
        let mut num_attempts_left = NUM_XFER_ATTEMPTS;
        fn dec(v: &mut usize) -> usize {
            *v -= 1;
            *v
        }

        loop {
            f(self);

            // We get interrupts if the device is disconnected, so the check is not racy.
            if unsafe { REGS.hprt().read().pena() == false } {
                return Err(UsbError::DeviceDisconnected);
            }

            match self.interrupt_context().xfer_signal.wait().await {
                Ok(()) => return Ok(()),
                Err(e) => match ErrorClass::from(e) {
                    ErrorClass::RetryableNak => {
                        if dec(&mut num_nak_attempts_left) == 0 {
                            debug!("Aborting transaction: too many NAKs received");
                            return Err(e);
                        }
                    }
                    ErrorClass::Retryable => {
                        if dec(&mut num_attempts_left) == 0 {
                            debug!("Aborting transaction: too many transfer errors: {:?}", e);
                            return Err(e);
                        }
                    }
                    ErrorClass::Fatal => {
                        debug!("Aborting transaction: fatal error: {:?}", e);
                        return Err(e)
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


    fn write_to_fifo(&mut self, mut src: &[u8]) {
        let dst = self.get_fifo();

        // We'll be spinning until we have all the data written to the fifo.
        while !src.is_empty() {
            let mut space_available_in_words = unsafe { REGS.gnptxsts().read().nptxfsav() };
            while !src.is_empty() && space_available_in_words > 0 {
                unsafe {
                    // We'll be sending garbage to the fifo if src.len mod 4 != 0,
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
            // Ensure we have the right direction
            debug_assert!(self.hcchar().read().epdir() == (dir == Direction::In));

            // 1023 because the pktcnt is 10 bits in the hcchar register
            const MAX_PACKET_COUNT: usize = 1023;

            let max_packet_size = self.hcchar().read().mpsiz() as usize;
            let pkt_cnt = div_round_up(size, max_packet_size).clamp(1, MAX_PACKET_COUNT);

            // Apparently, odd frames are used for periodic endpoints,
            // but the STM32 SDK does it for all endpoints.
            let oddfrm = REGS.hfnum().read().frnum() & 1 == 1;

            let nptx = REGS.gnptxsts().read();

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

            // Note: We cannot use hctsiz.modify() here. Sometimes, the bit 31
            // "Reserved" is set, and if we write it back, everything stops working.
            self.hctsiz().write(|w| {
                // When the dpid field is left alone, DATA0/1 toggle is done
                // automatically.
                if let Some(packet_type) = packet_type {
                    w.set_dpid(packet_type as u8);
                } else {
                    w.set_dpid(self.hctsiz().read().dpid());
                }

                w.set_pktcnt(pkt_cnt as u16);
                w.set_xfrsiz(size as u32);
            });

            self.interrupt_context().xfer_signal.reset();

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
    type Error = UsbError;
    type ReadFuture<'a> = impl Future<Output = Result<&'a [u8], Self::Error>> + 'a where Self: 'a;

    fn read<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadFuture<'a> {
        async move {
            self.channel.read(self.packet_type, buf).await?;
            Ok(unsafe { MaybeUninit::slice_assume_init_ref(buf) })
        }
    }
}

impl<'b> Write for ChannelAccessor<'b> {
    type Error = UsbError;
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

enum ErrorClass {
    RetryableNak,
    Retryable,
    Fatal,
}

impl From<UsbError> for ErrorClass {
    fn from(v: UsbError) -> Self {
        use UsbError::*;
        match v {
            Nak => Self::RetryableNak,
            DataToggleError | FrameOverrun | BabbleError | TransactionError => Self::Retryable,
            _ => Self::Fatal,
        }
    }
}
