// SPDX-License-Identifier: GPL-3.0-or-later

use core::mem;

use stm32f1xx_hal::{
    gpio::*,
    gpio::gpioa::*,
    rcc::Clocks,
    prelude::*,
    rcc::{Enable, Reset},
    time::Hertz,
    pac::{self, usb_otg_host::{FS_HCCHAR0, FS_HCINT0, FS_HCINTMSK0, FS_HCTSIZ0}},
};

use embassy::{
    channel::signal::Signal,
    time::{Duration, Timer, Instant},
};

use bitflags::bitflags;
use crate::{debug, drivers::{clock::delay_ms, usb::Msc}};
use super::{Channel, EndpointType, Direction, PacketType, enumerate};

pub type UsbResult<T> = Result<T, ()>;

macro_rules! ensure {
    ($expr:expr) => {
        if (!$expr) {
            return Err(());
        }
    };
}

pub(crate) use ensure;

//const NUM_USB_CHANNELS: usize = 8;

// We have 320 32-bit words of FIFO SRAM (1.25KB).
const FIFO_LEN: u16 = 320;
// The FIFO SRAM gets partionned as we wish.
// The sum of these constants must be less than 320.
const RX_FIFO_LEN: u16 = 128;
const TX_FIFO_LEN: u16 = 128;
const PERIODIC_TX_FIFO_LEN: u16 = 64;

pub struct UsbHost {
    otg_global: pac::OTG_FS_GLOBAL,
    otg_host: pac::USB_OTG_HOST,
    otg_pwrclk: pac::OTG_FS_PWRCLK,

    event: Signal<Event>,
}

impl UsbHost {
    pub fn new(
        dm: PA11<Input<Floating>>,
        dp: PA12<Input<Floating>>,
        otg_global: pac::OTG_FS_GLOBAL,
        otg_host: pac::USB_OTG_HOST,
        otg_pwrclk: pac::OTG_FS_PWRCLK,
        gpioa_crh: &mut Cr<CRH, 'A'>,
    ) -> Self {
        // Pins are configured by default at maximum speed
        dm.into_alternate_push_pull(gpioa_crh);
        dp.into_alternate_push_pull(gpioa_crh);

        Self::enable();

        let event = Signal::new();

        Self { otg_global, otg_host, otg_pwrclk, event }
    }

    fn enable() {
        let rcc = unsafe { &(*pac::RCC::ptr()) };
        rcc.ahbenr.modify(|_,w| w.otgfsen().set_bit());
    }

    fn reset() {
        let rcc = unsafe { &(*pac::RCC::ptr()) };
        rcc.ahbrstr.modify(|_,w| w.otgfsrst().set_bit());
        rcc.ahbrstr.modify(|_,w| w.otgfsrst().clear_bit());
    }

    pub fn init(&self) {
        Self::reset();
        // We follow the code from the STM32CubeF1 SDK.
        unsafe {
            // USB_CoreInit() from the SDK
            {
                // Select full-speed Embedded PHY
                self.otg_host.fs_hcfg.modify(|_,w| w
                    .fslspcs().bits(0b01)
                );

                // Core Soft Reset
                while self.otg_global.fs_grstctl.read().ahbidl().bit_is_clear() {}
                self.otg_global.fs_grstctl.modify(|_,w| w.csrst().set_bit());
                while self.otg_global.fs_grstctl.read().csrst().bit_is_set() {}

                // Activate the USB Transceiver
                // Note: It's a bit weird, it's called power down
                self.otg_global.fs_gccfg.modify(|_,w| w.pwrdwn().set_bit());
            }

            // USB_SetCurrentMode() from the SDK
            {
                self.otg_global.fs_gusbcfg.modify(|_,w| w
                    // Force host mode.
                    .fhmod().set_bit()
                );
                // Wait for the current mode of operation is Host mode
                while self.otg_global.fs_gintsts.read().cmod().bit_is_clear() {}
            }

            // USB_HostInit() from the SDK
            {
                // Restart the PHY Clock
                self.otg_pwrclk.fs_pcgcctl.write(|w| w.bits(0));

                // FIFO setup
                {
                    assert!(RX_FIFO_LEN+TX_FIFO_LEN+PERIODIC_TX_FIFO_LEN <= FIFO_LEN);

                    self.otg_global.fs_grxfsiz.write(|w| w
                        .rxfd().bits(RX_FIFO_LEN)
                    );

                    self.otg_global.fs_gnptxfsiz_host().write(|w| w
                        .nptxfsa().bits(RX_FIFO_LEN)
                        .nptxfd().bits(TX_FIFO_LEN)
                    );

                    self.otg_global.fs_hptxfsiz.write(|w| w
                        .ptxsa().bits(RX_FIFO_LEN+TX_FIFO_LEN)
                        .ptxfsiz().bits(PERIODIC_TX_FIFO_LEN)
                    );
                }

                // Specify which interrupts we care about
                self.otg_global.fs_gintmsk.write(|w| w
                    // bit 24 is PRTIM = Host port interrupt (it's incorrectly defined as read-only in the SVD file.)
                    .bits(1 << 24)
                    // Receive FIFO non-empty
                    .rxflvlm().set_bit()
                    // Host channels
                    .hcim().set_bit()
                    // Disconnected
                    .discint().set_bit()
                    // Incomplete periodic transfer mask
                    .ipxfrm_iisooxfrm().set_bit()
                )
            }

            self.otg_global.fs_gotgctl.modify(|_,w| w.dhnpen().set_bit());

            // HAL_HCD_Start() in the SDK
            {
                // Vbus power
                self.otg_host.fs_hprt.modify(|_,w| w
                    .ppwr().set_bit()
                );

                // unmask interrupts
                self.otg_global.fs_gahbcfg.modify(|_,w| w
                    .gint().set_bit()
                );
            }
        }
    }

