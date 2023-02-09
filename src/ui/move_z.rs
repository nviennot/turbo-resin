// SPDX-License-Identifier: GPL-3.0-or-later

use futures::Future;
use lvgl::{
    style::State,
    widgets::*,
    prelude::*,
};
use alloc::format;

use lvgl::cstr_core::{CStr,CString};
use crate::{
    TaskRunner,
    drivers::zaxis::{
        self,
        prelude::*,
    }, util::CancellableTask,
};
use lvgl::core::Display;
use lvgl::core::Event;
use lvgl::core::InputDevice;
use lvgl::core::InputDeviceState;
use lvgl::core::Lvgl;
use lvgl::core::ObjExt;
use lvgl::core::Screen;
use lvgl::core::TouchPad;
use lvgl::style::Style;
use lvgl::{style::Align, style::Flag, style::GridAlign};
use lvgl::style;

pub struct MoveZ {
    style: Style,
    col_dsc: Box<[i16; 4]>,
    row_dsc: Box<[i16; 5]>,
    btn_0_1mm: Btn<MoveZ>,
    btn_1mm: Btn<MoveZ>,
    btn_10mm: Btn<MoveZ>,
    btn_up: Btn<MoveZ>,
    btn_home: Btn<MoveZ>,
    btn_down: Btn<MoveZ>,
    btn_stop: Btn<MoveZ>,
    current_pos: Label<MoveZ>,

    distence: f32,
    task_runner: &'static TaskRunner<Task>,
    zaxis: &'static zaxis::MotionControlAsync,

}

use alloc::boxed::Box;

