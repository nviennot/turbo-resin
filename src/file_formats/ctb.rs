// SPDX-License-Identifier: GPL-3.0-or-later

// Based on https://github.com/sn4k3/UVtools/blob/master/UVtools.Core/FileFormats/ChituboxFile.cs

use core::mem::MaybeUninit;
use crate::drivers::lcd::Color8;
use crate::util::io::{Seek, BufReader, ReadPartial};
use crate::consts::io::*;
use embassy::blocking_mutex::raw::NoopRawMutex;
use embassy::channel::mpsc::{self, Channel, Receiver, Sender};
use alloc::vec::Vec;
use crate::util::io::Read;

type Color7 = u8; // We are spitting out 7bit per pixels colors.

#[inline]
fn color_7bpp_to_8bpp(color: Color7) -> Color8 {
    (color << 1) | (color >> 6)
}

impl Layer {
    pub async fn for_each_pixels<'a, R: ReadPartial + Seek>(
        &'a self,
        reader: &'a mut R,
        layer_index: u32,
        xor_key: u32,
        mut f: impl FnMut(Color8, u32),
    ) -> Result<(), R::Error> {

        let mut color: Color7 = 0;
        let mut repeat: u32 = 0;

        #[derive(PartialEq, Eq)]
        enum RleState {
            None,
            WaitingForHeader,
            WaitingForRLEByte(u8),
        }

        let mut rle_state: RleState = RleState::None;

        self.for_each_bytes(reader, layer_index, xor_key, |bytes| {
            for byte in bytes {
                let byte = *byte;
                match rle_state {
                    RleState::None => {
                        color = byte & 0x7F;
                        if byte & 0x80 != 0 {
                            rle_state = RleState::WaitingForHeader;
                        } else {
                            f(color_7bpp_to_8bpp(color), 1);
                        }
                    }
                    RleState::WaitingForHeader => {
                        let (repeat_, bytes_to_come) =
                             if byte & 0b1000_0000 == 0b0000_0000 { (byte & 0b1111_1111, 0) }
                        else if byte & 0b1100_0000 == 0b1000_0000 { (byte & 0b0111_1111, 1) }
                        else if byte & 0b1110_0000 == 0b1100_0000 { (byte & 0b0011_1111, 2) }
                        else if byte & 0b1111_0000 == 0b1110_0000 { (byte & 0b0001_1111, 3) }
                        else { panic!("file corrupted"); /* TODO return error */ };
                        repeat = repeat_ as u32;
                        rle_state = RleState::WaitingForRLEByte(bytes_to_come);
                    }
                    RleState::WaitingForRLEByte(0) => { /* we'll do that right after */ }
                    RleState::WaitingForRLEByte(n) => {
                        repeat = (repeat << 8) | byte as u32;
                        rle_state = RleState::WaitingForRLEByte(n-1);
                    }
                }

                if rle_state == RleState::WaitingForRLEByte(0) {
                    f(color_7bpp_to_8bpp(color), repeat);
                    rle_state = RleState::None;
                }
            }
        }).await?;

        // TODO return error
        assert!(rle_state == RleState::None);

        Ok(())
    }

    pub async fn for_each_bytes<'a, R: ReadPartial + Seek>(
        &'a self,
        reader: &'a mut R,
        layer_index: u32,
        xor_key: u32,
        mut f: impl FnMut(&[u8]),
    ) -> Result<(), R::Error> {
        reader.seek_from_start(self.image_offset);
        let mut buf_reader = BufReader::new(reader, self.image_size as usize);
        let mut buffer: [MaybeUninit::<u8>; FILE_READER_BUFFER_SIZE] = MaybeUninit::uninit_array();

        let mut xor_engine = if xor_key != 0 {
            Some(XorEngine::new(layer_index, xor_key))
        } else {
            None
        };

        while let Some(data) = buf_reader.next(&mut buffer).await? {
            if let Some(xor_engine) = xor_engine.as_mut() {
                // We need the mutable version of the buffer. It's a bit hacky,
                // but it's okay. We could also make a u32 slice, and xor int
                // by int, but things gets icky when it comes to guarantees on
                // buffers with lengths that aren't multiple of 4.
                let data_mut = unsafe {
                    core::slice::from_raw_parts_mut(data.as_ptr() as *mut u8, data.len())
                };
                xor_engine.process(data_mut);
            }

            f(data);
        }

        Ok(())
    }
}

pub struct XorEngine {
    k1: u32,
    k2: u32,
    offset_mod4: u8,
}

impl XorEngine {
    pub fn new(layer_index: u32, xor_key: u32) -> Self {
        // What a silly thing to do
        let k1: u32 = xor_key.wrapping_mul(0x2d83cdac).wrapping_add(0xd8a83423);
        let k2: u32 = (layer_index.wrapping_mul(0x1e1530cd).wrapping_add(0xec3d47cd)).wrapping_mul(k1);
        // This is for the ext flash
        //let k2: u32 = (layer_index.wrapping_mul(0x1e1530cd).wrapping_add(0x19112FCF)).wrapping_mul(k1);
        let offset_mod4 = 0;
        Self { k1, k2, offset_mod4 }
    }

    pub fn process(&mut self, data: &mut [u8]) {
        // What a silly thing to do
        for d in data {
            *d ^= (self.k2 >> (8*self.offset_mod4)) as u8;
            self.offset_mod4 += 1;
            if self.offset_mod4 == 4 {
                self.offset_mod4 = 0;
                self.k2 = self.k1.wrapping_add(self.k2);
            }
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Header {
    pub magic: u32, //# 0x12FD0086 for CTB, 0x12FD0106 for CTBv4
    pub version: u32,
    pub bed_size_x: f32,
    pub bed_size_y: f32,
    pub bed_size_z: f32,
    pub unknown1: u32,
    pub unknown2: u32,
    pub height_mm: f32,
    pub layer_height_mm: f32,
    pub normal_exposure_duration_sec: f32,
    pub bottom_exposure_duration_sec: f32,
    pub light_off_delay_duration_sec: f32,
    pub num_bottom_layers: u32,
    pub resolution_x: u32,
    pub resolution_y: u32,
    pub large_preview_offset: u32,
    pub layers_offset: u32,
    pub num_layers: u32,
    pub small_preview_offset: u32,
    pub print_duration_sec: u32,
    pub image_mirrored: u32,
    pub print_settings_offset: u32,
    pub print_settings_size: u32,
    pub anti_aliasing_level: u32,
    pub normal_uv_power: u16, // 0x00 to 0xFF
    pub bottom_uv_power: u16, // 0x00 to 0xFF
    pub xor_key: u32,
    pub slicer_settings_offset: u32,
    pub slicer_settings_size: u32,
}

impl Header {
    pub fn check_magic(&self) -> Result<(), ()> {
        match self.magic {
            0x12FD0086 => Ok(()),
            0x12FD0106 => Ok(()),
            _ => Err(()),
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Layer {
    pub position_z_mm: f32,
    pub exposure_time_sec: f32,
    pub light_off_sec: f32,
    pub image_offset: u32,
    pub image_size: u32,
    pub unknown1: u32,
    pub table_size: u32,
    pub unknown3: u32,
    pub unknown4: u32,
}

#[inline(always)]
pub fn div_round_up(v: usize, denom: usize) -> usize {
    (v + denom - 1)/denom
}
