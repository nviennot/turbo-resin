// SPDX-License-Identifier: Apache-2.0 OR MIT

use embassy_stm32::{
    pac,
    pac::otgfs::{OtgFs, regs, vals},
    gpio::low_level::{AFType, Pin},
    peripherals as p,
    rcc::low_level::RccPeripheral,
};
pub(crate) const REGS: OtgFs = pac::USB_OTG_FS;

use embassy_util::channel::signal::Signal;
use embassy_time::{Duration, Timer};

use super::{Channel, DetectedDevice, InterfaceHandler, UsbResult, UsbError};

macro_rules! ensure {
    ($expr:expr, $err:expr) => {
        if (!$expr) {
            return Err($err);
        }
    };
}

pub(crate) use ensure;

// We have 320 32-bit words of FIFO SRAM (1.25KB).
const FIFO_LEN: u16 = 320;
// The FIFO SRAM gets partionned as we wish.
// The sum of these constants must be less than 320.
const RX_FIFO_LEN: u16 = 128;
const NON_PERIODIC_TX_FIFO_LEN: u16 = 96;
const PERIODIC_TX_FIFO_LEN: u16 = 64;

pub struct UsbHost {
    event: Signal<Event>,
}

impl UsbHost {
    pub fn new(
        dm: p::PA11,
        dp: p::PA12,
        _usb: p::USB_OTG_FS,
    ) -> Self {
        unsafe {
            dm.set_as_af(10, AFType::OutputPushPull);
            dp.set_as_af(10, AFType::OutputPushPull);
        }
        let event = Signal::new();
        Self { event }
    }

    pub fn init(&self) {
        // RCC enable/reset
        p::USB_OTG_FS::enable();
        p::USB_OTG_FS::reset();
        self.event.reset();

        // We follow the code from the STM32CubeF1 SDK.
        unsafe {
            // USB_CoreInit() from the SDK
            {
                // Select full-speed Embedded PHY. This is what the device will
                // most likely support. If we get it wrong, we pay the cost of
                // doing an extra port reset (an extra 20ms, no big deal).
                // Doing a port reset is not necessary on the f4 family it appears.
                REGS.hcfg().modify(|w| w.set_fslspcs(vals::Speed::FULL_SPEED));

                // The following performs a Soft Reset. It doesn't seem that it's needed.
                // After all, we just did a hard reset via the RCC register.
                // If we change the PHY clock, we would need to do so.
                /*
                while REGS.grstctl().read().ahbidl() == false {}
                REGS.grstctl().modify(|w| w.set_csrst(true));
                while REGS.grstctl().read().csrst() {}
                */

                // Activate the USB Transceiver
                // It's a bit weird, it's called power down.
                REGS.gccfg().modify(|w| w.set_pwrdwn(true));
            }

            // USB_SetCurrentMode() from the SDK
            {
                REGS.gusbcfg().modify(|w| w
                    // Force host mode.
                    .set_fhmod(true)
                );
                // Wait for the current mode of operation to be host mode
                while REGS.gintsts().read().cmod() == false {}
            }

            // USB_HostInit() from the SDK
            {
                // FIFO setup
                {
                    assert!(RX_FIFO_LEN+NON_PERIODIC_TX_FIFO_LEN+PERIODIC_TX_FIFO_LEN <= FIFO_LEN);

                    REGS.grxfsiz().write(|w|
                        w.set_rxfd(RX_FIFO_LEN)
                    );

                    REGS.hnptxfsiz().write(|w| {
                        w.set_sa(RX_FIFO_LEN);
                        w.set_fd(NON_PERIODIC_TX_FIFO_LEN);
                    });

                    REGS.hptxfsiz().write(|w| {
                        w.set_sa(RX_FIFO_LEN+NON_PERIODIC_TX_FIFO_LEN);
                        w.set_fd(PERIODIC_TX_FIFO_LEN);
                    });

                    // Flush FIFOs, it must be done, otherwise, fs_gnptxsts.nptxfsav is all garbage.

                    // Flush RX Fifo
                    REGS.grstctl().write(|w| w.set_rxfflsh(true));
                    while REGS.grstctl().read().rxfflsh() {}

                    // Flush all TX Fifos
                    REGS.grstctl().write(|w| {
                        w.set_txfflsh(true);
                        w.set_txfnum(vals::Txfnum::ALL);
                    });
                    while REGS.grstctl().read().txfflsh() {}
                }

                // Specify which interrupts we care about
                REGS.gintmsk().write(|w| {
                    // Host port interrupt
                    w.set_prtim(true);
                    // Receive FIFO non-empty
                    w.set_rxflvlm(true);
                    // Host channels
                    w.set_hcim(true);
                    // Disconnected
                    w.set_discint(true);
                    // Incomplete periodic transfer mask
                    w.set_ipxfrm_iisooxfrm(true);
                })
            }

            // HNP is probably needed when we do real OTG and we don't know if
            // we are device or host yet.
            /*
            REGS.gotgctl().modify(|w| w.set_dhnpen(true));
            */

            // HAL_HCD_Start() in the SDK
            {
                // Vbus power
                REGS.hprt().modify(|w| w.set_ppwr(true));
                // Unmask interrupts
                REGS.gahbcfg().modify(|w| w.set_gint(true));
            }
        }
    }

