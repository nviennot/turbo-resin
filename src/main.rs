// SPDX-License-Identifier: GPL-3.0-or-later

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(type_alias_impl_trait)]
#![feature(maybe_uninit_as_bytes)]
#![feature(maybe_uninit_uninit_array)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(maybe_uninit_slice)]
#![feature(generic_const_exprs)]
#![feature(generic_associated_types)]
#![feature(core_intrinsics)]
#![feature(future_join)]
#![feature(const_maybe_uninit_uninit_array)]

#![allow(incomplete_features, unused_imports, dead_code, unused_variables, unused_macros, unreachable_code, unused_unsafe)]

extern crate alloc;

#[macro_use]
extern crate log;

mod drivers;
mod consts;
mod ui;
mod util;
mod file_formats;
mod logging;

use core::cell::RefCell;
use core::mem::MaybeUninit;

use lvgl::core::{Lvgl, TouchPad, Display, ObjExt};

use embassy_time::{Duration, Timer};
use embassy_util::{Forever, blocking_mutex::CriticalSectionMutex as Mutex};
use embassy_stm32::{
    Config,
    interrupt,
    interrupt::InterruptExt,
    executor::InterruptExecutor, time::Hertz,
};

use consts::display::*;
use drivers::{
    machine::Machine,
    touch_screen::{TouchEvent, TouchScreen},
    display::Display as RawDisplay,
    zaxis,
    usb::UsbHost, lcd::Lcd,
};

use crate::util::TaskRunner;


pub static Z_AXIS: Forever<zaxis::MotionControlAsync> = Forever::new();
static USB_HOST: Forever<UsbHost> = Forever::new();
pub static TASK_RUNNER: Forever<TaskRunner<ui::Task>> = Forever::new();
static LCD: Forever<Lcd> = Forever::new();

#[interrupt]
fn TIM7() {
    unsafe { Z_AXIS.steal().on_interrupt() }
}

#[interrupt]
fn OTG_FS() {
    unsafe { USB_HOST.steal().on_interrupt() }
}

mod medium_priority_tasks {
    use super::*;

    #[embassy_executor::task]
    pub async fn usb_stack() {
        let usb_host = unsafe { USB_HOST.steal() };
        /*
        loop {
            if let Err(e) = usb_main(usb_host).await {
            }
            let mut fs = usb.wait_for_filesystem().await?;
                warn!("File access failed: {:?}", e);
            }
            Timer::after(Duration::from_millis(100)).await
        }

        async fn usb_main(usb: &mut UsbHost) -> Result<(), embedded_sdmmc::Error<UsbError>> {
            usb_host.wait_for_filesystem().await?;

        }
        */
    }

    #[embassy_executor::task]
    pub async fn main_task() {
        let z_axis = unsafe { Z_AXIS.steal() };
        let task_runner = unsafe { TASK_RUNNER.steal() };
        task_runner.main_loop_task(z_axis).await;
    }
}

mod low_priority_tasks {
    use crate::drivers::touch_screen;

    use super::*;

}

#[cortex_m_rt::entry]
fn main() -> ! {
    logging::init_logging();

    let machine = {
        let p = drivers::clock::init();
        let cp = cortex_m::Peripherals::take().unwrap();
        Machine::new(cp, p)
    };

    #[cfg(feature="mono4k")]
    Z_AXIS.put(zaxis::MotionControlAsync::new(
        crate::util::SharedWithInterrupt::new(machine.stepper),
        machine.z_bottom_sensor,
    ));

    let (lvgl, display) = ui::lvgl_init(machine.display);

    USB_HOST.put(machine.usb_host);

    {
        let lcd = LCD.put(machine.lcd);
        lcd.init();
    }
    //debug!("FPGA version: {:x}", lcd.get_version());

    TASK_RUNNER.put(Default::default());

    // Maximum priority (P4)
    {
        // Maximum priority for the motion control of the stepper motor
        // as we need to deliver precise pulses with micro-second accuracy.
        let irq: interrupt::TIM7 = unsafe { ::core::mem::transmute(()) };
        irq.set_priority(interrupt::Priority::P4);
        irq.enable();
    }

    // High priority (P5)
    {
        // This is quick. It's to service the I/O between main memory and the USB IP-core FIFOs.
        // Maybe we don't need such high priority as we are the host and so we dictate the timing of things.
        let irq: interrupt::OTG_FS = unsafe { ::core::mem::transmute(()) };
        irq.set_priority(interrupt::Priority::P5);
        irq.enable();
    }

    // Medium priority (P6)
    {
        let irq = interrupt::take!(CAN1_RX0);
        irq.set_priority(interrupt::Priority::P6);

        let executor = {
            // Executors must live forever
            static EXECUTOR_MEDIUM: Forever<InterruptExecutor<interrupt::CAN1_RX0>> = Forever::new();
            let executor = EXECUTOR_MEDIUM.put(InterruptExecutor::new(irq));
            executor.start()
        };

        executor.must_spawn(ui::touch_screen_task(machine.touch_screen));
        executor.must_spawn(ui::lvgl_tick_task(lvgl.ticks()));
        executor.must_spawn(medium_priority_tasks::main_task());
        executor.must_spawn(medium_priority_tasks::usb_stack());

        //spawner.spawn(medium_priority_tasks::lcd_task(lcd_receiver)).unwrap();
    }

    // Idle task
    {
        // redraws the UI continuously.
        ui::idle_task(lvgl, display)
    }
}

/*
            let mut file = fs.open("TEST_P~1.CTB", Mode::ReadOnly).await?;

            use file_formats::ctb::*;
            let (layers_offset, num_layers, xor_key) = {
                let header = file.read_obj::<Header>().await?;
                (header.layers_offset, header.num_layers, header.xor_key)
            };

            debug!("Num layers: {}", num_layers);

            let lcd = unsafe { LCD.steal() };
            let start_cycles = read_cycles();
            //lcd.draw().set_all_black();

            for layer_index in 0..num_layers {
                // TODO Have proper errors
                file.seek_from_start(layers_offset + layer_index * core::mem::size_of::<Layer>() as u32).expect("bad file offset");
                let layer = file.read_obj::<Layer>().await?;
                //debug!("{:#?}", layer);

                {
                    let lcd = unsafe { LCD.steal() };
                    let start_cycles = read_cycles();
                    {

                        /*
                        lcd.draw().set_all_black();
                        lcd.draw().set_all_white();
                        //lcd.draw().gradient();
                        lcd.draw().waves(8, 100);
                        */

                        let mut lcd_drawing = lcd.draw();
                        layer.for_each_pixels(&mut file, layer_index, xor_key, |color, repeat| {
                            lcd_drawing.push_pixels(color, repeat);
                        }).await?;
                    }
                    let end_cycles = read_cycles();
                    debug!("Print drawing, took {}ms", end_cycles.wrapping_sub(start_cycles)/120_000);
                    Timer::after(Duration::from_secs(300)).await;
                }
            }

            Timer::after(Duration::from_secs(10000)).await;
            */


/*
// f(port, values)
fn iter_port_reg_changes(old_value: u32, new_value: u32, stride: u8, mut f: impl FnMut(u8, u8)) {
    let mut changes = old_value ^ new_value;
    let stride_mask = 0xFF >> (8 - stride);
    while changes != 0 {
        let right_most_bit = changes.trailing_zeros() as u8;
        let port = right_most_bit / stride;
        if port <= 16 {
            let v = (new_value >> (port*stride)) as u8 & stride_mask;
            f(port, v);
        }
        changes &= !(stride_mask as u32) << (port*stride);
    }
}
*/
