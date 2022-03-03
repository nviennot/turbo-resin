// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem::MaybeUninit;

use super::ReadPartial;

pub struct BufReader<'a, R> {
    reader: &'a mut R,
    len: usize,
    //buffer: [MaybeUninit::<u8>; FILE_READER_BUFFER_SIZE],
}

impl<'a, R: ReadPartial> BufReader<'a, R> {
    pub fn new(reader: &'a mut R, len: usize) -> Self {
        Self { reader, len }
    }

    // Normally we would want buffer to be in the struct, but that crashes the
    // compiler. So we'll provide it externally.
    pub async fn next<'b>(&'b mut self, buffer: &'b mut [MaybeUninit<u8>]) -> Result<Option<&'b [u8]>, R::Error> {
        if self.len == 0 {
            Ok(None)
        } else {
            let to_read = self.len.min(buffer.len());
            let buf = self.reader.read_partial(&mut buffer[0..to_read]).await?;
            self.len -= buf.len();
            Ok(Some(buf))
        }
    }
}
