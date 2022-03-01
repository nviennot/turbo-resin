// SPDX-License-Identifier: GPL-3.0-or-later

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(type_alias_impl_trait)]
#![feature(maybe_uninit_as_bytes)]
#![feature(maybe_uninit_uninit_array)]
#![feature(generic_const_exprs)]
#![allow(incomplete_features, unused_imports, dead_code, unused_variables, unused_macros, unreachable_code, unused_unsafe)]
#![feature(generic_associated_types)]
#![feature(core_intrinsics)]

extern crate alloc;

mod drivers;
mod consts;
mod ui;
mod util;
mod file_formats;

use core::cell::RefCell;
use core::mem::MaybeUninit;

use lvgl::core::{Lvgl, TouchPad, Display, ObjExt};

use embassy::{
    time::{Duration, Timer},
    util::Forever,
    executor::InterruptExecutor,
    interrupt::InterruptExt,
    blocking_mutex::CriticalSectionMutex as Mutex,
};
use embassy_stm32::{Config, interrupt};

use consts::display::*;
use drivers::{
    machine::Machine,
    touch_screen::{TouchEvent, TouchScreen},
    display::Display as RawDisplay,
    zaxis,
    usb::UsbHost, lcd::Lcd,
};
use util::TaskRunner;
use util::SharedWithInterrupt;
pub(crate) use runtime::debug;


static LAST_TOUCH_EVENT: Mutex<RefCell<Option<TouchEvent>>> = Mutex::new(RefCell::new(None));
static Z_AXIS: Forever<zaxis::MotionControlAsync> = Forever::new();
static USB_HOST: Forever<UsbHost> = Forever::new();
static TASK_RUNNER: Forever<TaskRunner<ui::Task>> = Forever::new();
static LCD: Forever<Lcd> = Forever::new();

mod maximum_priority_tasks {
    use super::*;

    #[interrupt]
    fn TIM7() {
        unsafe { Z_AXIS.steal().on_interrupt() }
    }
}

mod high_priority_tasks {
    use super::*;

    #[interrupt]
    fn OTG_FS() {
        unsafe { USB_HOST.steal().on_interrupt() }
    }
}

mod medium_priority_tasks {
    use embedded_sdmmc::Mode;

    use crate::drivers::usb::{Msc, UsbResult};

    use super::*;

