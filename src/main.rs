// SPDX-License-Identifier: GPL-3.0-or-later

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(int_abs_diff)]
#![allow(unused_imports, dead_code, unused_variables, unused_macros, unreachable_code)]

#![feature(core_intrinsics)]

mod drivers;
mod consts;
mod ui;

use alloc::format;
use lvgl::style::State;
use stm32f1xx_hal::pac::Interrupt;
use consts::system::*;
use consts::display::*;
use drivers::{
    machine::{Systick, Machine, prelude::*},
    touch_screen::{TouchScreenResult, TouchEvent},
    display::Display as RawDisplay,

    zaxis::{
        sensor::Sensor,
        stepper::Stepper,
    }
};

use embedded_graphics::pixelcolor::Rgb565;

use lvgl::core::{
    Lvgl, TouchPad, Display, InputDevice, ObjExt
};


pub(crate) use runtime::debug;

extern crate alloc;

use core::mem::MaybeUninit;

use lvgl::core::Screen;

mod runtime {
    use super::*;

    /*
    #[global_allocator]
    static ALLOCATOR: alloc_cortex_m::CortexMHeap = alloc_cortex_m::CortexMHeap::empty();

    pub fn init_heap() {
        // Using cortex_m_rt::heap_start() is bad. It doesn't tell us if our
        // HEAP_SIZE is too large and we will fault accessing non-existing RAM
        // Instead, we'll allocate a static buffer for our heap.
        unsafe {
            static mut HEAP: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
            ALLOCATOR.init((&mut HEAP).as_ptr() as usize, HEAP_SIZE);
        }
    }
    */

    #[alloc_error_handler]
    fn oom(l: core::alloc::Layout) -> ! {
        panic!("Out of memory. Failed to allocate {} bytes", l.size());
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

#[rtic::app(
    device = stm32f1xx_hal::stm32, peripherals = true,
    // Picked random interrupts that we'll never use. RTIC will use this to schedule tasks.
    dispatchers=[CAN_RX1, CAN_SCE, CAN2_RX0, CAN2_RX1]
)]
mod app {

    use super::*;

    #[monotonic(binds = SysTick, default = true)]
    type MonotonicClock = Systick;

    /* resources shared across RTIC tasks */
    #[shared]
    struct Shared {
        stepper: Stepper,
        #[lock_free]
        touch_screen: drivers::touch_screen::TouchScreen,
        last_touch_event: Option<TouchEvent>,
    }

    /* resources local to specific RTIC tasks */
    #[local]
    struct Local {
        lvgl: Lvgl,
        lvgl_ticks: lvgl::core::Ticks,
        lvgl_input_device: InputDevice::<TouchPad>,
        display: Display::<RawDisplay>,
        move_z_ui: Screen<ui::MoveZ>,
        lcd: drivers::lcd::Lcd,
        zsensor: Sensor,
    }

    fn lvgl_init(display: RawDisplay) -> (Lvgl, Display<RawDisplay>, InputDevice<TouchPad>) {
        let mut lvgl = Lvgl::new();

        // Register logger
        lvgl.register_logger(|s| rtt_target::rprint!(s));

        static mut DRAW_BUFFER: [MaybeUninit<Rgb565>; LVGL_BUFFER_LEN] =
            [MaybeUninit::<Rgb565>::uninit(); LVGL_BUFFER_LEN];

        let mut display = Display::new(&lvgl, display, unsafe { &mut DRAW_BUFFER });

        let input_device = lvgl::core::InputDevice::<TouchPad>::new(&mut display);

        (lvgl, display, input_device)
    }

    #[init]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_target::rtt_init_print!();
        debug!("Init...");

        lvgl::core::Lvgl::new();

        let machine = Machine::new(ctx.core, ctx.device);

        let display = machine.display;
        let systick = machine.systick;
        let stepper = machine.stepper;
        let touch_screen = machine.touch_screen;
        let lcd = machine.lcd;
        let zsensor = machine.zsensor;

