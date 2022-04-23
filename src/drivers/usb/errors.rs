// SPDX-License-Identifier: GPL-3.0-or-later

pub type UsbResult<T> = Result<T, UsbError>;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum UsbError {
    // Port errors
        DeviceDisconnected,

    // Enumeration errors
        InterfaceNotFound,
        InvalidDescriptor,
        DescriptorTooLarge,

    // Channel transfer errors
        Nak,
        Stall,
        DataToggleError,
        FrameOverrun,
        BabbleError,
        /// Abort is when the channel was disabled().
        Abort,
        /// Other can be CRC error, timeouts, bit stuffing error, false EOP
        TransactionError,

    // MSC errors
        BotRequestFailed,
        InvalidBlockSize,
}
