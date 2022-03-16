// SPDX-License-Identifier: GPL-3.0-or-later

use bitflags::bitflags;
use core::{mem::{self, MaybeUninit}, slice};
use heapless::Vec;

use super::{ensure, Channel, EndpointType, Direction, PacketType, UsbResult};

use crate::util::io::{Read,Write};

// A regular configuration descriptor is 32 bytes, this is plenty of margin.
const CONFIGURATION_DESCRIPTOR_BUFFER_SIZE: usize = 256;
const MAX_INTERFACES: usize = 2;

unsafe fn consume<T>(buf: &mut &[MaybeUninit<u8>]) -> UsbResult<T> {
    ensure!(buf.len() >= mem::size_of::<T>());
    // We make a copy because of potential alignment issues.
    let r = (buf.as_ptr() as *const T).read_unaligned();
    *buf = &buf[mem::size_of::<T>()..];
    Ok(r)
}

pub async fn enumerate<H: InterfaceHandler>() -> UsbResult<H> {
    const DEV_ADDR: u8 = 1;
    let mut ctrl = {
        let mut ctrl = ControlPipe::new(0, 8);
        let dd = ctrl.get_descriptor::<DeviceDescriptorPartial>(0).await?;
        let mps = dd.max_packet_size0 as u16;
        ctrl.set_address(DEV_ADDR).await?;
        ControlPipe::new(DEV_ADDR, mps)
    };

    let num_configurations = {
        let dd = ctrl.get_descriptor::<DeviceDescriptor>(0).await?;
        //debug!("{:#?}", dd);
        dd.num_configurations
    };

    for config_index in 0..(num_configurations as u16) {
        // We must allocate an array of constant size.
        let mut buf = [MaybeUninit::<u8>::uninit(); CONFIGURATION_DESCRIPTOR_BUFFER_SIZE];
        let mut full_cd_buf = {
            // We need to retrieve all descriptors. The length of all descriptors
            // is stored in the configuration descriptor (total_len).
            let total_len = {
                let cd = ctrl.get_descriptor::<ConfigurationDescriptor>(config_index).await?;
                cd.total_len
            };

            ensure!(total_len as usize <= buf.len());
            let buf = &mut buf[0..total_len as usize];
            ctrl.request_bytes_in(
                ControlPipe::STD_DEV,
                Request::GetDescriptor,
                (ConfigurationDescriptor::TYPE as u16) << 8,
                config_index,
                buf,
            ).await?;
            &buf[..]
        };

        unsafe {
            let config = consume::<ConfigurationDescriptor>(&mut full_cd_buf)?;
            //debug!("{:#?}", config);

            for _ in 0..config.num_interfaces {
                let interface = consume::<InterfaceDescriptor>(&mut full_cd_buf)?;

                let endpoints = (0..interface.num_endpoints)
                    .map(|_| consume::<EndpointDescriptor>(&mut full_cd_buf))
                    .collect::<UsbResult<Vec<_, MAX_INTERFACES>>>()?;

                //debug!("{:#?} {:#?}", interface, &endpoints);

                if let Ok(prepare_output) = H::prepare(DEV_ADDR, &interface, &endpoints) {
                    ctrl.set_configuration(config.configuration_value).await?;
                    //debug!("Configuration {} set", config.configuration_value);
                    return Ok(H::new(ctrl, prepare_output));
                }
            }
        }
    }

    debug!("No suitable interfaces found");
    Err(())
}

pub trait InterfaceHandler: Sized {
    type PrepareOutput;
    /// Returns Some() when the handler accepts this interface. None otherwise.
    fn prepare(dev_addr: u8, if_desc: &InterfaceDescriptor, ep_descs: &[EndpointDescriptor]) -> UsbResult<Self::PrepareOutput>;
    fn new(ctrl: ControlPipe, activate: Self::PrepareOutput) -> Self;
    // async in traits are not a stable thing, but we'd like this:
    //   async fn run(&mut self);
}

pub struct ControlPipe {
    ch_in: Channel,
    ch_out: Channel,
}


impl ControlPipe {
    const STD_DEV: RequestType = RequestType::TYPE_STANDARD.union(RequestType::RECIPIENT_DEVICE);

    /// The control pipe always uses channel 0 and 1
    pub fn new(dev_addr: u8, max_packet_size: u16) -> Self {
        let ch_in  = Channel::new(0, dev_addr, Direction::In,  0, EndpointType::Control, max_packet_size);
        let ch_out = Channel::new(1, dev_addr, Direction::Out, 0, EndpointType::Control, max_packet_size);
        Self { ch_in, ch_out }
    }

    pub async fn get_descriptor<T: Descriptor>(&mut self, index: u16) -> UsbResult<T> {
        self.request_in(Self::STD_DEV, Request::GetDescriptor, (T::TYPE as u16) << 8, index).await
    }

    pub async fn set_address(&mut self, dev_addr: u8) -> UsbResult<()> {
        self.request_out(Self::STD_DEV, Request::SetAddress, dev_addr as u16, 0, &()).await
    }