    #[embassy::task]
    pub async fn usb_stack() {
        async fn usb_main(usb: &mut UsbHost) -> UsbResult<()> {
            let mut fs = usb.wait_for_device::<Msc>().await?
                .into_block_device().await?
                .into_fatfs_controller();

            //debug!("Disk initialized");
            let mut volume = fs.get_volume(embedded_sdmmc::VolumeIdx(0)).await.map_err(drop)?;

            //debug!("{:#?}", volume);
            let root = fs.open_root_dir(&volume).map_err(drop)?;
            //debug!("Root dir:");

            fs.iterate_dir(&volume, &root, |entry| {
                if !entry.attributes.is_hidden() {
                    if entry.attributes.is_directory() {
                        //debug!("  DIR  {}", entry.name);
                    }
                    if entry.attributes.is_archive() {
                        debug!("  FILE {} {} {:3} MB", entry.name, entry.mtime, entry.size/1024/1024);
                    }
                }
            }).await.map_err(drop)?;

            let mut file = fs.open_file_in_dir(&mut volume, &root, "RESINX~2.PWM", Mode::ReadOnly).await.map_err(drop)?;
            debug!("File open, size={}", file.length());

            use file_formats::photon::*;
            let layer_definition_offset = {
                let mut header = MaybeUninit::<Header>::uninit();
                fs.read(&volume, &mut file, unsafe { core::mem::transmute(header.as_bytes_mut()) } ).await.map_err(drop)?;
                let header = unsafe { header.assume_init() };
                header.layer_definition_offset
            };


            let num_layers = {
                file.seek_from_start(layer_definition_offset)?;
                let mut header = MaybeUninit::<LayerDefinition>::uninit();
                fs.read(&volume, &mut file, unsafe { core::mem::transmute(header.as_bytes_mut()) } ).await.map_err(drop)?;
                let header = unsafe { header.assume_init() };
                header.layer_count
            };

            debug!("Num layers: {}", num_layers);

            let layers_offset = layer_definition_offset + core::mem::size_of::<LayerDefinition>() as u32;

            let (data_offset, data_size) = {
                let layer_index = 7;
                file.seek_from_start(layers_offset + layer_index * core::mem::size_of::<Layer>() as u32)?;
                let mut header = MaybeUninit::<Layer>::uninit();
                fs.read(&volume, &mut file, unsafe { core::mem::transmute(header.as_bytes_mut()) } ).await.map_err(drop)?;
                let header = unsafe { header.assume_init() };
                (header.data_address, header.data_length)
            };

            debug!("Data address={:x}, size={:x}", data_offset, data_size);

            {
                file.seek_from_start(data_offset)?;

                const BUF_LEN: usize = 1024;
                let mut buffer = [0u8; BUF_LEN];
                let mut data_left = data_size as usize;

                let lcd = unsafe { LCD.steal() };

                lcd.draw_all_black();
                Timer::after(Duration::from_millis(2*5000)).await;

                let mut palette = [0xFF; 16];
                palette[0] = 0;
                lcd.set_palette(&palette);

                //lcd.draw_waves(8);
                lcd.start_draw();

                //let pixels_left = Lcd::ROWS * Lcd::COLS;
                let mut x = 0;
                let mut y = 0;
                let mut long_color_repeat: Option<(u8, u8)> = None;


                while data_left > 0 {
                    let buf = &mut buffer[0..data_left.min(BUF_LEN)];
                    data_left -= buf.len();
                    fs.read(&volume, &mut file, buf).await.map_err(drop)?;

                    // Some sort of RLE encoding
                    for b in buf {
                        let (color, repeat) = if let Some((color, repeat)) = long_color_repeat.take() {
                            let repeat = ((repeat as u16) << 8) | *b as u16;
                            (color, repeat)
                        } else {
                            let color = *b >> 4;
                            let repeat = *b & 0x0F;
                            if color == 0 || color == 0xF {
                                long_color_repeat = Some((color, repeat));
                                continue;
                            } else {
                                (color, repeat as u16)
                            }
                        };

                        for _ in 0..repeat {
                            let tile = ((3*x / Lcd::COLS)+1) + ((2*y / Lcd::ROWS)*3);
                            let color = if color > 0 { tile as u8 } else { color };

                            lcd.push_pixel(color);
                            x += 1;
                            if x == Lcd::COLS {
                                x = 0;
                                y += 1;
                            }
                        }


                    }

                }

                lcd.end_draw();
                debug!("Done drawing");
                Timer::after(Duration::from_millis(2*1000)).await;

                loop {
                    for i in 1..8 {
                        for j in 1..7 {
                            palette[j] = if i > j { 0xff } else { 0x00 };
                        }

                        lcd.set_palette(&palette);

                        if i == 1 {
                            Timer::after(Duration::from_millis(2*1000)).await;
                        }

                        Timer::after(Duration::from_millis(2*200)).await;
                    }
                    Timer::after(Duration::from_millis(2*1000)).await;
                }

            }




            Timer::after(Duration::from_secs(10000)).await;

            Ok(())
        }

        let usb_host = unsafe { USB_HOST.steal() };
        loop {
            let _ = usb_main(usb_host).await;
            Timer::after(Duration::from_millis(100)).await
        }
    }

    #[embassy::task]
    pub async fn touch_screen_task(mut touch_screen: TouchScreen) {
        loop {
            // What should happen if lvgl is not pumping click events fast enought?
            // We have different solutions. But here we go with last event wins.
            // If we had a keyboard, we would queue up events to avoid loosing key
            // presses.
            let touch_event = touch_screen.get_next_touch_event().await;
            LAST_TOUCH_EVENT.lock(|e| *e.borrow_mut() = touch_event);
        }
    }

    #[embassy::task]
    pub async fn lvgl_tick_task(mut lvgl_ticks: lvgl::core::Ticks) {
        loop {
            lvgl_ticks.inc(1);
            Timer::after(Duration::from_millis(1)).await
        }
    }

    #[embassy::task]
    pub async fn main_task() {
        let z_axis = unsafe { Z_AXIS.steal() };
        let task_runner = unsafe { TASK_RUNNER.steal() };
        task_runner.run_tasks(z_axis).await;
    }
}

mod low_priority_tasks {
    use crate::drivers::touch_screen;

    use super::*;

    pub fn idle_task(
        mut lvgl: Lvgl,
        mut display: Display<RawDisplay>,
    ) -> ! {
        let mut lvgl_input_device = lvgl::core::InputDevice::<TouchPad>::new(&mut display);

        let mut ui = ui::new_screen(&display, |screen| {
            let z_axis = unsafe { Z_AXIS.steal() };
            let task_runner = unsafe { TASK_RUNNER.steal() };
            ui::MoveZ::new(screen, task_runner, z_axis)
        });

        display.load_screen(&mut ui);

        loop {
            ui.context().as_mut().unwrap().refresh();

            LAST_TOUCH_EVENT.lock(|e| {
                *lvgl_input_device.state() = touch_screen::into_lvgl_event(&e.borrow());
            });

            lvgl.run_tasks();
            display.backlight.set_high();
        }
    }
}