impl MoveZ {
    pub fn new(
        screen: &mut Screen<Self>,
        task_runner: &'static mut TaskRunner<Task>,
        zaxis: &'static zaxis::MotionControlAsync,
    ) -> Self {

        let distence = 1.0;

        let mut style = Style::new();
        style.set_pad_all(10);

        screen.add_style(&mut style, 0);

        let mut col_dsc = Box::new([
            style::grid_free(1),
            style::grid_free(1),
            style::grid_free(1),
            style::grid_last(),
        ]);
        let mut row_dsc = Box::new([
            style::grid_free(1),
            style::grid_free(1),
            style::grid_free(1),
            style::grid_free(1),
            style::grid_last(),
        ]);

        screen.set_grid_dsc_array(col_dsc.as_mut_ptr(), row_dsc.as_mut_ptr());

        let btn_0_1mm = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {
                
                context.distence = 0.1;
                
            })
            .set_grid_cell(
                GridAlign::Stretch,
                0,
                1,
                GridAlign::Stretch,
                0,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("0.1mm").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let btn_1mm = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {
                
                context.distence = 1.0;
                
            })
            .add_state(State::CHECKED)
            .set_grid_cell(
                GridAlign::Stretch,
                1,
                1,
                GridAlign::Stretch,
                0,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("1mm").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let btn_10mm = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {
                
                context.distence = 10.0;
                
            })
            .set_grid_cell(
                GridAlign::Stretch,
                2,
                1,
                GridAlign::Stretch,
                0,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("10mm").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let btn_up = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {
                
                context.task_runner.enqueue_task(Task::MoveUp{steps:context.distence.mm()}).unwrap();                
                
            })
            .set_grid_cell(
                GridAlign::Stretch,
                0,
                1,
                GridAlign::Stretch,
                1,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("UP").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let btn_home = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {

                context.task_runner.enqueue_task(Task::MoveZero).unwrap();                

            })
            .set_grid_cell(
                GridAlign::Stretch,
                1,
                1,
                GridAlign::Stretch,
                1,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("HOME").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let btn_down = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {

                context.task_runner.enqueue_task(Task::MoveDown{steps:context.distence.mm()}).unwrap();                
                
            })
            .set_grid_cell(
                GridAlign::Stretch,
                2,
                1,
                GridAlign::Stretch,
                1,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("DOWN").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let btn_stop = Btn::new(screen).apply(|obj| {
            obj.on_event(Event::Clicked, |context| {

                context.task_runner.cancel_task();

            })
            .add_state(State::DISABLED)
            .set_grid_cell(
                GridAlign::Stretch,
                0,
                3,
                GridAlign::Stretch,
                2,
                1,
            );

            let mut btn_lbl = Label::new(obj);
            btn_lbl.set_text(CString::new("STOP").unwrap().as_c_str());
            btn_lbl.align_to(obj, Align::Center, 0, 0);
        });

        let current_pos = Label::new(screen).apply(|obj| {
            obj.set_text(CString::new("0.0").unwrap().as_c_str());
            obj.set_grid_cell(
                GridAlign::Center,
                0,
                3,
                GridAlign::Center,
                3,
                1,
            );
        });

        Self {
            style,
            col_dsc,
            row_dsc,
            btn_0_1mm,
            btn_1mm,
            btn_10mm,
            btn_up,
            btn_home,
            btn_down,
            btn_stop,
            current_pos,
            distence,
            task_runner,
            zaxis,
        }
    }
    pub fn refresh(&mut self) {

        //self.current_pos.set_text(CString::new(
        //    format!("Position: {:.2} mm\0", self.zaxis.get_current_position().as_mm()).as_bytes()
        //).unwrap().as_c_str());

        let c = self.task_runner.is_task_cancelled();
        // We could use get/set state instead?
        match self.task_runner.get_current_task() {
            Some(Task::MoveZero) => {
                self.btn_up.add_state(State::DISABLED);
                self.btn_home.add_state(State::DISABLED);
                self.btn_down.add_state(State::DISABLED);
                self.btn_stop.clear_state(State::DISABLED);
            },
            Some(Task::MoveUp {steps }) => {
                self.btn_up.add_state(State::DISABLED);
                self.btn_home.add_state(State::DISABLED);
                self.btn_down.add_state(State::DISABLED);
                self.btn_stop.clear_state(State::DISABLED);
            },
            Some(Task::MoveDown {steps }) => {
                self.btn_up.add_state(State::DISABLED);
                self.btn_home.add_state(State::DISABLED);
                self.btn_down.add_state(State::DISABLED);
                self.btn_stop.clear_state(State::DISABLED);                
            },
            None => {
                self.btn_up.clear_state(State::DISABLED);
                self.btn_home.clear_state(State::DISABLED);
                self.btn_down.clear_state(State::DISABLED);
                self.btn_stop.add_state(State::DISABLED);                
            }
        }

        match self.distence {
            x if x == 0.1 => {
                self.btn_0_1mm.add_state(State::CHECKED);
                self.btn_1mm.clear_state(State::CHECKED);
                self.btn_10mm.clear_state(State::CHECKED);
            },
            x if x == 1.0 => {
                self.btn_0_1mm.clear_state(State::CHECKED);
                self.btn_1mm.add_state(State::CHECKED);
                self.btn_10mm.clear_state(State::CHECKED);            
            },
            x if x == 10.0 => {
                self.btn_0_1mm.clear_state(State::CHECKED);
                self.btn_1mm.clear_state(State::CHECKED);
                self.btn_10mm.add_state(State::CHECKED);            
            },
            // SHould never happen
            _ => {
                self.btn_0_1mm.clear_state(State::DISABLED);
                self.btn_1mm.clear_state(State::DISABLED);
                self.btn_10mm.clear_state(State::DISABLED);                            
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Task {
    MoveUp { steps: Steps },
    MoveDown { steps: Steps },
    MoveZero,
}

impl CancellableTask for Task {
    type Context = zaxis::MotionControlAsync;

    type RunFuture<'a> = impl Future<Output = ()> + 'a where Self: 'a;
    type CancelFuture<'a> = impl Future<Output = ()> + 'a where Self: 'a;

    fn run<'a>(&'a self, mc: &'a mut zaxis::MotionControlAsync) -> Self::RunFuture<'a> {
        async move {
            match self {
                Self::MoveUp { steps } => mc.set_target_relative(*steps),
                Self::MoveDown { steps } => mc.set_target_relative(-*steps),
                Self::MoveZero => {
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

    fn cancel<'a>(&'a self, mc: &'a mut zaxis::MotionControlAsync) -> Self::CancelFuture<'a> {
        async move {
            // The task was cancelled
            mc.stop();
            mc.wait(zaxis::Event::Idle).await;
        }
    }
}