    pub async fn set_configuration(&mut self, configuration_value: u8) -> UsbResult<()> {
        self.request_out(Self::STD_DEV, Request::SetConfiguration, configuration_value as u16, 0, &()).await
    }

    ////////////////////////////////////////////////////////////////////////

    pub async fn request_in<T>(&mut self, request_type: RequestType, request: Request, value: u16, index: u16) -> UsbResult<T> {
        let mut result = MaybeUninit::<T>::uninit();
        let dst = result.as_bytes_mut();
        self.request_bytes_in(request_type, request, value, index, dst).await?;
        Ok(unsafe { result.assume_init() })
    }

    pub async fn request_out<T>(&mut self, request_type: RequestType, request: Request, value: u16, index: u16, src: &T) -> UsbResult<()> {
        let ptr = (src as *const T) as *const u8;
        let src = unsafe { slice::from_raw_parts(ptr, mem::size_of::<T>()) };
        self.request_bytes_out(request_type, request, value, index, src).await
    }

    pub async fn request_bytes_in(&mut self, request_type: RequestType, request: Request, value: u16, index: u16, buf: &mut [MaybeUninit<u8>]) -> UsbResult<()> {
        let pkt = SetupPacket {
            request_type: RequestType::IN | request_type,
            request, value, index,
            length: buf.len() as u16,
        };

        self.ch_out.with_pid(PacketType::Setup).write_obj(&pkt).await?;
        if !buf.is_empty() {
            // TODO It's not clear if we need to force it to Data1, or we should be toggling.
            // try with a small max_packet_size.
            self.ch_in.with_pid(PacketType::Data1).read(buf).await?;
        }
        self.ch_out.with_pid(PacketType::Data1).write_obj(&pkt).await?;

        Ok(())
    }

    pub async fn request_bytes_out(&mut self, request_type: RequestType, request: Request, value: u16, index: u16, buf: &[u8]) -> UsbResult<()> {
        let pkt = SetupPacket {
            request_type: RequestType::OUT | request_type,
            request, value, index,
            length: buf.len() as u16,
        };

        self.ch_out.with_pid(PacketType::Setup).write_obj(&pkt).await?;
        if !buf.is_empty() {
            self.ch_out.with_pid(PacketType::Data1).write(buf).await?;
        }
        self.ch_in.with_pid(PacketType::Data1).read(&mut []).await?;

        Ok(())
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SetupPacket {
    pub request_type: RequestType,
    pub request: Request,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

bitflags! {
    pub struct RequestType: u8 {
        // Recipient
        const RECIPIENT_DEVICE    = 0;
        const RECIPIENT_INTERFACE = 1;
        const RECIPIENT_ENDPOINT  = 2;
        const RECIPIENT_OTHER     = 3;
        // Type
        const TYPE_STANDARD = 0 << 5;
        const TYPE_CLASS    = 1 << 5;
        const TYPE_VENDOR   = 2 << 5;
        const TYPE_RESERVED = 3 << 5;
        // Direction
        const OUT = 0 << 7;
        const IN  = 1 << 7;
    }
}

// XXX We absolutely cannot use `repr(u8) enum` in the struct that we are
// receiving from the USB device. Unspecified enum values breaks things.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Request {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,

    // Not sure if this is the right place, but it's fine for now
    BotReset = 0xFF,
    GetMaxLun = 0xFE,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DescriptorType {
    Device = 1,
    Configuration = 2,
    String = 3,
    Interface = 4,
    Endpoint = 5,
    DeviceQualifier = 6,
    OtherSpeedConfiguration = 7,
    InterfacePower = 8,
}

type StringIndex = u8;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct DeviceDescriptor {
    pub len: u8,
    pub descriptor_type: u8,
    pub bcd_usb: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bcd_device: u16,
    pub manufacturer: StringIndex,
    pub product: StringIndex,
    pub serial_number: StringIndex,
    pub num_configurations: u8,
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct DeviceDescriptorPartial {
    _padding: [u8; 7],
    pub max_packet_size0: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ConfigurationDescriptor {
    pub len: u8,
    pub descriptor_type: u8,
    pub total_len: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration_name: StringIndex,
    pub attributes: u8,
    pub max_power: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct InterfaceDescriptor {
    pub len: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub interface_name: StringIndex,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct EndpointDescriptor {
    pub len: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

pub trait Descriptor {
    const TYPE: DescriptorType;
}

impl Descriptor for DeviceDescriptorPartial {
    const TYPE: DescriptorType = DescriptorType::Device;
}

impl Descriptor for DeviceDescriptor {
    const TYPE: DescriptorType = DescriptorType::Device;
}

impl Descriptor for ConfigurationDescriptor {
    const TYPE: DescriptorType = DescriptorType::Configuration;
}

impl Descriptor for InterfaceDescriptor {
    const TYPE: DescriptorType = DescriptorType::Interface;
}

impl Descriptor for EndpointDescriptor {
    const TYPE: DescriptorType = DescriptorType::Endpoint;
}
