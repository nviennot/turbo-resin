// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem::{self, MaybeUninit};

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
use super::{Channel, EndpointType, Direction, PacketType, ControlPipe, ensure, InterfaceHandler, InterfaceDescriptor, EndpointDescriptor, UsbResult, RequestType, Request};

const USB_MSC_CLASS: u8 = 8;
const USB_MSC_SCSI_SUBCLASS: u8 = 6;
const USB_MSC_BOT_PROTOCOL: u8 = 0x50;

const NUM_ATTEMPS: usize = 100;

// Mass Storage Class

pub struct Msc {
    ctrl: ControlPipe,
    data_in: Channel,
    data_out: Channel,
}

impl InterfaceHandler for Msc {
    type PrepareOutput = (Channel, Channel);

    fn prepare(
        dev_addr: u8,
        if_desc: &InterfaceDescriptor,
        ep_descs: &[EndpointDescriptor],
    ) -> UsbResult<Self::PrepareOutput> {
        ensure!(if_desc.interface_class == USB_MSC_CLASS);
        ensure!(if_desc.interface_subclass == USB_MSC_SCSI_SUBCLASS);
        ensure!(if_desc.interface_protocol == USB_MSC_BOT_PROTOCOL);

        ensure!(ep_descs.len() == 2);

        // 0x80 means input
        let (ep_in_desc, ep_out_desc) = if ep_descs[0].endpoint_address & 0x80 != 0 {
            (&ep_descs[0], &ep_descs[1])
        } else {
            (&ep_descs[1], &ep_descs[0])
        };

        ensure!(ep_in_desc.endpoint_address & 0x80 != 0);
        ensure!(ep_out_desc.endpoint_address & 0x80 == 0);

        ensure!(ep_in_desc.attributes == EndpointType::Bulk as u8);
        ensure!(ep_out_desc.attributes == EndpointType::Bulk as u8);

        let data_in = Channel::new(2, dev_addr, Direction::In, ep_in_desc.endpoint_address & 0x0F,
            EndpointType::Bulk, ep_in_desc.max_packet_size);
        let data_out = Channel::new(3, dev_addr, Direction::Out, ep_out_desc.endpoint_address & 0x0F,
            EndpointType::Bulk, ep_out_desc.max_packet_size);

        Ok((data_in, data_out))
    }

    fn new(ctrl: ControlPipe, (data_in, data_out): (Channel, Channel)) -> Self {
        Self { ctrl, data_in, data_out }
    }
}

impl Msc {
    async fn get_max_lun(&mut self) -> UsbResult<u8> {
        self.ctrl.request_in(
            RequestType::TYPE_CLASS | RequestType::RECIPIENT_INTERFACE,
            Request::GetMaxLun, 0, 0,
        ).await
    }

    async fn reset_bot(&mut self) -> UsbResult<()> {
        self.ctrl.request_out(
            RequestType::TYPE_CLASS | RequestType::RECIPIENT_INTERFACE,
            Request::BotReset, 0, 0, &(),
        ).await
    }

    async fn bot_request_in<T: 'static>(&mut self, cmd: T, dst: &mut [MaybeUninit<u8>]) -> UsbResult<()>
      where [(); 16 - core::mem::size_of::<T>()]: {
        let cmd = CommandBlockWrapper::new(Direction::In, dst.len() as u32, cmd);
        for i in 0..NUM_ATTEMPS {
            //debug!("request_in attempt={}", i);
            self.data_out.write(None, &cmd).await?;
            //debug!("request_in cmd sent");
            if !dst.is_empty() {
                self.data_in.read_bytes(None, dst).await?;
                //debug!("IN read bytes len={}", dst.len());
            }
            if self.data_in.read::<CommandStatusWrapper>(None).await?.success() {
                //debug!("status received, OK");
                return Ok(());
            }
            //debug!("status bad");
            Timer::after(Duration::from_millis(1)).await;
        }
        debug!("MSC command retried too many times. Abort");
        Err(())
    }

    async fn bot_request_out<T: 'static>(&mut self, cmd: T, src: &[u8]) -> UsbResult<()>
      where [(); 16 - core::mem::size_of::<T>()]: {
        let cmd = CommandBlockWrapper::new(Direction::Out, src.len() as u32, cmd);
        for i in 0..NUM_ATTEMPS {
            //debug!("request_out attempt={}", i);
            self.data_out.write(None, &cmd).await?;
            //debug!("request_out cmd sent");
            if !src.is_empty() {
                self.data_out.write_bytes(None, src).await?;
                //debug!("OUT write bytes len={}", src.len());
            }
            if self.data_in.read::<CommandStatusWrapper>(None).await?.success() {
                //debug!("status received, OK");
                return Ok(());
            }
            //debug!("status bad");
            Timer::after(Duration::from_millis(1)).await;
        }
        debug!("MSC command retried too many times. Abort");
        Err(())
    }

    async fn test_unit_ready(&mut self) -> UsbResult<()> {
        let cmd = scsi::TestUnitReady::new();
        self.bot_request_out(cmd, &mut[]).await
    }

    async fn read_capacity10(&mut self) -> UsbResult<scsi::ReadCapacity10Response> {
        let cmd = scsi::ReadCapacity10::new();
        let mut response = MaybeUninit::<scsi::ReadCapacity10Response>::uninit();
        self.bot_request_in(cmd, response.as_bytes_mut()).await?;
        Ok(unsafe { response.assume_init() })
    }

    async fn read10(&mut self, lba: u32, num_blocks: u16, dst: &mut [MaybeUninit<u8>]) -> UsbResult<()> {
        let cmd = scsi::Read10::new(lba, num_blocks);
        self.bot_request_in(cmd, dst).await
    }

