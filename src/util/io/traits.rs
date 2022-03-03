// SPDX-License-Identifier: GPL-3.0-or-later

use core::{
    future::Future,
    mem::MaybeUninit,
};

pub trait ReadPartial {
    type Error;
    type ReadPartialFuture<'a>: Future<Output = Result<&'a [u8], Self::Error>> + 'a where Self: 'a;
    /// Not all requested bytes may be read. This is useful for optimizing reads
    /// that are not aligned with the block size
    fn read_partial<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadPartialFuture<'a>;
}

pub trait Read {
    type Error;
    type ReadFuture<'a>: Future<Output = Result<&'a [u8], Self::Error>> + 'a where Self: 'a;
    /// All bytes will be read. Otherwise, it errors.
    fn read<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadFuture<'a>;
}

pub trait Write {
    type Error;
    type WriteFuture<'a>: Future<Output = Result<(), Self::Error>> + 'a where Self: 'a;
    /// The entirety of the buffer will be written, otherwise an error is returned.
    fn write<'a>(&'a mut self, buf: &'a [u8]) -> Self::WriteFuture<'a>;
}

pub trait Seek {
    fn seek_from_start(&mut self, pos: u32);
}

// Not sure how to make async functions in a trait with a default
// implementation, so we'll make macros to include. Not ideal. Can someone chim
// in with a better solution?

macro_rules! impl_read_obj {
    ($self:ty) => {
        pub async fn read_obj<O>(&mut self) -> Result<O, <$self as Read>::Error> {
            let mut response = core::mem::MaybeUninit::<O>::uninit();
            self.read(response.as_bytes_mut()).await?;
            Ok(unsafe { response.assume_init() })
        }
    };
}
pub(crate) use impl_read_obj;

macro_rules! impl_write_obj {
    ($self:ty) => {
        pub async fn write_obj<O>(&mut self, obj: &O) -> Result<(), <$self as Write>::Error> {
            let ptr = (obj as *const O) as *const u8;
            let bytes = unsafe { core::slice::from_raw_parts(ptr, core::mem::size_of::<O>()) };
            self.write(bytes).await
        }
    };
}

pub(crate) use impl_write_obj;
