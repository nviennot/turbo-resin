// SPDX-License-Identifier: GPL-3.0-or-later

use core::{
    future::Future,
    mem::{self, MaybeUninit},
    slice,
};

pub trait Read {
    type Error;
    type ReadFuture<'a>: Future<Output = Result<(), Self::Error>> + 'a where Self: 'a;
    fn read<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadFuture<'a>;
}

pub trait Write {
    type Error;
    type WriteFuture<'a>: Future<Output = Result<(), Self::Error>> + 'a where Self: 'a;
    fn write<'a>(&'a mut self, buf: &'a [u8]) -> Self::WriteFuture<'a>;
}

// Not sure how to make async functions in a trait with a default
// implementation, so we'll make macros to include. Not ideal. Can someone chim
// in with a better solution?

macro_rules! impl_read_obj {
    ($err:ty) => {
        pub async fn read_obj<O>(&mut self) -> Result<O, $err> {
            let mut response = core::mem::MaybeUninit::<O>::uninit();
            self.read(response.as_bytes_mut()).await?;
            Ok(unsafe { response.assume_init() })
        }
    };
}
pub(crate) use impl_read_obj;

macro_rules! impl_write_obj {
    ($err:ty) => {
        pub async fn write_obj<O>(&mut self, obj: &O) -> Result<(), $err> {
            let ptr = (obj as *const O) as *const u8;
            let bytes = unsafe { core::slice::from_raw_parts(ptr, core::mem::size_of::<O>()) };
            self.write(bytes).await
        }
    };
}

pub(crate) use impl_write_obj;


pub enum DirectionBuffer<'a> {
    In(&'a mut [MaybeUninit<u8>]), // read()
    Out(&'a [u8]), // write()
}

impl<'a> DirectionBuffer<'a> {
    pub fn len(&self) -> usize {
        match self {
            DirectionBuffer::In(buf) => buf.len(),
            DirectionBuffer::Out(buf) => buf.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dir(&self) -> Direction {
        match self {
            DirectionBuffer::In(_) => Direction::In,
            DirectionBuffer::Out(_) => Direction::Out,
        }
    }
}

use crate::drivers::usb::Direction;