    async fn write10(&mut self, lba: u32, num_blocks: u16, src: &[u8]) -> UsbResult<()> {
        let cmd = scsi::Write10::new(lba, num_blocks);
        self.bot_request_out(cmd, src).await
    }

    pub async fn run(&mut self) -> UsbResult<()> {
        debug!("Init Mass Storage Class");

        {
            // Read the number of logical units, not that we'll be accessing multiple ones,
            // We always pick the first one, but there might be a better thing to do.
            let num_luns = self.get_max_lun().await.unwrap_or(0) + 1;
            if num_luns > 1 {
                debug!("Multiple logical units found ({}). Picking the first one", num_luns);
            } else {
                debug!("Logical units: 1");
            }
        }

        self.test_unit_ready().await?;
        debug!("Disk is ready");

        {
            let capacity = self.read_capacity10().await?;
            let block_size = capacity.block_size();
            let block_count = capacity.block_count();
            let disk_size = (block_size as u64) * (block_count as u64);
            debug!("Disk size: {}MB", disk_size/1024/1024);
        }

        // Reading a block
        loop
        {
            let mut buf = [MaybeUninit::<u8>::uninit(); 512];
            self.read10(0x4080, 1, &mut buf).await?;
            let buf: &[u8] = unsafe { core::mem::transmute(&buf[..]) };
            //debug!("sector read");
        }

        Ok(())
    }
}

const CBW_SIG: u32 = 0x43425355; // Spells USBC
const CSW_SIG: u32 = 0x53425355; // Spells USBS
const CBW_TAG: u32 = 0x1155AA33; // semi-random

#[repr(C, packed)]
#[derive()]
pub struct CommandBlockWrapper<T: Sized>  where [(); 16 - core::mem::size_of::<T>()]: {
    pub signature: u32,
    pub tag: u32,
    pub data_len: u32,
    pub flags: u8, // direction
    pub lun: u8,
    pub cb_len: u8,
    pub cb: T,
    pub padding: [u8; 16 - core::mem::size_of::<T>()]
}

impl<T: Sized + 'static> CommandBlockWrapper<T> where [(); 16 - core::mem::size_of::<T>()]: {
    pub fn new(dir: Direction, data_len: u32, payload: T) -> Self {
        Self {
            signature: CBW_SIG,
            tag: CBW_TAG,
            data_len,
            flags: dir as u8,
            lun: 0,
            cb_len: core::mem::size_of::<T>() as u8,
            cb: payload,
            padding: [0; 16 - core::mem::size_of::<T>()],
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct CommandStatusWrapper {
    pub signature: u32,
    pub tag: u32,
    pub data_residue: u32, // was 0x00 02 00 00 // Actual length
    pub status: u8, // 0 ok, 1 failed, 2, phase error
}

impl CommandStatusWrapper {
    pub fn is_valid(&self) -> bool {
        self.tag == CBW_TAG
    }
    pub fn success(&self) -> bool {
        // 0 => Success
        // 1 => Failed
        // 2 => Phase error
        // We don't really care about the error difference. we'll reset the USB
        // device eventually.
        self.is_valid() && self.status == 0
    }
}

mod scsi {
    // Each struct represent a different SCSI command and response

    // Command: TEST UNIT READY, opcode 0x00
    #[repr(C, packed)]
    #[derive(Default)]
    pub struct TestUnitReady {
        opcode: u8,
        reserved: u32,
        control: u8,
    }
    impl TestUnitReady {
        pub fn new() -> Self {
            Self { opcode: 0x00, ..Default::default() }
        }
    }

    // Command: READ CAPACITY(10), opcode = 0x25
    #[repr(C, packed)]
    #[derive(Default)]
    pub struct ReadCapacity10 {
        opcode: u8,
        reserved1: u8,
        lba: u32,
        reserved2: u16,
        reserved3: u8,
        control: u8,
    }
    impl ReadCapacity10 {
        pub fn new() -> Self {
            Self { opcode: 0x25, ..Default::default() }
        }
    }

    // Response: READ CAPACITY(10)
    #[repr(C, packed)]
    #[derive(Default)]
    pub struct ReadCapacity10Response {
        block_count_msb: u32,
        block_size_msb: u32,
    }
    impl ReadCapacity10Response {
        pub fn block_count(&self) -> u32 { self.block_count_msb.to_be() }
        pub fn block_size(&self) -> u32 { self.block_size_msb.to_be() }
    }

    // Command: READ(10), opcode 0x28
    #[repr(C, packed)]
    #[derive(Default)]
    pub struct Read10 {
        opcode: u8,
        flags: u8,
        lba_msb: u32,
        group_number: u8,
        len_msb: u16,
        control: u8,
    }
    impl Read10 {
        pub fn new(lba: u32, blocks: u16) -> Self {
            Self {
                opcode: 0x28,
                lba_msb: lba.to_be(),
                len_msb: blocks.to_be(),
                ..Default::default()
            }
        }
    }

    // Command: WRITE(10), opcode 0x2A
    #[repr(C, packed)]
    #[derive(Default)]
    pub struct Write10 {
        opcode: u8,
        flags: u8,
        lba_msb: u32,
        group_number: u8,
        len_msb: u16,
        control: u8,
    }
    impl Write10 {
        pub fn new(lba: u32, blocks: u16) -> Self {
            Self {
                opcode: 0x2A,
                lba_msb: lba.to_be(),
                len_msb: blocks.to_be(),
                ..Default::default()
            }
        }
    }

}
