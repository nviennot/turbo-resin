// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem;

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
    time::{Duration, Timer, Instant},
};

use bitflags::bitflags;
use crate::{debug, drivers::clock::delay_ms};
use super::{Channel, EndpointType, Direction, PacketType, ensure, InterfaceHandler, InterfaceDescriptor, EndpointDescriptor, UsbResult};

const USB_MSC_CLASS: u8 = 8;
const USB_MSC_SCSI_SUBCLASS: u8 = 6;

pub struct Msc {
    ep_in: Channel,
    ep_out: Channel,
}

impl InterfaceHandler for Msc {
    fn activate(
        dev_addr: u8,
        if_desc: InterfaceDescriptor,
        ep_descs: &[EndpointDescriptor],
    ) -> UsbResult<Self> {
        ensure!(if_desc.interface_class == USB_MSC_CLASS);
        ensure!(if_desc.interface_subclass == USB_MSC_SCSI_SUBCLASS);
        ensure!(ep_descs.len() == 2);

        // 0x80 means input
        let (ep_in_desc, ep_out_desc) = if ep_descs[0].endpoint_address & 0x80 != 0 {
            (&ep_descs[0], &ep_descs[1])
        } else {
            (&ep_descs[1], &ep_descs[0])
        };

        ensure!(ep_in_desc.attributes == EndpointType::Bulk as u8);
        ensure!(ep_out_desc.attributes == EndpointType::Bulk as u8);

        let ep_in = Channel::new(2, dev_addr, Direction::In, ep_in_desc.endpoint_address & 0x0F,
            EndpointType::Bulk, ep_in_desc.max_packet_size);
        let ep_out = Channel::new(3, dev_addr, Direction::Out, ep_out_desc.endpoint_address & 0x0F,
            EndpointType::Bulk, ep_out_desc.max_packet_size);

        Ok(Self {
            ep_in,
            ep_out,
        })
    }
}

impl Msc {
    pub async fn run(&mut self) -> UsbResult<()> {
        debug!("Msc main loop");
        loop {
            Timer::after(Duration::from_millis(1000)).await;
        }
        Ok(())
    }
}

