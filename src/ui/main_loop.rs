// SPDX-License-Identifier: GPL-3.0-or-later

use crate::drivers::touch_screen;
use embassy_time::{Timer, Duration};
use embassy_util::{Forever, blocking_mutex::CriticalSectionMutex as Mutex};
use core::cell::RefCell;
use core::mem::MaybeUninit;

use crate::consts::display::*;
use crate::drivers::{
    touch_screen::{TouchEvent, TouchScreen},
    display::Display as RawDisplay,
};
use lvgl::core::{Lvgl, TouchPad, Display, ObjExt, Screen};

static LAST_TOUCH_EVENT: Mutex<RefCell<Option<TouchEvent>>> = Mutex::new(RefCell::new(None));

pub fn lvgl_init(display: RawDisplay) -> (Lvgl, Display<RawDisplay>) {
    use embedded_graphics::pixelcolor::Rgb565;

    let mut lvgl = Lvgl::new();
    lvgl.register_logger(|s| rtt_target::rprint!(s));
    // Display init with its draw buffer
    static mut DRAW_BUFFER: [MaybeUninit<Rgb565>; LVGL_BUFFER_LEN] =
        [MaybeUninit::<Rgb565>::uninit(); LVGL_BUFFER_LEN];
    let display = Display::new(&lvgl, display, unsafe { &mut DRAW_BUFFER });
    (lvgl, display)
}


#[embassy_executor::task]
pub async fn lvgl_tick_task(mut lvgl_ticks: lvgl::core::Ticks) {
    loop {
        lvgl_ticks.inc(1);
        Timer::after(Duration::from_millis(1)).await
    }
}

#[embassy_executor::task]
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
pub fn idle_task(
    mut lvgl: Lvgl,
    mut display: Display<RawDisplay>,
) -> ! {
    let mut lvgl_input_device = lvgl::core::InputDevice::<TouchPad>::new(&mut display);

    let mut ui = new_screen(&display, |screen| {
        let z_axis = unsafe { crate::Z_AXIS.steal() };
        let task_runner = unsafe { crate::TASK_RUNNER.steal() };
        super::MoveZ::new(screen, task_runner, z_axis)
    });

    display.load_screen(&mut ui);

    loop {
        ui.context().as_mut().unwrap().refresh();

        LAST_TOUCH_EVENT.lock(|e| {
            *lvgl_input_device.state() = touch_screen::into_lvgl_event(&e.borrow());
        });

        lvgl.run_tasks();
        display.set_backlight(true);
    }
}

pub fn new_screen<D,C>(display: &Display<D>, init_f: impl FnOnce(&mut Screen::<C>) -> C) -> Screen::<C> {
    let mut screen = Screen::<C>::new(display);
    let context = init_f(&mut screen);
    screen.context().replace(context);
    screen
}