fn lvgl_init(display: RawDisplay) -> (Lvgl, Display<RawDisplay>) {
    use embedded_graphics::pixelcolor::Rgb565;

    let mut lvgl = Lvgl::new();
    lvgl.register_logger(|s| rtt_target::rprint!(s));
    // Display init with its draw buffer
    static mut DRAW_BUFFER: [MaybeUninit<Rgb565>; LVGL_BUFFER_LEN] =
        [MaybeUninit::<Rgb565>::uninit(); LVGL_BUFFER_LEN];
    let display = Display::new(&lvgl, display, unsafe { &mut DRAW_BUFFER });
    (lvgl, display)
}

fn main() -> ! {
    rtt_target::rtt_init_print!(NoBlockSkip, 10240);

    let machine = {
        let p = {
            // We are doing the clock init here because of the gigadevice differences.
            let clk = crate::drivers::clock::setup_clock_120m_hxtal();
            let clk = crate::drivers::clock::embassy_stm32_clock_from(&clk);
            unsafe { embassy_stm32::rcc::set_freqs(clk) };

            // Note: TIM3 is taken for time accounting. It's configurable in Cargo.toml
            embassy_stm32::init(Config::default())
        };

        let cp = cortex_m::Peripherals::take().unwrap();
        Machine::new(cp, p)
    };

    Z_AXIS.put(zaxis::MotionControlAsync::new(
        SharedWithInterrupt::new(machine.stepper),
        machine.z_bottom_sensor,
    ));

    LCD.put(machine.lcd);

    let (lvgl, display) = lvgl_init(machine.display);

    USB_HOST.put(machine.usb_host);

    //let mut lcd = machine.lcd;
    //lcd.draw_waves(16);

    // Maximum priority for the motion control of the stepper motor.
    // as we need to deliver precise pulses with micro-second accuracy.
    {
        let irq: interrupt::TIM7 = unsafe { ::core::mem::transmute(()) };
        irq.set_priority(interrupt::Priority::P4);
        irq.unpend();
        irq.enable();
    }

    // High priority for the USB port
    {
        let irq: interrupt::OTG_FS = unsafe { ::core::mem::transmute(()) };
        irq.set_priority(interrupt::Priority::P5);
        //irq.unpend();
        irq.enable();
    }

    // Medium priority executor. It interrupts the low priority tasks (UI rendering)
    {
        let lvgl_ticks = lvgl.ticks();
        let touch_screen = machine.touch_screen;
        let irq = interrupt::take!(CAN1_RX0);
        irq.set_priority(interrupt::Priority::P6);
        static EXECUTOR_MEDIUM: Forever<InterruptExecutor<interrupt::CAN1_RX0>> = Forever::new();
        let executor = EXECUTOR_MEDIUM.put(InterruptExecutor::new(irq));
        executor.start(|spawner| {
            spawner.spawn(medium_priority_tasks::touch_screen_task(touch_screen)).unwrap();
            spawner.spawn(medium_priority_tasks::lvgl_tick_task(lvgl_ticks)).unwrap();
            spawner.spawn(medium_priority_tasks::main_task()).unwrap();
            spawner.spawn(medium_priority_tasks::usb_stack()).unwrap();
        });
    }

    // TODO release the stack

    // The idle task does UI drawing continuously.
    low_priority_tasks::idle_task(lvgl, display)
}

// Wrap main(), otherwise auto-completion with rust-analyzer doesn't work.
#[cortex_m_rt::entry]
fn main_() -> ! { main() }

pub mod runtime {
    use super::*;

    #[alloc_error_handler]
    fn oom(l: core::alloc::Layout) -> ! {
        panic!("Out of memory. Failed to allocate {} bytes", l.size());
    }

    pub fn print_stack_size() {
        debug!("stack size: {:x?}",  0x20000000 + 96*1024 - ((&mut [0u8;1]).as_ptr() as u32));
    }

    #[inline(never)]
    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        debug!("{}", info);
        loop {}
    }

    macro_rules! debug {
        ($($tt:tt)*) => {
            rtt_target::rprintln!($($tt)*)
        }
    }
    pub(crate) use debug;
}