        let (mut lvgl, mut display, lvgl_input_device) = lvgl_init(display);

        let lvgl_ticks = lvgl.ticks();
        lvgl_tick_task::spawn().unwrap();

        let last_touch_event = None;

        let mut move_z_ui = ui::MoveZ::new(&display);
        // Fill the display with something before turning it on.
        display.load_screen(&mut move_z_ui);
        lvgl.run_tasks();
        display.backlight.set_high();

        /*
        let ext_flash = machine.ext_flash;
        let delay = machine.delay;
        */

        debug!("Init complete");

        (
            Shared { stepper, touch_screen, last_touch_event },
            Local { lvgl, lvgl_ticks, lvgl_input_device, display, move_z_ui, lcd, zsensor },
            init::Monotonics(systick),
        )
    }

    #[task(priority = 5, binds = TIM7, shared = [stepper])]
    fn stepper_interrupt(mut ctx: stepper_interrupt::Context) {
        ctx.shared.stepper.lock(|s| s.on_interrupt());
    }

    #[task(priority = 3, local = [lvgl_ticks], shared = [])]
    fn lvgl_tick_task(ctx: lvgl_tick_task::Context) {
        // Not very precise (by the time we get here, some time has passed
        // already), but good enough
        lvgl_tick_task::spawn_after(1.millis()).unwrap();
        ctx.local.lvgl_ticks.inc(1);
    }

    #[task(priority = 2, binds = EXTI9_5, shared = [touch_screen])]
    fn touch_screen_pen_down_interrupt(ctx: touch_screen_pen_down_interrupt::Context) {
        use TouchScreenResult::*;
        match ctx.shared.touch_screen.on_pen_down_interrupt() {
            DelayMs(delay_ms) => {
                cortex_m::peripheral::NVIC::mask(Interrupt::EXTI9_5);
                touch_screen_sampling_task::spawn_after((delay_ms as u64).millis()).unwrap();
            }
            Done(None) => {},
            Done(Some(_)) => unreachable!(),
        }
    }

    #[task(priority = 2, local = [], shared = [touch_screen, last_touch_event])]
    fn touch_screen_sampling_task(mut ctx: touch_screen_sampling_task::Context) {
        use TouchScreenResult::*;
        match ctx.shared.touch_screen.on_delay_expired() {
            DelayMs(delay_ms) => {
                touch_screen_sampling_task::spawn_after((delay_ms as u64).millis()).unwrap();
            },
            Done(touch_event) => {
                ctx.shared.last_touch_event.lock(|t| *t = touch_event);
                unsafe { cortex_m::peripheral::NVIC::unmask(Interrupt::EXTI9_5); }
            },
        }
    }

    #[idle(local = [lvgl, lvgl_input_device, display, move_z_ui, lcd, zsensor], shared = [last_touch_event, stepper])]
    fn idle(mut ctx: idle::Context) -> ! {
        let lvgl = ctx.local.lvgl;
        let lvgl_input_device = ctx.local.lvgl_input_device;
        let zsensor = ctx.local.zsensor;
        let move_z_ui = ctx.local.move_z_ui.context().as_mut().unwrap();

        loop {
            ctx.shared.last_touch_event.lock(|e| {
                *lvgl_input_device.state() = if let Some(ref e) = e {
                    TouchPad::Pressed { x: e.x as i16, y: e.y as i16 }
                } else {
                    TouchPad::Released
                };
            });

            move_z_ui.update(&mut ctx.shared.stepper, zsensor);
            lvgl.run_tasks();
        }
    }
}


    /*
    fn draw_touch_event(display: &mut Display, touch_event: Option<&TouchEvent>) {
        use embedded_graphics::{prelude::*, primitives::{Circle, PrimitiveStyle}, pixelcolor::Rgb565};

        if let Some(touch_event) = touch_event {
            Circle::new(Point::new(touch_event.x as i32, touch_event.y as i32), 3)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
                .draw(display).unwrap();
        }
    }
*/