    pub fn on_interrupt(&mut self) {
        let intr = self.otg_global.fs_gintsts.read();
        // Ack all interrupts that we see
        unsafe { self.otg_global.fs_gintsts.write(|w| w.bits(intr.bits())) };

        // Host port interrupt
        if intr.hprtint().bit_is_set() { self.on_host_port_interrupt(); }

        // Rx FIFO non empty
        if intr.rxflvl().bit_is_set() { Channel::on_host_rx_interrupt(); }

        // Host channel interrupt
        if intr.hcint().bit_is_set() { Channel::on_host_ch_interrupt(); }

        if intr.discint().bit_is_set() {
            // TODO channels need to be signaled to fail.
            debug!("usb interrupt: disconnect");
            self.event.signal(Event::Disconnected);
        }

        if intr.ipxfr_incompisoout().bit_is_set() {
            debug!("usb interrupt: incomplete periodic tx");
        }
    }

    fn on_host_port_interrupt(&self) {
        self.otg_host.fs_hprt.modify(|r,w| {
            // Port detected?
            if r.pcdet().bit_is_set() {
                w.pcdet().set_bit();
                self.event.signal(Event::PortConnectDetected);
            }

            // Port enabled?
            if r.penchng().bit_is_set() {
                w.penchng().set_bit();

                // Not clearing the port enable bit makes the core to disable
                // the port immedately after .Took me hours to figure this out :(
                w.pena().clear_bit();

                if r.pena().bit_is_set() {
                    self.setup_port_speed();
                    self.event.signal(Event::PortEnabled);
                } else {
                    debug!("Port disabled");
                    Channel::disable_all();
                    self.event.signal(Event::Disconnected);
                }
            }

            // Port overcurrent?
            if r.pocchng().bit_is_set() {
                w.pocchng().set_bit();
                debug!("Port went overcurrent");
                self.event.signal(Event::Disconnected);
            }

            w
        });
    }

    fn setup_port_speed(&self) {
        unsafe {
            let port_speed = self.otg_host.fs_hprt.read().pspd().bits();
            let host_speed = self.otg_host.fs_hcfg.read().fslspcs().bits();

            if port_speed != host_speed {
                self.otg_host.fs_hcfg.modify(|_,w| w.fslspcs().bits(port_speed));
                self.reset_port();
            }

            let hfir = match port_speed {
                0b10 => 6000, // Low speed
                0b01 => 48_000, // Full speed
                v @ _ => panic!("Port speed not recognized: {}", v)
            };
            self.otg_host.hfir.write(|w| w.frivl().bits(hfir));
        }
    }

    fn reset_port(&self) {
        // Brings the D- D+ lines down to initiate a device reset
        debug!("Resetting port");
        self.otg_host.fs_hprt.modify(|_,w| w.prst().set_bit());
        // 10ms is the minimum by the USB specs. We add margins.
        delay_ms(20);
        self.otg_host.fs_hprt.modify(|_,w| w.prst().clear_bit());
    }

    async fn wait_for_event(&self, event: Event) -> UsbResult<()> {
        ensure!(self.event.wait().await == event);
        Ok(())
    }

    async fn run(&mut self) -> UsbResult<()> {
        self.wait_for_event(Event::PortConnectDetected).await?;
        debug!("Port detected");

        // Let the device boot. USB Specs say 200ms is enough, but some devices
        // can take longer apparently, so we'll wait a little longer.
        Timer::after(Duration::from_millis(300)).await;
        self.reset_port();
        self.wait_for_event(Event::PortEnabled).await?;
        debug!("Port enabled");
        Timer::after(Duration::from_millis(20)).await;

        let mut msc = enumerate::<Msc>().await?;
        msc.run().await
    }

    pub async fn main_loop(&mut self) {
        loop {
            self.init();
            if self.run().await.is_err() {
                debug!("USB Failed. Starting over");
                Timer::after(Duration::from_millis(2*10000)).await;
            }
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    PortConnectDetected,
    PortEnabled,
    Disconnected,
    TxComplete,
}
