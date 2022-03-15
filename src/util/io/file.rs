// SPDX-License-Identifier: GPL-3.0-or-later

use embedded_sdmmc::{
    Volume,
    Directory,
    File as FileInner,
    Controller,
    Mode,
    BlockDevice,
    TimeSource,
    Error,
};

use core::future::Future;
use core::mem::MaybeUninit;

use super::{Read, ReadPartial, Write, Seek, impl_read_obj, impl_write_obj};

pub struct File<'b, D: BlockDevice, T: TimeSource> {
    inner: FileInner,
    volume: &'b mut Volume,
    fs: &'b mut Controller<D,T>,
}

impl<'b, D: BlockDevice, T: TimeSource> File<'b, D, T> {
    pub async fn new(
        fs: &'b mut Controller<D,T>,
        volume: &'b mut Volume,
        dir: &Directory,
        name: &str,
        mode: Mode,
    ) -> Result<File<'b, D, T>, Error<D::Error>> {
        let inner = fs.open_file_in_dir(volume, dir, name, mode).await?;
        crate::debug!("File open, size={}", inner.length());
        Ok(Self { inner, fs, volume })
    }

    impl_read_obj!(File<'b, D, T>);
    impl_write_obj!(File<'b, D, T>);
}

impl<'b, D: BlockDevice, T: TimeSource> Read for File<'b, D, T> {
    type Error = Error<D::Error>;
    type ReadFuture<'a> = impl Future<Output = Result<&'a [u8], Self::Error>> + 'a where Self: 'a;

    fn read<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadFuture<'a> {
        async move {
            // assume_init because the fs.read() takes an initialized slice.
            let buf_orig: &mut [u8] = unsafe { MaybeUninit::slice_assume_init_mut(buf) };
            let mut buf: &mut [u8] = &mut buf_orig[..];
            while !buf.is_empty() {
                let n = self.fs.read(self.volume, &mut self.inner, buf).await?;
                if n == 0 {
                    // EOF reached
                    return Err(Error::EndOfFile);
                }
                buf = &mut buf[n..];
            }
            Ok(&buf_orig[..])
        }
    }
}

impl<'b, D: BlockDevice, T: TimeSource> ReadPartial for File<'b, D, T> {
    type Error = Error<D::Error>;
    type ReadPartialFuture<'a> = impl Future<Output = Result<&'a [u8], Self::Error>> + 'a where Self: 'a;

    fn read_partial<'a>(&'a mut self, buf: &'a mut [MaybeUninit<u8>]) -> Self::ReadPartialFuture<'a> {
        async move {
            // assume_init because the fs.read() takes an initialized slice.
            let buf: &mut [u8] = unsafe { MaybeUninit::slice_assume_init_mut(buf) };
            let n = self.fs.read(self.volume, &mut self.inner, buf).await?;
            Ok(&buf[0..n])
        }
    }
}

impl<'b, D: BlockDevice, T: TimeSource> Write for File<'b, D, T> {
    type Error = Error<D::Error>;
    type WriteFuture<'a> = impl Future<Output = Result<(), Self::Error>> + 'a where Self: 'a;

    fn write<'a>(&'a mut self, mut buf: &'a [u8]) -> Self::WriteFuture<'a> {
        async move {
            while !buf.is_empty() {
                let n = self.fs.write(self.volume, &mut self.inner, buf).await?;
                if n == 0 {
                    return Err(Error::NotEnoughSpace);
                }
                buf = &buf[n..];
            }
            Ok(())
        }
    }
}

impl<'b, D: BlockDevice, T: TimeSource> Seek for File<'b, D, T> {
    fn seek_from_start(&mut self, pos: u32) {
        self.inner.seek_from_start(pos).unwrap();
    }
}

impl<'b, D: BlockDevice, T: TimeSource> core::ops::Deref for File<'b, D, T> {
    type Target = FileInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'b, D: BlockDevice, T: TimeSource> core::ops::DerefMut for File<'b, D, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

