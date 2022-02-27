
use embedded_sdmmc::{BlockDevice, Block, BlockIdx, BlockCount, Timestamp, TimeSource, Controller};
use core::cell::RefCell;
use core::future::Future;
use core::mem;
use crate::drivers::usb::Msc;
use crate::runtime::debug;

use super::UsbResult;

pub struct MscBlockDevice {
    block_count:  u32,
    msc: RefCell<Msc>,
}

impl BlockDevice for MscBlockDevice {
    type Error = ();

    type ReadFuture<'a> where Self: 'a = impl Future<Output = Result<(), Self::Error>> + 'a;
    type WriteFuture<'a> where Self: 'a = impl Future<Output = Result<(), Self::Error>> + 'a;

    fn read<'a>(&'a self, blocks: &'a mut [Block], start_block_idx: BlockIdx, reason: &str) -> Self::ReadFuture<'a> {
        async move {
            let dst = unsafe { core::slice::from_raw_parts_mut(
                blocks.as_mut_ptr() as *mut _,
                blocks.len() * mem::size_of::<Block>(),
            )};
            self.msc.borrow_mut().read10(start_block_idx.0, blocks.len() as u16, dst).await
        }
    }

    fn write<'a>(&'a self, blocks: &'a [Block], start_block_idx: BlockIdx) -> Self::WriteFuture<'a> {
        async move {
            let src = unsafe { core::slice::from_raw_parts(
                blocks.as_ptr() as *const _,
                blocks.len() * mem::size_of::<Block>(),
            )};
            self.msc.borrow_mut().write10(start_block_idx.0, blocks.len() as u16, src).await
        }
    }

    fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
        Ok(BlockCount(self.block_count))
    }
}

impl MscBlockDevice {
    pub async fn new(mut msc: Msc) -> UsbResult<Self> {
        debug!("Init Mass Storage Class");

        {
            // Read the number of logical units, not that we'll be accessing multiple ones,
            // We always pick the first one, but there might be a better thing to do.
            let num_luns = msc.get_max_lun().await.unwrap_or(0) + 1;
            if num_luns > 1 {
                debug!("Multiple logical units found ({}). Picking the first one", num_luns);
            } else {
                debug!("Logical units: 1");
            }
        }

        msc.test_unit_ready().await?;
        debug!("Disk is ready");

        let capacity = msc.read_capacity10().await?;
        let block_size = capacity.block_size();
        let block_count = capacity.block_count();
        let disk_size = (block_size as u64) * (block_count as u64);

        if block_size == Block::LEN_U32 {
            debug!("Disk size: {}MB", disk_size/1024/1024);
            Ok(Self { block_count, msc: RefCell::new(msc) })
        } else {
            debug!("Disk has a block size of {}. Not supported", block_size);
            Err(())
        }
    }

    pub fn into_fatfs_controller(self) -> Controller<Self, NullTimeSource> {
        Controller::new(self, NullTimeSource)
    }
}

pub struct NullTimeSource;
impl TimeSource for NullTimeSource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp { year_since_1970: 0, zero_indexed_month: 0, zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
    }
}
