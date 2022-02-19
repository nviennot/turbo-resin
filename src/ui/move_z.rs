// SPDX-License-Identifier: GPL-3.0-or-later

use lvgl::{
    style::State,
    core::Screen,
    widgets::*,
    prelude::*,
};
use alloc::format;

use lvgl::cstr_core::CStr;
use crate::{
    TaskRunner,
    drivers::zaxis::{
        self,
        prelude::*,
    },
};
use crate::consts::zaxis::motion_control::*;

pub struct MoveZ {
    btn_move_up: Btn<Self>,
    btn_move_down: Btn<Self>,
    speed_slider: Slider<Self>,
    speed_label: Label<Self>,
    position_label: Label<Self>,
    btn_move_zero: Btn<Self>,

    task_runner: &'static TaskRunner<Task>,
    zaxis: &'static zaxis::MotionControlAsync,
}

impl MoveZ {
    pub fn new(
        screen: &mut Screen::<Self>,
        task_runner: &'static mut TaskRunner<Task>,
        zaxis: &'static zaxis::MotionControlAsync,
    ) -> Self {
        use lvgl::widgets::*;
        use lvgl::style::*;
        use lvgl::core::*;

        let spacing = 12;

        let btn_move_up = Btn::new(screen).apply(|obj| {
            Label::new(obj)
                .set_text(&CStr::from_bytes_with_nul(b"Move Up\0").unwrap());
            obj
            .align_to(screen, Align::TopMid, 0, 2*spacing)
            .add_flag(Flag::CHECKABLE)
            .on_event(Event::Clicked, |context| {
                let checked = context.btn_move_up.has_state(State::CHECKED);
                if checked {
                    context.task_runner.enqueue_task(Task::MoveUp).unwrap();
                } else {
                    context.task_runner.cancel_task();
                }
            });
        });

        let btn_move_down = Btn::new(screen).apply(|obj| {
            Label::new(obj)
                .set_text(&CStr::from_bytes_with_nul(b"Move Down\0").unwrap());
            obj
            .align_to(&btn_move_up, Align::OutBottomMid, 0, spacing)
            .add_flag(Flag::CHECKABLE)
            .on_event(Event::Clicked, |context| {
                let checked = context.btn_move_down.has_state(State::CHECKED);
                if checked {
                    context.task_runner.enqueue_task(Task::MoveDown).unwrap();
                } else {
                    context.task_runner.cancel_task();
                }
            });
        });

        let speed_slider = Slider::new(screen).apply(|obj| { obj
            .align_to(&btn_move_down, Align::OutBottomMid, 0, 2*spacing)
            .set_range(1500, 10_000)
            .set_value(10_000, 0)
            .on_event(Event::ValueChanged, |context| {
                let value = unsafe { lvgl::sys::lv_slider_get_value(context.speed_slider.raw) };

                let value = (value as f32)/10000.0;
                let value = value*value*value;
                let value = value * MAX_SPEED;

                context.zaxis.set_max_speed(value.mm());
            });
        });

        let speed_label = Label::new(screen).apply(|obj| { obj
            .align_to(&speed_slider, Align::OutBottomLeft, 50, spacing);
        });

        let position_label = Label::new(screen).apply(|obj| { obj
            .align_to(&speed_label, Align::OutBottomLeft, 0, 0);
        });

        let btn_move_zero = Btn::new(screen).apply(|obj| {
            Label::new(obj)
                .set_text(&CStr::from_bytes_with_nul(b"Move to Z=0\0").unwrap());
            obj
            .align_to(&position_label, Align::OutBottomMid, 0, spacing)
            .add_flag(Flag::CHECKABLE)
            .on_event(Event::Clicked, |context| {
                let checked = context.btn_move_zero.has_state(State::CHECKED);
                if checked {
                    context.task_runner.enqueue_task(Task::MoveZero).unwrap();
                } else {
                    context.task_runner.cancel_task();
                }
            });
        });

        Label::new(screen).apply(|obj| { obj
            .set_text(&CStr::from_bytes_with_nul(b"Turbo Resin v0.1.3\0").unwrap())
            .align_to(screen, Align::BottomRight, -5, -5);
        });

        Self {
            btn_move_up, btn_move_down, btn_move_zero,
            speed_label, position_label, speed_slider,
            task_runner, zaxis,
        }
    }

    // Called before every frame rendering.
    pub fn refresh(&mut self) {
        let c = self.task_runner.is_task_cancelled();
        // We could use get/set state instead?
        match self.task_runner.get_current_task().cloned() {
            Some(Task::MoveUp) => {
                if c { self.btn_move_up.add_state(State::DISABLED); }
                self.btn_move_down.add_state(State::DISABLED);
                self.btn_move_zero.add_state(State::DISABLED);
            },
            Some(Task::MoveDown) => {
                self.btn_move_up.add_state(State::DISABLED);
                if c { self.btn_move_down.add_state(State::DISABLED); }
                self.btn_move_zero.add_state(State::DISABLED);
            },
            Some(Task::MoveZero) => {
                self.btn_move_up.add_state(State::DISABLED);
                self.btn_move_down.add_state(State::DISABLED);
                if c { self.btn_move_zero.add_state(State::DISABLED); }
            },
            None => {
                self.btn_move_up.clear_state(State::CHECKED | State::DISABLED);
                self.btn_move_down.clear_state(State::CHECKED | State::DISABLED);
                self.btn_move_zero.clear_state(State::CHECKED | State::DISABLED);
            }
        }

        // set_text() makes a copy of the string internally.
        self.position_label.set_text(&CStr::from_bytes_with_nul(
            format!("Position: {:.2} mm\0", self.zaxis.get_current_position().as_mm()).as_bytes()
        ).unwrap());

        self.speed_label.set_text(&CStr::from_bytes_with_nul(
            format!("Max speed: {:.2} mm/s\0", self.zaxis.get_max_speed().as_mm()).as_bytes()
        ).unwrap());
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Task {
    MoveUp,
    MoveDown,
    MoveZero,
}

impl Task {
    pub async fn run(&self, task_runner: &TaskRunner<Self>, mc: &mut zaxis::MotionControlAsync) {
        let f = self.run_inner(mc);
        if task_runner.cancellable(f).await.is_err() {
            // The task was cancelled
            mc.stop();
            mc.wait(zaxis::Event::Idle).await;
        }
    }

    async fn run_inner(&self, mc: &mut zaxis::MotionControlAsync) {
        use Task::*;
        match self {
            MoveUp => mc.set_target_relative(40.0.mm()),
            MoveDown => mc.set_target_relative((-40.0).mm()),
            MoveZero => {
                let s = mc.get_max_speed();
                zaxis::calibrate_origin(mc, None).await;
                // FIXME we don't restore the original speed when the task is cancelled.
                mc.set_max_speed(s);
                mc.set_target(0.0.mm());
            }
        };
        mc.wait(zaxis::Event::Idle).await;
    }

}
