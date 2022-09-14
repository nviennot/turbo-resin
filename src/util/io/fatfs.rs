// SPDX-License-Identifier: Apache-2.0 OR MIT

use embedded_sdmmc::{Timestamp, TimeSource, Controller, Volume, Directory, Mode};
use crate::util::io::File;

use crate::drivers::usb::{
    Msc, UsbHost,
    UsbResult, UsbError,
    MscBlockDevice,
};

pub type Error = embedded_sdmmc::Error<UsbError>;
pub type Result<T> = core::result::Result<T, Error>;

pub struct NullTimeSource;
impl TimeSource for NullTimeSource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp { year_since_1970: 0, zero_indexed_month: 0, zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
    }
}

type TimelessController = Controller<MscBlockDevice, NullTimeSource>;

impl From<MscBlockDevice> for TimelessController {
    fn from(msc: MscBlockDevice) -> Self {
        Self::new(msc, NullTimeSource)
    }
}

impl UsbHost {
    pub async fn wait_for_filesystem(&mut self) -> Result<FileSystem> {
        // An inner function just to make error handling easier.
        async fn wait_for_usb_block_device(usb: &mut UsbHost) -> UsbResult<MscBlockDevice> {
            usb.wait_for_device().await?
                .enumerate::<Msc>().await?
                .into_block_device().await
        }

        let mut fs: TimelessController = wait_for_usb_block_device(self).await
            .map_err(embedded_sdmmc::Error::DeviceError)?.into();

        debug!("Disk initialized");
        let volume = fs.get_volume(embedded_sdmmc::VolumeIdx(0)).await?;
        trace!("{:#?}", volume);
        let root = fs.open_root_dir(&volume)?;

        debug!("Root dir:");
        fs.iterate_dir(&volume, &root, |entry| {
            if !entry.attributes.is_hidden() {
                let ftype = if entry.attributes.is_directory() { "DIR" } else { "FILE" };
                debug!("  {:4} {:3}MB {} {}", ftype, entry.size/1024/1024, entry.mtime, entry.name);
            }
        }).await?;

        Ok(FileSystem { fs, volume, root })
    }
}

pub struct FileSystem {
    fs: Controller<MscBlockDevice, NullTimeSource>,
    volume: Volume,
    root: Directory,
}

type FsFile<'a> = File<'a, MscBlockDevice, NullTimeSource>;

impl FileSystem {
    pub async fn open<'a>(&'a mut self, filename: &str, mode: Mode) -> Result<FsFile> {
        File::new(&mut self.fs, &mut self.volume, &self.root, filename, mode).await
    }
}