    pub fn on_interrupt(&mut self) {
        let intr = unsafe { REGS.gintsts().read() };
        // Ack all interrupts that we see
        unsafe { REGS.gintsts().write_value(intr) };

        // Host port interrupt
        if intr.hprtint() { self.on_host_port_interrupt(); }

        // Rx FIFO non empty
        if intr.rxflvl() { Channel::on_host_rx_interrupt(); }

        // Host channel interrupt
        if intr.hcint() { Channel::on_host_ch_interrupt(); }

        if intr.discint() {
            debug!("USB Disconnected");
            self.event.signal(Event::Disconnected);
            Channel::on_host_disconnect_interrupt();
        }

        if intr.ipxfr_incompisoout() {
            debug!("USB incomplete periodic TX");
        }
    }

    fn on_host_port_interrupt(&self) {
        unsafe {
            REGS.hprt().modify(|w| {
                // Port detected?
                if w.pcdet() {
                    self.event.signal(Event::DeviceDetected);
                }

                // Port enabled?
                if w.penchng() {
                    if w.pena() {
                        trace!("Port ready");
                        // Not clearing the port enable bit makes the core to disable
                        // the port immedately after. Took me hours to figure this out :(
                        // This is not what the documentation says!
                        w.set_pena(false);

                        let event = self.maybe_change_port_speed(w.pspd());
                        self.event.signal(event.unwrap_or(Event::PortReady));
                    } else {
                        trace!("Port disabled");
                        self.event.signal(Event::Disconnected);
                        Channel::on_host_disconnect_interrupt();
                    }
                }

                // Port overcurrent?
                if w.poca() {
                    debug!("Port went overcurrent");
                    self.event.signal(Event::Disconnected);
                    Channel::on_host_disconnect_interrupt();
                }
            });
        }
    }

    /// When the port speed changes, it returns an event that indicates that the
    /// port needs to be reset.
    fn maybe_change_port_speed(&self, port_speed: vals::Speed) -> Option<Event> {
        unsafe {
            let hfir = match port_speed {
                vals::Speed::LOW_SPEED => 6_000,
                vals::Speed::FULL_SPEED => 48_000,
                v @ _ => panic!("Port speed not recognized: {}", v.0)
            };
            REGS.hfir().write(|w| w.set_frivl(hfir));

            let host_speed = REGS.hcfg().read().fslspcs();
            if port_speed != host_speed {
                REGS.hcfg().modify(|w| w.set_fslspcs(port_speed));

                // Odd. on the f4, we don't need to reset the port again for the
                // new speed to take account. If we do it, we get a port disabled.
                #[cfg(feature="stm32f1")]
                return Some(Event::NeedPortReset);
            }

            None
        }
    }

    async fn reset_port(&self) {
        unsafe {
            trace!("USB port reset initiated");
            // Brings the D- D+ lines down to initiate a device reset
            REGS.hprt().modify(|w| w.set_prst(true));
            // 10ms is the minimum by the USB specs. We add margins.
            Timer::after(Duration::from_millis(20)).await;
            REGS.hprt().modify(|w| w.set_prst(false));
            trace!("USB port reset done");
        }
    }

    async fn wait_for_event(&self, wanted_event: Event) -> UsbResult<()> {
        loop {
            let event = self.event.wait().await;
            match event {
                _ if event == wanted_event => return Ok(()),
                Event::Disconnected => return Err(UsbError::DeviceDisconnected),
                Event::NeedPortReset => {
                    trace!("USB port speed changed. Resetting port");
                    self.reset_port().await;
                }
                // We should not get other events
                // This shouldn't happen. That mean we don't understand the control flow.
                _ => panic!("While waiting for {:?}, received {:?}", wanted_event, event),
            }
        }
    }

    pub async fn wait_for_device(&mut self) -> UsbResult<DetectedDevice> {
        self.init();

        trace!("USB waiting for device");
        self.wait_for_event(Event::DeviceDetected).await?;

        debug!("USB device detected");

        // Let the device boot. USB Specs say 200ms is enough, but some devices
        // can take longer apparently, so we'll wait a little longer.
        Timer::after(Duration::from_millis(300)).await;

        self.reset_port().await;

        self.wait_for_event(Event::PortReady).await?;
        trace!("USB port ready");

        Timer::after(Duration::from_millis(20)).await;
        Ok(DetectedDevice)
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    DeviceDetected,
    PortReady,
    NeedPortReset,
    Disconnected,
}
